//! Only one writer per portable data root. Separate prototype folders stay independent.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::path::Path;
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0},
        System::Threading::{CreateMutexW, ReleaseMutex, WaitForSingleObject},
    },
    core::PCWSTR,
};
pub struct Instance(HANDLE);
pub fn acquire(root: &Path) -> Result<Instance> {
    let canonical = root.canonicalize()?.to_string_lossy().to_lowercase();
    let name = format!(
        "Local\\2KLauncherRust-{:x}",
        Sha256::digest(canonical.as_bytes())
    );
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let handle = unsafe { CreateMutexW(None, false, PCWSTR(wide.as_ptr()))? };
    let wait = unsafe { WaitForSingleObject(handle, 0) };
    if wait != WAIT_OBJECT_0 && wait != WAIT_ABANDONED {
        unsafe {
            let _ = CloseHandle(handle);
        }
        ensure!(
            false,
            "This portable launcher folder is already open. Close its existing window before opening another copy."
        );
    }
    Ok(Instance(handle))
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_root_is_exclusive_and_releases() {
        let root =
            std::env::temp_dir().join(format!("launcher-instance-{}", crate::backend::unique_id()));
        std::fs::create_dir_all(&root).unwrap();
        let guard = acquire(&root).unwrap();
        let other = root.clone();
        assert!(
            std::thread::spawn(move || acquire(&other).is_err())
                .join()
                .unwrap()
        );
        drop(guard);
        let guard = acquire(&root).unwrap();
        drop(guard);
        std::fs::remove_dir(root).unwrap();
    }
}
