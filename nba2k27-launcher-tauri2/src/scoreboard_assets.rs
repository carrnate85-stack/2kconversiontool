//! Actual installed-stock scoreboard preparation. Read-only archive handles,
//! pinned ordinary parents, fixed six logical paths; Oodle/VCZ reused verbatim.
//! No receipt, mod publication, game process, cache or provider mutation here.
#![allow(dead_code)]
use crate::{
    loader_files::Directories,
    scoreboard_layout::{self, Prepared},
    stock_selectors::{Decoder, Resource},
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};
fn parse(
    mut reader: impl BufRead,
    check: impl Fn() -> Result<()>,
) -> Result<BTreeMap<String, Resource>> {
    let mut found = BTreeMap::new();
    let mut total = 0usize;
    let mut buffer = Vec::new();
    for row in 0..2_000_000 {
        // Only a read-only scan. Keep bounded checkpoints and an EOF check;
        // never reuse this result as permission to mutate the game folder.
        if row % 16384 == 0 {
            check()?;
        }
        buffer.clear();
        let n = reader.by_ref().take(8193).read_until(b'\n', &mut buffer)?;
        if n == 0 {
            check()?;
            ensure!(
                found.len() == 6,
                "Stock scoreboard assets are missing from the manifest"
            );
            return Ok(found);
        }
        total = total
            .checked_add(n)
            .context("Scoreboard manifest size overflow")?;
        ensure!(
            n <= 8192 && total <= 256 * 1024 * 1024,
            "Scoreboard manifest exceeds bounds"
        );
        let line = std::str::from_utf8(&buffer)?.trim_end_matches(['\r', '\n']);
        let fields = line.split(',').collect::<Vec<_>>();
        if fields.len() != 4 {
            continue;
        }
        let name = fields[0].replace('\\', "/").to_ascii_lowercase();
        if name != "loc/english.iff" && !scoreboard_layout::PATHS.contains(&name.as_str()) {
            continue;
        }
        let r = Resource {
            archive: fields[1].into(),
            offset: fields[2].parse()?,
            size: fields[3].parse()?,
        };
        r.validate()?;
        ensure!(
            found.insert(name, r).is_none(),
            "Duplicate stock scoreboard manifest row"
        );
    }
    anyhow::bail!("Scoreboard manifest row limit exceeded")
}
fn manifest(root: &Path, check: impl Fn() -> Result<()>) -> Result<BTreeMap<String, Resource>> {
    let _pins = Directories::pin(root)?;
    let file: File = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(root.join("manifest"))?;
    let before = file.metadata()?;
    ensure!(
        before.is_file()
            && before.file_attributes() & 0x400 == 0
            && before.len() <= 256 * 1024 * 1024,
        "Scoreboard manifest is not a bounded ordinary file"
    );
    let result = parse(BufReader::new(&file), &check)?;
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.last_write_time() == after.last_write_time(),
        "Scoreboard manifest changed during inspection"
    );
    check()?;
    Ok(result)
}
pub(crate) fn prepare(root: &Path, check: impl Fn() -> Result<()>) -> Result<Prepared> {
    let entries = manifest(root, &check)?;
    let decoder = Decoder::load(root)?;
    scoreboard_layout::prepare(
        |name| {
            let entry = entries
                .get(name)
                .context("Unknown scoreboard logical source")?;
            decoder.decode(&entry.packed(root, &check)?, &check)
        },
        &check,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn valid() -> String {
        std::iter::once("loc/english.iff")
            .chain(scoreboard_layout::PATHS)
            .enumerate()
            .map(|(i, p)| format!("{p},0A,{},24\n", i * 24))
            .collect()
    }
    #[test]
    fn fixed_manifest_lookup_is_unique_bounded_and_cancellable() {
        assert_eq!(parse(Cursor::new(valid()), || Ok(())).unwrap().len(), 6);
        for bad in [
            valid() + &valid(),
            valid().replace(",0A,", ",../0A,"),
            valid().replace(",24\n", ",0\n"),
            "x".repeat(8193),
        ] {
            assert!(parse(Cursor::new(bad), || Ok(())).is_err());
        }
        assert!(parse(Cursor::new(valid()), || anyhow::bail!("cancelled")).is_err());
    }
    #[test]
    fn readonly_manifest_batches_still_check_end_and_reject_late_duplicates() {
        use std::cell::Cell;
        let text = "other,0A,0,24\n".repeat(16384) + &valid();
        let calls = Cell::new(0);
        parse(Cursor::new(&text), || {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(calls.get(), 3);
        calls.set(0);
        assert!(
            parse(Cursor::new(&text), || {
                calls.set(calls.get() + 1);
                ensure!(calls.get() != 3, "final cancellation");
                Ok(())
            })
            .is_err()
        );
        assert!(parse(Cursor::new(text + &valid()), || Ok(())).is_err());
    }
    #[test]
    #[ignore = "Explicit read-only installed V2 stock archive audit; no process or game-file writes"]
    fn actual_installed_v2_five_layouts_preserve_original_sha_bound_edits() {
        let root = Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27");
        let prepared = prepare(root, || Ok(())).unwrap();
        assert_eq!(prepared.files().len(), 5);
        assert_eq!(prepared.source_hashes().len(), 6);
        for id in [
            "mycourt-scrimmage",
            "mypark-simple",
            "proam-spectator",
            "shut-it-down",
            "w-scrimmage",
        ] {
            assert!(prepared.ready_for(id).unwrap());
        }
        println!(
            "All5 Version2 stock layouts decoded, source-SHA/text/English verified, patched and ZIP readback verified entirely in memory; no process or file changes"
        );
    }
}
