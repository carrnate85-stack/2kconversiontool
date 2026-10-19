//! Persistent kernel file identity. Creation timestamps are metadata and can
//! change under NTFS rename tunneling; they are not an identity capability.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs::File, mem::size_of, os::windows::io::AsRawHandle};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Stable {
    volume: u64,
    index: [u8; 16],
}
impl Stable {
    pub(crate) fn read(file: &File) -> Result<Self> {
        let mut info = FILE_ID_INFO::default();
        unsafe {
            GetFileInformationByHandleEx(
                HANDLE(file.as_raw_handle()),
                FileIdInfo,
                &mut info as *mut FILE_ID_INFO as _,
                size_of::<FILE_ID_INFO>() as u32,
            )?;
        }
        ensure!(
            info.FileId.Identifier.iter().any(|b| *b != 0),
            "Filesystem did not provide a persistent file identity"
        );
        Ok(Self {
            volume: info.VolumeSerialNumber,
            index: info.FileId.Identifier,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn persistent_identity_follows_the_retained_file_not_its_name_or_equal_bytes() {
        let root =
            std::env::temp_dir().join(format!("stable-file-id-{}", crate::backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let a = root.join("a");
        let b = root.join("b");
        fs::write(&a, b"same bytes").unwrap();
        fs::write(&b, b"same bytes").unwrap();
        let first = File::open(&a).unwrap();
        let id = Stable::read(&first).unwrap();
        assert_ne!(id, Stable::read(&File::open(&b).unwrap()).unwrap());
        fs::rename(&a, root.join("moved")).unwrap();
        assert_eq!(id, Stable::read(&first).unwrap());
        fs::write(&a, b"same bytes").unwrap();
        assert_ne!(id, Stable::read(&File::open(&a).unwrap()).unwrap());
        drop(first);
        fs::remove_dir_all(root).unwrap();
    }
}
