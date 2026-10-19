//! Portable, content-keyed stock decode cache. Cache contents are checked
//! against the current packed game's authored length/CRC, not trusted metadata.
#![allow(dead_code)]
use crate::{
    backend,
    loader_files::{self, Directories},
    sigconfig,
    stock_selectors::{self, Decoder, Manifest},
};
use anyhow::{Result, ensure};
use std::{
    fs,
    io::{Read, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
const NAMES: [&str; 3] = ["all.iff", "edit.iff", "registry.iff"];
fn folder(portable: &Path, key: &str) -> Result<(PathBuf, Vec<Directories>)> {
    ensure!(
        key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit()),
        "Invalid stock cache fingerprint"
    );
    let mut path = portable.to_path_buf();
    let mut pins = vec![Directories::pin(portable)?];
    for name in ["Cache", "native-selectors", key] {
        path.push(name);
        match fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        pins.push(Directories::pin(&path)?);
    }
    Ok((path, pins))
}
fn cached(path: &Path, packed: &[u8]) -> Result<Vec<u8>> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.file_attributes() & 0x400 == 0
            && meta.len() <= sigconfig::MAX_BYTES as u64,
        "Invalid stock cache file"
    );
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take((sigconfig::MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    stock_selectors::verify_decoded(packed, &bytes)?;
    Ok(bytes)
}
fn save(path: &Path, bytes: &[u8], check: impl Fn() -> Result<()>) -> Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        return Ok(());
    }
    let temporary = path.with_file_name(format!(".stock-selector-{}.tmp", backend::unique_id()));
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .access_mode(0x80000000 | 0x40000000 | 0x10000)
        .share_mode(1)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        check()?;
        loader_files::publish(&file, path)
    })();
    if result.is_err() {
        let _ = loader_files::unlink(&file);
    }
    result
}
pub(crate) fn load(
    portable: &Path,
    game: &Path,
    manifest: &Manifest,
    check: impl Fn() -> Result<()>,
) -> Result<([Vec<u8>; 3], bool)> {
    let mut packed = Vec::new();
    for entry in &manifest.entries {
        packed.push(entry.packed(game, &check)?);
    }
    let packed: [Vec<u8>; 3] = packed.try_into().unwrap();
    let mut decoder = None;
    decoded_cache(portable, &packed, &check, |source| {
        if decoder.is_none() {
            decoder = Some(Decoder::load(game)?);
        }
        decoder.as_ref().unwrap().decode(source, &check)
    })
}
fn decoded_cache(
    portable: &Path,
    packed: &[Vec<u8>; 3],
    check: impl Fn() -> Result<()>,
    mut decode: impl FnMut(&[u8]) -> Result<Vec<u8>>,
) -> Result<([Vec<u8>; 3], bool)> {
    let key = stock_selectors::fingerprint(packed);
    let (directory, _pins) = folder(portable, &key)?;
    let mut hit = Vec::new();
    for (name, source) in NAMES.iter().zip(packed) {
        check()?;
        if let Ok(bytes) = cached(&directory.join(name), source) {
            hit.push(bytes);
        } else {
            break;
        }
    }
    if hit.len() == 3 {
        return Ok((hit.try_into().unwrap(), true));
    }
    let mut output = Vec::new();
    for (source, name) in packed.iter().zip(NAMES) {
        check()?;
        let bytes = decode(source)?;
        // A damaged/foreign cache is never adopted or overwritten. Regenerate
        // in memory; a cache write failure cannot make valid game data invalid.
        let _ = save(&directory.join(name), &bytes, &check);
        output.push(bytes);
    }
    check()?;
    Ok((output.try_into().unwrap(), false))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn valid_cache_skips_native_decoder_but_tampering_and_cancellation_cannot_be_adopted() {
        let root =
            std::env::temp_dir().join(format!("rust-selector-cache-hit-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let raw = crate::selector_tables::tests::bundle()[0].clone();
        let mut source = vec![0; 17];
        source[..4].copy_from_slice(&[0x1f, 0x8b, 0x21, 4]);
        source[10] = 4;
        source[12..16].copy_from_slice(b"VCZ\0");
        source.extend(crc32fast::hash(&raw).to_le_bytes());
        source.extend((raw.len() as u32).to_le_bytes());
        let packed = [source.clone(), source.clone(), source];
        let calls = std::cell::Cell::new(0);
        let result = decoded_cache(
            &root,
            &packed,
            || Ok(()),
            |_| {
                calls.set(calls.get() + 1);
                Ok(raw.clone())
            },
        )
        .unwrap();
        assert!(!result.1);
        assert_eq!(calls.get(), 3);
        let result = decoded_cache(
            &root,
            &packed,
            || Ok(()),
            |_| panic!("valid cache must not load native decoder"),
        )
        .unwrap();
        assert!(result.1);
        assert_eq!(result.0, [raw.clone(), raw.clone(), raw.clone()]);
        let path = root
            .join("Cache/native-selectors")
            .join(stock_selectors::fingerprint(&packed))
            .join("all.iff");
        fs::write(&path, b"tampered cache").unwrap();
        let result = decoded_cache(&root, &packed, || Ok(()), |_| Ok(raw.clone())).unwrap();
        assert!(!result.1);
        assert_eq!(result.0[0], raw);
        assert_eq!(fs::read(path).unwrap(), b"tampered cache");
        assert!(
            decoded_cache(
                &root,
                &packed,
                || anyhow::bail!("cancelled"),
                |_| panic!("cancelled")
            )
            .is_err()
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn cache_is_content_keyed_bounded_and_never_follows_a_linked_parent() {
        let root =
            std::env::temp_dir().join(format!("rust-selector-cache-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let (path, pins) = folder(&root, &"a".repeat(64)).unwrap();
        save(&path.join("all.iff"), b"owned cache", || Ok(())).unwrap();
        save(&path.join("all.iff"), b"must not overwrite", || Ok(())).unwrap();
        assert_eq!(fs::read(path.join("all.iff")).unwrap(), b"owned cache");
        assert!(folder(&root, "../outside").is_err());
        drop(pins);
        fs::remove_dir_all(&root).unwrap();
        let root =
            std::env::temp_dir().join(format!("rust-selector-cache-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("outside")).unwrap();
        crate::mod_profile::fixture_junction(&root.join("Cache"), &root.join("outside")).unwrap();
        assert!(folder(&root, &"a".repeat(64)).is_err());
        assert_eq!(fs::read_dir(root.join("outside")).unwrap().count(), 0);
        fs::remove_dir_all(root).unwrap();
    }
}
