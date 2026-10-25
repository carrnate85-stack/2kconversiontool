//! Fixed-root, development-only cooperative shutdown. No forced termination/UAC.
use anyhow::{Context,Result,ensure,bail};
use serde::{Serialize,Deserialize};
use sha2::{Digest,Sha256};
use std::{fs,io::Read,path::{Path,PathBuf},time::{Duration,Instant,SystemTime,UNIX_EPOCH},os::windows::{ffi::OsStrExt,fs::MetadataExt}};
use windows::{core::{PCWSTR,PWSTR},Win32::{Foundation::*,Storage::FileSystem::*,System::Threading::*,Security::Cryptography::*}};
pub fn root()->PathBuf {Path::new(env!("CARGO_MANIFEST_DIR")).join("../2k-tools-hub/artifacts/update-integration/installed-layout-dev")}
pub fn worker()->PathBuf {Path::new(env!("CARGO_MANIFEST_DIR")).join("target-qa124/release/launcher-hub-update-worker.exe")}
pub fn ordinary(p:&Path)->Result<()> {for a in p.ancestors(){match fs::symlink_metadata(a){Ok(m)=>ensure!(m.file_attributes()&0x400==0,"Reparse path refused"),Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(e.into())}}Ok(())}
pub fn hash(p:&Path)->Result<String>{ordinary(p)?;let mut f=fs::File::open(p)?;let mut h=Sha256::new();let mut b=[0;65536];loop{let n=f.read(&mut b)?;if n==0{break}h.update(&b[..n]);}Ok(format!("{:x}",h.finalize()))}
fn now()->u64 {SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub struct Identity {pid:u32,started:u64,path:PathBuf}
struct Handle(HANDLE);impl Drop for Handle{fn drop(&mut self){unsafe{let _=CloseHandle(self.0);}}}
fn identity(h:HANDLE,pid:u32)->Result<Identity>{unsafe{
 let mut path=vec![0u16;32768];let mut size=path.len() as u32;
 QueryFullProcessImageNameW(h,PROCESS_NAME_WIN32,PWSTR(path.as_mut_ptr()),&mut size)?;
 let(mut a,mut b,mut c,mut d)=(FILETIME::default(),FILETIME::default(),FILETIME::default(),FILETIME::default());GetProcessTimes(h,&mut a,&mut b,&mut c,&mut d)?;
 Ok(Identity{pid,started:((a.dwHighDateTime as u64)<<32)|a.dwLowDateTime as u64,path:fs::canonicalize(String::from_utf16_lossy(&path[..size as usize]))?})
}}
fn open(pid:u32)->Result<Handle>{Ok(Handle(unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE,false,pid)?}))}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {schema:u32,nonce:String,created:u64,sender:Identity,target:Identity,target_sha256:String}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {schema:u32,nonce:String,target:Identity,status:String}
fn read<T:serde::de::DeserializeOwned>(p:&Path)->Result<T>{ordinary(p)?;let mut b=vec![];fs::File::open(p)?.take(16385).read_to_end(&mut b)?;ensure!(b.len()<=16384,"Handoff metadata exceeds bounds");Ok(serde_json::from_slice(&b)?)}
fn request_dir(nonce:&str)->PathBuf{root().join("Handoff").join(nonce)}
pub fn finish(r:&Request,status:&str)->Result<()> {
 ensure!(["cleaned","busy"].contains(&status),"Unknown shutdown state");
 let p=request_dir(&r.nonce).join("response.json");ordinary(&p)?;
 crate::backend::atomic_json(&p,&Response{schema:1,nonce:r.nonce.clone(),target:r.target.clone(),status:status.into()})
}
/// Called only by an App executable in the fixed dev copy. A matching worker
/// identity and pinned executable hash are required before any close callback.
pub fn poll(worker_sha:&str)->Result<Option<Request>> {
 let me=open(std::process::id())?;let target=identity(me.0,std::process::id())?;
 let app=fs::canonicalize(root().join("App"))?;
 ensure!(target.path.parent()==Some(app.as_path()),"Shutdown listener is outside the dev App");
 let folder=root().join("Handoff");ordinary(&folder)?;if !folder.try_exists()?{return Ok(None)}
 for e in fs::read_dir(folder)?.take(256){let p=e?.path();ordinary(&p)?;
  if p.join("response.json").try_exists()?{continue}
  let r:Request=match read(&p.join("request.json")){Ok(r)=>r,Err(_)=>continue};
  if r.schema!=1||r.nonce.len()!=64||!r.nonce.bytes().all(|b|b.is_ascii_hexdigit())||p.file_name().and_then(|s|s.to_str())!=Some(r.nonce.as_str())||r.created>now()||now()-r.created>25_000||r.target!=target {continue}
  let sender=match open(r.sender.pid){Ok(s)=>s,Err(_)=>continue};
  if identity(sender.0,r.sender.pid)?!=r.sender||r.sender.path!=fs::canonicalize(worker())?||hash(&r.sender.path)?!=worker_sha.trim(){continue}
  if hash(&target.path)?!=r.target_sha256 {continue}
  return Ok(Some(r));
 }Ok(None)
}
pub fn request(pid:u32,exe:&Path)->Result<()> {
 ordinary(exe)?;let held=open(pid)?;let target=identity(held.0,pid)?;
 ensure!(target.path==fs::canonicalize(exe)?,"Target launcher identity changed");
 let me=open(std::process::id())?;let sender=identity(me.0,std::process::id())?;
 ensure!(sender.path==fs::canonicalize(worker())?,"Shutdown requester is not the fixed dev worker");
 let mut random=[0u8;32];unsafe{BCryptGenRandom(None,&mut random,BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;}
 let nonce=random.iter().map(|b|format!("{b:02x}")).collect::<String>();
 let r=Request{schema:1,nonce:nonce.clone(),created:now(),sender,target,target_sha256:hash(exe)?};
 let dir=request_dir(&nonce);ordinary(&dir)?;fs::create_dir_all(&dir)?;
 crate::backend::atomic_json(&dir.join("request.json"),&r)?;
 let deadline=Instant::now()+Duration::from_secs(20);
 loop {
  let response=dir.join("response.json");
  if response.try_exists()?{
   let a:Response=read(&response)?;
   ensure!(a.schema==1&&a.nonce==r.nonce&&a.target==r.target,"Shutdown acknowledgment identity mismatch");
   ensure!(a.status=="cleaned","Launcher declined shutdown while busy; update held");
   if unsafe{WaitForSingleObject(held.0,0)}==WAIT_OBJECT_0 {return Ok(())}
  }
  ensure!(Instant::now()<deadline,"Launcher cleanup acknowledgment and exit timed out; no forced close or App swap");
  std::thread::sleep(Duration::from_millis(100));
 }
}
/// Ask Windows for the exact mutation rights without changing ACLs or elevating.
/// Delete rights are checked separately because writable files need not be movable.
pub fn permissions()->Result<()> {
 let r=root();ordinary(&r)?;
 for p in [r.clone(),r.join("Recovery"),r.join("Recovery/Updates"),r.join("Recovery/PreviousApp")] {
  let mut existing=p.as_path();while !existing.try_exists()?{existing=existing.parent().context("No permission ancestor")?;}
  probe(existing,0x2|0x4|0x40)?; // add file/subdirectory, delete child
 }
 for p in [r.join("App"),r.join("Recovery/PreviousApp/App")] {
  if p.try_exists()? {probe(&p,0x10000)?;for e in fs::read_dir(&p)?{let f=e?.path();probe(&f,0x10000)?;}}
 }Ok(())
}
fn probe(p:&Path,access:u32)->Result<()> {
 ordinary(p)?;let w:Vec<u16>=p.as_os_str().encode_wide().chain(Some(0)).collect();
 let h=unsafe{CreateFileW(PCWSTR(w.as_ptr()),access,FILE_SHARE_READ|FILE_SHARE_WRITE|FILE_SHARE_DELETE,None,OPEN_EXISTING,FILE_FLAG_BACKUP_SEMANTICS|FILE_FLAG_OPEN_REPARSE_POINT,None)};
 match h {Ok(h)=>{drop(Handle(h));Ok(())},Err(e)=>bail!("Installation permission/lock preflight failed at {}: {}. No ACL change or automatic elevation was attempted.",p.display(),e)}
}
