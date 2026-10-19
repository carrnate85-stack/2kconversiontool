//! Exact runtime allowlist: inspect bytes through a deny-write handle before
//! loading, and retain that handle plus pinned ancestors until FreeLibrary.
use anyhow::{Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    ops::Deref,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
#[derive(Clone, Copy)]
pub(crate) enum Runtime {
    Core,
    Texture,
}
impl Runtime {
    fn name(self) -> &'static str {
        match self {
            Self::Core => "oo2core_9_win64.dll",
            Self::Texture => "oo2texrt_9_win64.dll",
        }
    }
    fn hash(self) -> &'static str {
        match self {
            Self::Core => "9A66B713306E504863646969A59FA435C3EAC01457925F25024A4883803892F5",
            Self::Texture => "6270921A7226CD6DC6623CACB3173DE902547F8ABCCA72402ADACC2F9B1F6914",
        }
    }
}
pub(crate) struct Library {
    // Drop the loaded module before releasing the file and its ancestors.
    library: libloading::Library,
    _file: File,
    _pins: crate::loader_files::Directories,
}
impl Deref for Library {
    type Target = libloading::Library;
    fn deref(&self) -> &Self::Target {
        &self.library
    }
}
fn verify(file: &mut File, runtime: Runtime) -> Result<()> {
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.file_attributes() & 0x400 == 0
            && (1024..=16 * 1024 * 1024).contains(&meta.len()),
        "Unsupported Oodle runtime file"
    );
    let mut digest = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buf)?;
        if count == 0 {
            break;
        }
        digest.update(&buf[..count]);
    }
    ensure!(
        format!("{:X}", digest.finalize()) == runtime.hash(),
        "Oodle runtime is not an approved NBA 2K27 binary"
    );
    Ok(())
}
pub(crate) fn load(root: &Path, runtime: Runtime) -> Result<Library> {
    let path = root.join("data/oodle").join(runtime.name());
    let pins = crate::loader_files::Directories::pin(path.parent().unwrap())?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    verify(&mut file, runtime)?;
    let canonical = path.canonicalize()?;
    // Fixed absolute approved binary. Dependencies resolve only beside it or
    // in System32; never search the current working directory or PATH.
    let library = unsafe {
        libloading::os::windows::Library::load_with_flags(canonical.as_os_str(), 0x100 | 0x800)?
    }
    .into();
    Ok(Library {
        library,
        _file: file,
        _pins: pins,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unapproved_runtime_refused_before_load() {
        let dir = std::env::temp_dir().join(format!("qa085-oodle-{}", crate::backend::unique_id()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("runtime.dll");
        std::fs::write(&path, vec![0u8; 2048]).unwrap();
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(
            verify(&mut file, Runtime::Core)
                .unwrap_err()
                .to_string()
                .contains("approved")
        );
        assert!(OpenOptions::new().write(true).open(&path).is_err());
        assert!(std::fs::remove_file(&path).is_err());
        drop(file);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn both_pins_are_complete_and_distinct() {
        assert_eq!(Runtime::Core.hash().len(), 64);
        assert_eq!(Runtime::Texture.hash().len(), 64);
        assert_ne!(Runtime::Core.hash(), Runtime::Texture.hash());
    }
}
