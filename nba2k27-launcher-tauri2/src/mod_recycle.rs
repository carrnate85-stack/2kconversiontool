//! Recoverable Mod Library removal. Every target is resolved beneath the
//! configured mods root before Windows is asked to move it to the Recycle Bin.
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::HashSet,
    os::windows::ffi::OsStrExt,
    path::{Component, Path, PathBuf},
};
use windows::{
    Win32::{
        Foundation::HWND,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
        UI::Shell::{
            FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOFX_ADDUNDORECORD,
            FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IFileOperationProgressSink,
            IShellItem, SHCreateItemFromParsingName,
        },
    },
    core::PCWSTR,
};

const MAX_ITEMS: usize = 256;
const MAX_RELATIVE_PATH: usize = 1024;

pub fn resolve(mods_root: &str, args: &Value) -> Result<Vec<PathBuf>> {
    let root = std::fs::canonicalize(mods_root)
        .ok()
        .filter(|path| path.is_dir())
        .context("The selected mods folder does not exist")?;
    let requested = args["paths"].as_array().context("Mod paths required")?;
    ensure!(
        !requested.is_empty() && requested.len() <= MAX_ITEMS,
        "Choose between 1 and {MAX_ITEMS} mod files"
    );
    let mut seen = HashSet::new();
    let mut files = Vec::with_capacity(requested.len());
    for value in requested {
        let relative = value.as_str().context("Every mod path must be a string")?;
        let relative = Path::new(relative);
        ensure!(
            !relative.as_os_str().is_empty()
                && relative.as_os_str().len() <= MAX_RELATIVE_PATH
                && relative
                    .components()
                    .all(|part| matches!(part, Component::Normal(_))),
            "Invalid mod path"
        );
        let file = std::fs::canonicalize(root.join(relative))
            .context("A mod file no longer exists; scan again")?;
        ensure!(
            file.starts_with(&root) && file.is_file(),
            "A selected item is outside the mods folder or is not a file"
        );
        if seen.insert(file.clone()) {
            files.push(file);
        }
    }
    ensure!(!files.is_empty(), "No unique mod files were selected");
    Ok(files)
}

/// Uses IFileOperation's recycle-only flag so an unavailable Recycle Bin is
/// reported as a failure instead of silently falling back to permanent delete.
pub fn recycle(hwnd: HWND, files: &[PathBuf]) -> Result<()> {
    ensure!(!files.is_empty(), "No mod files were selected");
    unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_INPROC_SERVER)?;
        operation.SetOwnerWindow(hwnd)?;
        operation.SetOperationFlags(
            FOFX_RECYCLEONDELETE
                | FOFX_ADDUNDORECORD
                | FOF_NOCONFIRMATION
                | FOF_NOERRORUI
                | FOF_SILENT,
        )?;
        for file in files {
            let wide: Vec<u16> = file.as_os_str().encode_wide().chain(Some(0)).collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            operation.DeleteItem(&item, None::<&IFileOperationProgressSink>)?;
        }
        operation.PerformOperations()?;
        ensure!(
            !operation.GetAnyOperationsAborted()?.as_bool(),
            "Windows cancelled the Recycle Bin operation"
        );
    }
    ensure!(
        files.iter().all(|file| !file.exists()),
        "One or more mod files could not be moved to the Recycle Bin"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "launcher-mod-recycle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("players/sample")).unwrap();
        std::fs::write(root.join("players/sample/png1.iff"), b"mod").unwrap();
        std::fs::write(root.join("players/sample/png1_item_hair.iff"), b"hair").unwrap();
        root
    }

    #[test]
    fn resolves_only_unique_files_inside_the_active_mods_root() {
        let root = fixture();
        let paths = resolve(
            root.to_str().unwrap(),
            &json!({"paths":[
                "players/sample/png1.iff",
                "players/sample/png1_item_hair.iff",
                "players/sample/png1.iff"
            ]}),
        )
        .unwrap();
        let canonical_root = std::fs::canonicalize(&root).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.starts_with(&canonical_root)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_traversal_absolute_missing_and_directory_targets() {
        let root = fixture();
        for value in [
            json!({"paths":["../outside.iff"]}),
            json!({"paths":["C:\\outside.iff"]}),
            json!({"paths":["missing.iff"]}),
            json!({"paths":["players/sample"]}),
            json!({"paths":[]}),
        ] {
            assert!(resolve(root.to_str().unwrap(), &value).is_err());
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
