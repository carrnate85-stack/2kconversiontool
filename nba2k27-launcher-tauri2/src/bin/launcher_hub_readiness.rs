//! Read-only development bridge. No UI initialization, game access or mutations.
use std::{fs, io::{Read, Write}, path::Path, os::windows::fs::OpenOptionsExt};
use sha2::{Digest, Sha256};
use serde::Deserialize;
use windows::{core::PWSTR, Win32::{Foundation::CloseHandle, System::{Diagnostics::ToolHelp::*, Threading::*}}};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { schema:u32, transaction:String, expected_exe_sha256:String, location:String }
fn run() -> anyhow::Result<serde_json::Value> {
    let mut bytes=Vec::new();std::io::stdin().take(4097).read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len()<=4096,"Handoff request too large");
    let req:Request=serde_json::from_slice(&bytes)?;
    for value in [&req.transaction,&req.expected_exe_sha256] {anyhow::ensure!(value.len()==64 && value.bytes().all(|b|b.is_ascii_hexdigit()),"Invalid handoff digest");}
    anyhow::ensure!(req.schema==1,"Unsupported handoff schema");
    anyhow::ensure!(matches!(req.location.as_str(),"App"|"PreviousApp"|"DisplacedApp"),"Unsupported sandbox readiness location");
    let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../2k-tools-hub/artifacts/update-integration/sandbox");
    for p in root.ancestors() {use std::os::windows::fs::MetadataExt; anyhow::ensure!(fs::symlink_metadata(p)?.file_attributes()&0x400==0,"Reparse point refused");}
    // Fail closed for any game, EAC, or launcher process. No termination.
    unsafe {
        let snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0)?;
        let outcome=(|| -> anyhow::Result<()> {
            let mut row=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>() as u32,..Default::default()};
            Process32FirstW(snapshot,&mut row)?;
            loop {
                let n=String::from_utf16_lossy(&row.szExeFile[..row.szExeFile.iter().position(|c|*c==0).unwrap_or(row.szExeFile.len())]).to_lowercase();
                anyhow::ensure!(!(n=="nba2k27.exe" || n.starts_with("easyanticheat") || n=="start_protected_game.exe" || n=="eaclaunch.exe"),"Close the game normally before the update handoff");
                if n=="launcher.exe" || n.starts_with("2k launcher") || n.starts_with("nba2k27-launcher") || n.starts_with("nba 2k27 launcher") {
                    let process=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,row.th32ProcessID)?;
                    let mut path=vec![0u16;32768];let mut size=path.len() as u32;
                    let queried=QueryFullProcessImageNameW(process,PROCESS_NAME_WIN32,PWSTR(path.as_mut_ptr()),&mut size);
                    let _=CloseHandle(process);queried?;
                    let image=fs::canonicalize(String::from_utf16_lossy(&path[..size as usize]))?;
                    let target=fs::canonicalize(&root)?;
                    anyhow::ensure!(!image.starts_with(target),"Close the sandbox launcher normally before the update handoff");
                }
                match Process32NextW(snapshot,&mut row) { Ok(())=>{}, Err(e) if e.code()==windows::Win32::Foundation::ERROR_NO_MORE_FILES.to_hresult()=>break, Err(e)=>return Err(e.into()) }
            }
            Ok(())
        })();
        let _=CloseHandle(snapshot);outcome?;
    }
    let path=root.join(&req.location).join("launcher.exe");
    for p in path.ancestors(){use std::os::windows::fs::MetadataExt;anyhow::ensure!(fs::symlink_metadata(p)?.file_attributes()&0x400==0,"Reparse point refused");}
    let mut file=fs::OpenOptions::new().read(true).share_mode(0).open(&path)?;
    let mut hash=Sha256::new();let mut buf=[0u8;65536];
    loop {let n=file.read(&mut buf)?;if n==0{break}hash.update(&buf[..n]);}
    let actual=format!("{:x}",hash.finalize());
    anyhow::ensure!(actual.eq_ignore_ascii_case(&req.expected_exe_sha256),"Launcher changed before handoff");
    Ok(serde_json::json!({"schema":1,"ready":true,"transaction":req.transaction,"exeSha256":actual,"sandboxOnly":true,"gameAccess":false}))
}
fn main(){let r=run();let output=match &r{Ok(v)=>v.clone(),Err(e)=>serde_json::json!({"error":format!("{e:#}")})};let _=writeln!(std::io::stdout(),"{output}");std::process::exit(if r.is_ok(){0}else{1});}
