//! Read-only, bounded-memory copies of resources that need no conversion.
use crate::{
    catalog::{Catalog, Resource},
    codec,
};
use anyhow::{Result, ensure};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::AtomicBool,
};

pub const BUFFER_SIZE: usize = 1024 * 1024;

/// Documents need complete parsing even in original mode. Containers retain
/// directory/CRC validation. Mod-ready textures/buffers retain reconstruction.
pub fn eligible_name(name: &str, mode: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(mode, "original" | "mod-ready")
        && ![".iff", ".scne", ".txtr"]
            .iter()
            .any(|s| lower.ends_with(s))
        && !(mode == "mod-ready" && [".tld", ".gz"].iter().any(|s| lower.ends_with(s)))
}

pub struct Source {
    file: File,
    offset: u64,
    size: u64,
    pub header: Vec<u8>,
}
impl Source {
    pub fn open(catalog: &Catalog, resource: &Resource, cancel: &AtomicBool) -> Result<Self> {
        codec::cancelled(cancel)?;
        // Use the same archive-ID/path policy, but not the buffered size cap.
        let mut file = File::open(catalog.path(resource)?)?;
        let len = file.metadata()?.len();
        ensure!(
            resource.offset <= len && resource.size <= len - resource.offset,
            "Resource exceeds archive bounds"
        );
        file.seek(SeekFrom::Start(resource.offset))?;
        let mut header = vec![0; resource.size.min(148) as usize];
        file.read_exact(&mut header)?;
        codec::cancelled(cancel)?;
        Ok(Self {
            file,
            offset: resource.offset,
            size: resource.size,
            header,
        })
    }
    pub fn is_plain(&self) -> bool {
        // Do not let a ZIP with a misleading filename bypass archive checks.
        // Unknown PK wrappers stay on the existing strict/buffered path too.
        !self.header.starts_with(b"PK")
    }
    pub fn copy_to(
        &mut self,
        output: &mut impl Write,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<()> {
        self.file.seek(SeekFrom::Start(self.offset))?;
        copy_exact(&mut self.file, output, self.size, cancel, &mut progress)
    }
}

fn copy_exact(
    input: &mut impl Read,
    output: &mut impl Write,
    size: u64,
    cancel: &AtomicBool,
    progress: &mut impl FnMut(u64, u64),
) -> Result<()> {
    let mut buffer = vec![0; BUFFER_SIZE];
    let mut done = 0;
    progress(0, size);
    while done < size {
        codec::cancelled(cancel)?;
        let n = (size - done).min(buffer.len() as u64) as usize;
        input.read_exact(&mut buffer[..n])?;
        output.write_all(&buffer[..n])?;
        done += n as u64;
        progress(done, size);
    }
    // Includes cancellation triggered by the final progress callback.
    codec::cancelled(cancel)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::Ordering;
    #[test]
    fn conversion_and_container_eligibility_is_explicit() {
        for name in ["arena.IFF", "scene.SCNE", "color.TXTR"] {
            for mode in ["original", "mod-ready"] {
                assert!(!eligible_name(name, mode));
            }
        }
        for name in ["texture.TLD", "buffer.GZ"] {
            assert!(eligible_name(name, "original"));
            assert!(!eligible_name(name, "mod-ready"));
        }
        for name in [
            "media/movie.bk2",
            "audio/file.wem",
            "map.dds",
            "data.bin",
            "mip.mip0",
            "asset.zip", // Header, not extension, decides whether ZIP checks are needed.
        ] {
            assert!(eligible_name(name, "original"));
            assert!(eligible_name(name, "mod-ready"));
        }
        assert!(!eligible_name("file.bin", "unknown"));
    }
    #[test]
    fn exact_copy_stops_before_neighbor_range_and_progress_is_monotonic() {
        let mut input = Cursor::new(b"resourceNEIGHBOR".to_vec());
        let mut output = vec![];
        let mut events = vec![];
        copy_exact(
            &mut input,
            &mut output,
            8,
            &AtomicBool::new(false),
            &mut |a, b| events.push((a, b)),
        )
        .unwrap();
        assert_eq!(output, b"resource");
        assert_eq!(input.position(), 8);
        assert_eq!(events, [(0, 8), (8, 8)]);
        assert!(
            copy_exact(
                &mut input,
                &mut vec![],
                100,
                &AtomicBool::new(false),
                &mut |_, _| {}
            )
            .is_err()
        );
    }
    #[test]
    fn chunk_allocation_and_cancellation_are_bounded() {
        struct Input {
            max: usize,
        }
        impl Read for Input {
            fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
                self.max = self.max.max(b.len());
                b.fill(7);
                Ok(b.len())
            }
        }
        let mut input = Input { max: 0 };
        let cancel = AtomicBool::new(false);
        assert!(
            copy_exact(
                &mut input,
                &mut std::io::sink(),
                u64::MAX,
                &cancel,
                &mut |done, _| {
                    if done > 0 {
                        cancel.store(true, Ordering::Relaxed)
                    }
                }
            )
            .is_err()
        );
        assert_eq!(input.max, BUFFER_SIZE);
        let cancel = AtomicBool::new(false);
        assert!(
            copy_exact(
                &mut Cursor::new([1]),
                &mut vec![],
                1,
                &cancel,
                &mut |done, _| {
                    if done == 1 {
                        cancel.store(true, Ordering::Relaxed)
                    }
                }
            )
            .is_err()
        );
    }
}
