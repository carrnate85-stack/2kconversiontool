//! Bounded camera localization transforms, equivalent to WPF's
//! camera-localization.js. No extraction, process access or publication.
#![allow(dead_code)]
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
struct Label {
    id: u32,
    key: u64,
    source: &'static str,
    name: &'static str,
    donor: Option<(u64, &'static str)>,
    locked: bool,
}
const LABELS: [Label; 7] = [
    Label {
        id: 51,
        key: 0x070A02F4676F7A26,
        source: "Full Court",
        name: "Broadcast Low 2K18",
        donor: Some((0xE2D6B673633FB485, "Pregame Shootaround")),
        locked: false,
    },
    Label {
        id: 53,
        key: 0x724E93A6FB8B2389,
        source: "Half Court",
        name: "Press 2K17",
        donor: Some((0xB09970FB8F8E8E2A, "Coach View")),
        locked: false,
    },
    Label {
        id: 54,
        key: 0xB800A4C12E23B6BD,
        source: "DO NOT USE THIS CAM",
        name: "Press 2K27",
        donor: None,
        locked: false,
    },
    Label {
        id: 52,
        key: 0x81C8B0C640438EFB,
        source: "Full Court Corner",
        name: "Beluba 2K17",
        donor: None,
        locked: false,
    },
    Label {
        id: 57,
        key: 0x2AFCCD8CA86B576C,
        source: "Rail High",
        name: "Beluba 2K27",
        donor: Some((0xC0E7B391A0EDA7AD, "Minigame Downtown")),
        locked: false,
    },
    Label {
        id: 59,
        key: 0x3B94D5AAE1F235DE,
        source: "Side",
        name: "Side",
        donor: None,
        locked: true,
    },
    Label {
        id: 55,
        key: 0x7DE7AF563ED79DC4,
        source: "Nosebleeds",
        name: "Nosebleeds",
        donor: None,
        locked: true,
    },
];
pub(crate) fn names(overrides: &BTreeMap<u32, String>) -> Result<BTreeMap<u32, String>> {
    ensure!(
        overrides.len() <= LABELS.len()
            && overrides
                .keys()
                .all(|id| LABELS.iter().any(|l| l.id == *id)),
        "Unknown camera name override"
    );
    LABELS
        .iter()
        .map(|label| {
            let text = overrides
                .get(&label.id)
                .map(String::as_str)
                .unwrap_or(label.name)
                .trim();
            let maximum = label
                .donor
                .map(|(_, source)| source)
                .unwrap_or(label.source)
                .len();
            ensure!(
                !text.is_empty()
                    && text.len() <= maximum
                    && text.bytes().all(|b| (32..=126).contains(&b)),
                "Camera {} name must contain 1–{} printable English characters",
                label.id,
                maximum
            );
            ensure!(
                !label.locked || text == label.name,
                "{} retains its stock name",
                label.name
            );
            Ok((label.id, text.into()))
        })
        .collect()
}
pub(crate) fn key(id: u32) -> Result<u64> {
    Ok(LABELS
        .iter()
        .find(|l| l.id == id)
        .context("Unknown camera name key")?
        .key)
}
fn read<const N: usize>(bytes: &[u8], at: usize) -> Result<[u8; N]> {
    Ok(bytes
        .get(
            at..at
                .checked_add(N)
                .context("Camera localization range overflow")?,
        )
        .context("Truncated camera localization")?
        .try_into()?)
}
struct TextSpan {
    pool: usize,
    field: usize,
    start: usize,
    end: usize,
    value: String,
}
fn resolve(text: &[u8], key: u64) -> Result<TextSpan> {
    ensure!(
        (64..=crate::sigconfig::MAX_BYTES).contains(&text.len()),
        "Camera localization size is invalid"
    );
    let declared = u64::from_le_bytes(read(text, 24)?);
    let pool = usize::try_from(
        declared
            .checked_add(63)
            .context("Camera string-pool overflow")?
            & !63,
    )?;
    ensure!(
        pool >= 64 && pool < text.len(),
        "Unsupported camera string pool"
    );
    let needle = key.to_le_bytes();
    let mut found = None;
    for at in (64..pool).step_by(8).filter(|at| at % 64 < 40) {
        if text.get(at..at + 8) == Some(needle.as_slice()) {
            ensure!(
                found.replace(at).is_none(),
                "Camera localization key is not unique"
            );
        }
    }
    let at = found.context("Camera localization key is missing")?;
    let field = (at / 64) * 64 + 40 + (at % 64 / 8) * 4;
    let offset = u32::from_le_bytes(read(text, field)?) as usize;
    let start = pool
        .checked_add(
            offset
                .checked_mul(2)
                .context("Camera name offset overflow")?,
        )
        .context("Camera name offset overflow")?;
    ensure!(
        start >= pool && start.checked_add(2).is_some_and(|end| end <= text.len()),
        "Camera name is outside its string pool"
    );
    let mut end = start;
    let mut words = Vec::new();
    loop {
        let word = u16::from_le_bytes(read(text, end)?);
        if word == 0 {
            break;
        }
        words.push(word);
        end += 2;
        ensure!(words.len() <= 4096, "Camera source name exceeds bounds");
    }
    Ok(TextSpan {
        pool,
        field,
        start,
        end,
        value: String::from_utf16(&words)?,
    })
}
pub(crate) fn patch_text(original: &[u8], overrides: &BTreeMap<u32, String>) -> Result<Vec<u8>> {
    let names = names(overrides)?;
    let mut output = original.to_vec();
    // Inspect all sources and destinations BEFORE mutating the private copy.
    let mut changes = Vec::new();
    for label in LABELS {
        let source = resolve(original, label.key)?;
        ensure!(
            source.value == label.source,
            "Camera {} source text changed",
            label.id
        );
        let name = &names[&label.id];
        if name == label.source {
            continue;
        }
        let target = if let Some((donor, expected)) = label.donor {
            let target = resolve(original, donor)?;
            ensure!(target.value == expected, "Camera name donor changed");
            target
        } else {
            resolve(original, label.key)?
        };
        ensure!(
            target.start == target.pool
                || u16::from_le_bytes(read(original, target.start - 2)?) == 0,
            "Camera name storage is suffix-shared"
        );
        let bytes: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        ensure!(
            bytes.len() <= target.end - target.start,
            "Camera name exceeds verified storage"
        );
        ensure!(
            changes.iter().all(
                |(old, _, _): &(TextSpan, usize, Vec<u8>)| old.end <= target.start
                    || target.end <= old.start
            ),
            "Camera name destinations overlap"
        );
        changes.push((target, source.field, bytes));
    }
    for (target, field, bytes) in changes {
        output[target.start..target.end].fill(0);
        output[target.start..target.start + bytes.len()].copy_from_slice(&bytes);
        let offset = u32::try_from((target.start - target.pool) / 2)?;
        output[field..field + 4].copy_from_slice(&offset.to_le_bytes());
    }
    verify_text(&output, &names)?;
    Ok(output)
}
fn verify_text(text: &[u8], overrides: &BTreeMap<u32, String>) -> Result<()> {
    for (id, name) in names(overrides)? {
        ensure!(
            resolve(text, key(id)?)?.value == name,
            "Camera {id} name verification failed"
        );
    }
    Ok(())
}
pub(crate) fn build(archive: &[u8], overrides: &BTreeMap<u32, String>) -> Result<Vec<u8>> {
    let mut entries =
        crate::selector_tables::archive(archive).context("Invalid camera localization IFF")?;
    let source = entries
        .get("TEXT.VCLOCALIZEDATA")
        .context("English archive has no localization member")?;
    entries.insert("TEXT.VCLOCALIZEDATA".into(), patch_text(source, overrides)?);
    crate::selector_tables::pack(&entries)
}
pub(crate) fn verify(archive: &[u8], overrides: &BTreeMap<u32, String>) -> Result<()> {
    let entries = crate::selector_tables::archive(archive)?;
    verify_text(
        entries
            .get("TEXT.VCLOCALIZEDATA")
            .context("Camera localization member missing")?,
        overrides,
    )
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn stock() -> Vec<u8> {
        let mut bytes = vec![0; 2048];
        bytes[24..32].copy_from_slice(&256u64.to_le_bytes());
        let keys: Vec<_> = LABELS
            .iter()
            .map(|l| (l.key, l.source))
            .chain(LABELS.iter().filter_map(|l| l.donor))
            .collect();
        let mut next = 256;
        for (index, (key, name)) in keys.into_iter().enumerate() {
            let block = 64 + (index / 5) * 64;
            let lane = index % 5;
            bytes[block + lane * 8..block + lane * 8 + 8].copy_from_slice(&key.to_le_bytes());
            let prefix = if name == "Half Court" {
                "Eight Seconds "
            } else {
                ""
            };
            let words: Vec<u8> = format!("{prefix}{name}\0")
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            bytes[next..next + words.len()].copy_from_slice(&words);
            let offset = ((next + prefix.len() * 2 - 256) / 2) as u32;
            bytes[block + 40 + lane * 4..block + 44 + lane * 4]
                .copy_from_slice(&offset.to_le_bytes());
            next += words.len();
        }
        bytes
    }
    #[test]
    fn default_and_custom_names_preserve_suffixes_size_and_unrelated_zip_members() {
        let original = stock();
        let mut overrides = BTreeMap::new();
        for custom in [false, true] {
            if custom {
                overrides.insert(51, "My Low Camera".into());
                overrides.insert(53, "My Press".into());
            }
            let output = patch_text(&original, &overrides).unwrap();
            assert_eq!(output.len(), original.len());
            verify_text(&output, &overrides).unwrap();
            let suffix: Vec<u8> = "Eight Seconds Half Court\0"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect();
            assert!(output.windows(suffix.len()).any(|w| w == suffix));
            let entries = BTreeMap::from([
                ("TEXT.VCLOCALIZEDATA".into(), original.clone()),
                ("OTHER".into(), b"unchanged".to_vec()),
            ]);
            let packed = crate::selector_tables::pack(&entries).unwrap();
            let built = build(&packed, &overrides).unwrap();
            verify(&built, &overrides).unwrap();
            assert_eq!(
                crate::selector_tables::archive(&built).unwrap()["OTHER"],
                b"unchanged"
            );
        }
    }
    #[test]
    fn names_keys_lengths_nonascii_and_modified_sources_fail_closed() {
        let source = stock();
        for overrides in [
            BTreeMap::from([(999, "Unknown".into())]),
            BTreeMap::from([(51, "x".repeat(20))]),
            BTreeMap::from([(51, "é".into())]),
            BTreeMap::from([(51, "a\nb".into())]),
            BTreeMap::from([(59, "Renamed".into())]),
            BTreeMap::from([(51, " ".into())]),
        ] {
            assert!(patch_text(&source, &overrides).is_err());
        }
        assert!(
            patch_text(
                &patch_text(&source, &BTreeMap::new()).unwrap(),
                &BTreeMap::new()
            )
            .is_err()
        );
        let mut duplicate = source.clone();
        let key = LABELS[0].key.to_le_bytes();
        duplicate[192..200].copy_from_slice(&key);
        assert!(patch_text(&duplicate, &BTreeMap::new()).is_err());
        for len in [0, 16, 63, 64, 256] {
            assert!(patch_text(&source[..len], &BTreeMap::new()).is_err());
        }
        let mut overflow = source;
        overflow[24..32].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(patch_text(&overflow, &BTreeMap::new()).is_err());
    }
    #[test]
    fn provider_only_labels_are_immutable_and_never_replace_an_existing_localization_mod() {
        use std::{
            fs,
            sync::{Arc, atomic::AtomicBool},
        };
        let root = std::env::temp_dir().join(format!(
            "camera-provider-only-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir(&root).unwrap();
        let original = crate::selector_tables::pack(&BTreeMap::from([
            ("TEXT.VCLOCALIZEDATA".into(), stock()),
            ("OTHER".into(), b"retain".to_vec()),
        ]))
        .unwrap();
        let names = BTreeMap::new();
        let prepared = build(&original, &names).unwrap();
        let service = crate::provider_assets::Service::open(
            &root,
            &[],
            &[],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        service.add_probe(&crate::backend::unique_id()).unwrap();
        assert!(
            service
                .publish_camera_names(b"invalid IFF".to_vec(), &names)
                .is_err()
        );
        assert!(service.query("loc/english.iff").is_err());
        service
            .publish_camera_names(prepared.clone(), &names)
            .unwrap();
        assert!(!root.join("loc").exists());
        assert_eq!(
            service.query("loc/english.iff").unwrap().size,
            prepared.len() as u64
        );
        assert_eq!(
            service
                .read("loc/english.iff", 0, prepared.len() as u32)
                .unwrap()
                .1,
            prepared
        );
        assert!(
            service
                .publish_camera_names(prepared.clone(), &names)
                .is_err()
        );
        drop(service);
        fs::create_dir(root.join("loc")).unwrap();
        fs::write(root.join("loc/english.iff"), &original).unwrap();
        let service = crate::provider_assets::Service::open(
            &root,
            &[],
            &[],
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
        assert!(service.publish_camera_names(prepared, &names).is_err());
        assert_eq!(fs::read(root.join("loc/english.iff")).unwrap(), original);
        drop(service);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore = "explicit installed Version 2 localization disk audit; no game launch or memory access"]
    fn actual_installed_version_two_localization_is_read_only() {
        use crate::stock_selectors::{Decoder, Resource};
        use sha2::{Digest, Sha256};
        let root = std::path::PathBuf::from(
            std::env::var("C27_CAMERA_NAMES_GAME").expect("Explicit game folder required"),
        );
        let check = || -> Result<()> {
            ensure!(
                !crate::session_safety::processes()?
                    .iter()
                    .any(|p| p.name.eq_ignore_ascii_case("NBA2K27.exe")
                        || crate::session_safety::protected_process(&p.name)),
                "Keep the game and protected processes closed for this disk audit"
            );
            Ok(())
        };
        check().unwrap();
        let image = crate::provider_access::verify_build(&root.join("NBA2K27.exe")).unwrap();
        assert_eq!(image.build["label"], "Version 2");
        let resource = Resource::camera_localization(&root, &check).unwrap();
        let packed = resource.packed(&root, &check).unwrap();
        let decoder = Decoder::load(&root).unwrap();
        let raw = decoder.decode(&packed, &check).unwrap();
        let built = build(&raw, &BTreeMap::new()).unwrap();
        verify(&built, &BTreeMap::new()).unwrap();
        let original = crate::selector_tables::archive(&raw).unwrap();
        let after = crate::selector_tables::archive(&built).unwrap();
        assert_eq!(original.len(), after.len());
        for (name, data) in &original {
            if name != "TEXT.VCLOCALIZEDATA" {
                assert_eq!(data, &after[name]);
            }
        }
        check().unwrap();
        let report = serde_json::json!({"schema":"nba2k27-v2-camera-localization-disk-audit-v1","packedBytes":packed.len(),"decodedBytes":raw.len(),"rebuiltBytes":built.len(),
            "packedSha256":format!("{:X}",Sha256::digest(&packed)),"cameraNamesVerified":7,"unrelatedMembersPreserved":true,"gameProcessAccess":false,"gameFilesChanged":false});
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("artifacts")
            .join(format!(
                "camera-localization-disk-{}",
                crate::backend::unique_id()
            ));
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(
            folder.join("report.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{}", report);
    }
}
