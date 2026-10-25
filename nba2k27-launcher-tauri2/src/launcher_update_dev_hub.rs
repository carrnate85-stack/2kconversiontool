//! Fixed-path dev bridge; public update authentication remains disabled.
use super::*;
use base64::{Engine,engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature,VerifyingKey};
use std::os::windows::fs::{MetadataExt,OpenOptionsExt};
const DOMAIN:&[u8]=b"2K-HUB-LAUNCHER-PACKAGE-V1\0";
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Envelope {key_id:String,payload:String,signature:String}
fn ordinary(p:&Path)->Result<()> {
    for a in p.ancestors(){match fs::symlink_metadata(a){
        Ok(m)=>ensure!(m.file_attributes()&0x400==0,"Reparse point refused: {}",a.display()),
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())
    }} Ok(())
}
fn bounded(p:&Path)->Result<Vec<u8>> {ordinary(p)?;let mut b=vec![];fs::File::open(p)?.take(8*1024*1024+1).read_to_end(&mut b)?;ensure!(b.len()<=8*1024*1024,"Metadata too large");Ok(b)}
fn signed(p:&Path)->Result<Manifest> {
    let e:Envelope=serde_json::from_slice(&bounded(p)?)?;
    ensure!(e.key_id=="hub-local-dev-20261024","Unknown dev signing key");
    let hex=include_str!("../../2k-tools-hub/dev-update-config/public-key.hex").trim();
    let key:Vec<u8>=(0..hex.len()).step_by(2).map(|i|u8::from_str_radix(&hex[i..i+2],16)).collect::<std::result::Result<_,_>>()?;
    let key=VerifyingKey::from_bytes(&key.try_into().map_err(|_|anyhow::anyhow!("Invalid public key"))?)?;
    let payload=STANDARD.decode(e.payload)?;let sig=Signature::from_slice(&STANDARD.decode(e.signature)?)?;
    let mut message=DOMAIN.to_vec();message.extend_from_slice(&payload);key.verify_strict(&message,&sig)?;
    let m:Manifest=serde_json::from_slice(&payload)?;validate_manifest(&m)?;
    ensure!(m.channel=="QA","Only QA packages allowed in the dev worker");Ok(m)
}
fn idle(root:&Path)->Result<()> {
    let root=fs::canonicalize(root)?.to_string_lossy().to_lowercase();
    for p in crate::session_safety::processes()? {
        let n=p.name.to_lowercase();
        ensure!(!(n=="nba2k27.exe"||n.starts_with("easyanticheat")||n=="start_protected_game.exe"||n=="eaclaunch.exe"),"Close the game normally before updating");
        if p.pid==std::process::id(){continue}
        if let Some(i)=p.identity {
            let image=fs::canonicalize(&i.path).unwrap_or_else(|_|PathBuf::from(&i.path)).to_string_lossy().to_lowercase();
            ensure!(!image.starts_with(&(root.clone()+"\\")),"Dev-copy process is running: {}",p.name);
        }else if n.contains("launcher") {bail!("Cannot establish launcher process identity: {}",p.name)}
    } Ok(())
}
fn durable_copy(from:&Path,to:&Path,m:&Manifest)->Result<()> {
    ensure!(!to.try_exists()?,"Staging already exists; recover or retain it before retrying");
    copy_tree(from,to,&m.files)?;
    for f in &m.files {fs::OpenOptions::new().write(true).open(to.join(&f.path))?.sync_all()?;}
    write_json(&to.join(MANIFEST),m)?;verify(to,m,&|_|{})
}
#[cfg(feature="dev-hub-update-fixtures")]
pub(super) fn interrupt(point:&str){if std::env::var("HUB_NATIVE_INTERRUPT").ok().as_deref()==Some(point){std::process::exit(86)}}
pub(crate) fn run(action:&str)->Result<Value> {
    ensure!(["check","apply","rollback","recover","permissions"].contains(&action),"Unknown installed-copy action");
    let base=Path::new(env!("CARGO_MANIFEST_DIR")).join("../2k-tools-hub/artifacts/update-integration");
    let root=base.join("installed-layout-dev");ordinary(&root)?;
    let root=fs::canonicalize(root)?;let l=locations(&root);
    crate::hub_shutdown::permissions().context("Installation permission/lock preflight failed; no automatic elevation or ACL changes attempted")?;
    for p in [&l.app,&l.updates,&l.staged,&l.previous,&l.displaced,&l.journal]{ordinary(p)?;}
    if action=="permissions" {return Ok(json!({"devOnly":true,"action":action,"writable":true,"automaticElevation":false,"message":"Required App/Recovery access checks passed; no elevation requested."}));}
    let lock=root.join("update.lock");ordinary(&lock)?;
    let _lease=fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).share_mode(0).open(lock)?;
    if action!="apply"&&action!="rollback" {idle(&root)?;}
    let authorization=l.updates.join("authorization.json");ordinary(&authorization)?;
    if action=="recover" {
        if l.journal.try_exists()? {
            // Authentication is independent of the mutable transaction journal.
            let expected=signed(&authorization)?;
            let j:Journal=serde_json::from_slice(&bounded(&l.journal)?)?;
            ensure!(j.incoming.as_ref().is_some_and(|s|s.manifest==expected),"Recovery incoming identity is not authorized");
        }
        let recovered=recover(&l)?;
        if l.staged.try_exists()? {
            let m=verify_dir(&l.staged)?;ensure!(m==signed(&authorization)?,"Orphan staging is not authorized");
            let retained=l.updates.join(format!("Retained-{}",crate::backend::unique_id()));rename(&l.staged,&retained)?;
        }
        return Ok(json!({"devOnly":true,"action":action,"recovered":recovered,"currentVersion":verify_dir(&l.app)?.version}));
    }
    ensure!(!l.journal.try_exists()?&&!l.staged.try_exists()?&&!l.displaced.try_exists()?,"Interrupted update needs recovery; nothing changed");
    let current=verify_dir(&l.app)?;
    let candidate=base.join("installed-package-dev/App");ordinary(&candidate)?;
    let envelope=base.join("installed-package-dev/authorization.json");
    let incoming=if action=="rollback" {verify_dir(&l.previous)?}else{let m=signed(&envelope)?;verify(&candidate,&m,&|_|{})?;ensure!(read_manifest(&candidate)?.as_ref()==Some(&m),"Candidate manifest differs from signed payload");version_policy(&current,&m)?;m};
    if action=="check"{return Ok(json!({"devOnly":true,"action":action,"currentVersion":current.version,"version":incoming.version,"files":incoming.files.len(),"signatureVerified":true}));}
    if action=="apply" {
        ensure!(!l.previous.try_exists()?,"Retained rollback copy exists; repeat-update retention policy must be reviewed first");
        close_for_update(&root,&l.app.join(current.exe().context("Missing launcher EXE")?))?;
        write_json(&authorization,&serde_json::from_slice::<Value>(&bounded(&envelope)?)?)?;
        durable_copy(&candidate,&l.staged,&incoming)?;
        ensure!(verify_dir(&l.staged)?==signed(&authorization)?,"Staged authorization changed");
        idle(&root)?;apply_transaction_checked(&l,None)?;
    }else{
        // Fixture baseline has its own signed native manifest; no unsigned downgrade.
        let baseline=base.join("installed-baseline-authorization.json");
        ensure!(signed(&baseline)?==incoming,"Rollback does not match signed baseline");
        close_for_update(&root,&l.app.join(current.exe().context("Missing launcher EXE")?))?;
        let expected=snapshot(&l.previous)?;
        write_json(&authorization,&serde_json::from_slice::<Value>(&bounded(&baseline)?)?)?;
        durable_copy(&l.previous,&l.staged,&incoming)?;
        idle(&root)?;apply_transaction_checked(&l,Some(&expected))?;
    }
    Ok(json!({"devOnly":true,"action":action,"currentVersion":verify_dir(&l.app)?.version,"previousVersion":verify_dir(&l.previous)?.version,"nativeTransaction":true}))
}

fn close_for_update(root:&Path,exe:&Path)->Result<()> {
    let target=fs::canonicalize(exe)?;
    let rows=crate::session_safety::processes()?;
    for p in &rows {
        let n=p.name.to_lowercase();
        ensure!(!(n=="nba2k27.exe"||n.starts_with("easyanticheat")||n=="start_protected_game.exe"||n=="eaclaunch.exe"),"Close the game normally before updating");
    }
    let mut candidates=vec![];
    for p in rows {if let Some(i)=p.identity {if fs::canonicalize(i.path).ok().as_ref()==Some(&target){candidates.push(p.pid)}}}
    ensure!(candidates.len()<=1,"Multiple target launcher processes; close them normally");
    if let Some(pid)=candidates.first(){crate::hub_shutdown::request(*pid,exe)?;}
    idle(root)?;crate::hub_shutdown::permissions()?;Ok(())
}
