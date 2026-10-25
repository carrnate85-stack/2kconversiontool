mod backend {
    pub const VERSION:&str=env!("CARGO_PKG_VERSION");
    pub fn unique_id()->String {format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())}
    pub fn atomic_json(path:&std::path::Path,v:&impl serde::Serialize)->anyhow::Result<()> {
        use std::io::Write;
        use std::os::windows::ffi::OsStrExt;
        use windows::{core::PCWSTR,Win32::Storage::FileSystem::*};
        let tmp=path.with_extension(format!("{}.tmp",unique_id()));
        let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(&serde_json::to_vec_pretty(v)?)?;file.sync_all()?;drop(file);
        let from:Vec<u16>=tmp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to:Vec<u16>=path.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {MoveFileExW(PCWSTR(from.as_ptr()),PCWSTR(to.as_ptr()),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH)?;} Ok(())
    }
}
mod session_safety {
    use windows::{core::PWSTR,Win32::{Foundation::*,System::{Diagnostics::ToolHelp::*,Threading::*}}};
    pub struct Identity {pub path:String,pub started:String}
    pub struct Process {pub pid:u32,pub name:String,pub identity:Option<Identity>}
    pub fn processes()->anyhow::Result<Vec<Process>> {unsafe {
        let snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0)?;
        let result=(||->anyhow::Result<Vec<Process>>{
            let mut rows=vec![];
            let mut row=PROCESSENTRY32W{dwSize:std::mem::size_of::<PROCESSENTRY32W>() as u32,..Default::default()};
            Process32FirstW(snapshot,&mut row)?;
            loop {
                let name=String::from_utf16_lossy(&row.szExeFile[..row.szExeFile.iter().position(|c|*c==0).unwrap_or(row.szExeFile.len())]);
                let identity=OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,false,row.th32ProcessID).ok().and_then(|h|{
                    let mut path=vec![0u16;32768];let mut size=path.len() as u32;
                    let mut created=FILETIME::default();let mut exit=FILETIME::default();let mut kernel=FILETIME::default();let mut user=FILETIME::default();
                    let ok=QueryFullProcessImageNameW(h,PROCESS_NAME_WIN32,PWSTR(path.as_mut_ptr()),&mut size).is_ok() && GetProcessTimes(h,&mut created,&mut exit,&mut kernel,&mut user).is_ok();
                    let _=CloseHandle(h);
                    ok.then(||Identity{path:String::from_utf16_lossy(&path[..size as usize]),started:format!("{}",((created.dwHighDateTime as u64)<<32)|created.dwLowDateTime as u64)})
                });
                rows.push(Process{pid:row.th32ProcessID,name,identity});
                match Process32NextW(snapshot,&mut row){Ok(())=>{},Err(e) if e.code()==ERROR_NO_MORE_FILES.to_hresult()=>break,Err(e)=>return Err(e.into())}
            } Ok(rows)
        })();let _=CloseHandle(snapshot);result
    }}
}