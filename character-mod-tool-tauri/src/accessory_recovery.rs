use anyhow::{Context, Result, ensure};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, fs, io::Read, os::windows::fs::MetadataExt, path::PathBuf};

pub const PATH_FIELDS: &[&str] = &[
    "target",
    "source",
    "filePath",
    "path",
    "geometryPath",
    "sourcePlayerPath",
];
type Inventory = Vec<(PathBuf, Vec<u8>)>;

#[derive(Default)]
pub struct Recovery {
    checked: HashMap<PathBuf, (Vec<u8>, u64)>,
    clock: u64,
}

fn player_path(path: &str) -> Option<PathBuf> {
    if path.contains("://") {
        return None;
    }
    let path = std::path::absolute(path).ok()?;
    let stem = path.file_stem()?.to_str()?;
    let player = stem.split('_').next()?;
    let lower = player.to_ascii_lowercase();
    if !path.extension()?.to_str()?.eq_ignore_ascii_case("iff")
        || !lower.starts_with("png")
        || lower.len() <= 3
        || !lower[3..].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let target = path.with_file_name(format!("{player}.iff"));
    Some(target.canonicalize().unwrap_or(target))
}

fn inventory(target: &std::path::Path) -> Result<Inventory> {
    let Some(parent) = target.parent() else {
        return Ok(Vec::new());
    };
    if !parent.is_dir() {
        return Ok(Vec::new());
    }
    let prefix = format!(
        ".cmt-accessory-{}-",
        target.file_stem().unwrap().to_string_lossy()
    )
    .to_ascii_lowercase();
    let mut rows = Vec::new();
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry
            .file_name()
            .to_string_lossy()
            .to_ascii_lowercase()
            .starts_with(&prefix)
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "Unverified accessory recovery path was preserved: {}",
            entry.path().display()
        );
        let journal = entry.path().join("journal.json");
        let metadata = fs::symlink_metadata(&journal)
            .context("Accessory recovery journal is missing; files were preserved")?;
        ensure!(
            metadata.is_file()
                && metadata.file_attributes() & 0x400 == 0
                && metadata.len() <= 4 * 1024 * 1024,
            "Unverified accessory recovery journal was preserved"
        );
        let mut bytes = Vec::new();
        fs::File::open(&journal)?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 4 * 1024 * 1024,
            "Oversized accessory recovery journal was preserved"
        );
        rows.push((entry.path(), Sha256::digest(&bytes).to_vec()));
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(rows)
}

impl Recovery {
    pub fn pending(&self, payload: &Value) -> Result<Vec<PathBuf>> {
        let mut targets = Vec::new();
        for key in PATH_FIELDS {
            if let Some(target) = payload[*key].as_str().and_then(player_path) {
                if targets.contains(&target) {
                    continue;
                }
                let rows = inventory(&target)?;
                if !rows.is_empty()
                    && self.checked.get(&target).map(|entry| &entry.0)
                        != Some(&inventory_hash(&rows))
                {
                    targets.push(target);
                }
            }
        }
        Ok(targets)
    }

    pub fn remember(&mut self, target: &std::path::Path) -> Result<()> {
        let rows = inventory(target)?;
        if self.checked.len() >= 512 && !self.checked.contains_key(target) {
            if let Some(oldest) = self
                .checked
                .iter()
                .min_by_key(|(_, entry)| entry.1)
                .map(|(path, _)| path.clone())
            {
                self.checked.remove(&oldest);
            }
        }
        self.clock += 1;
        self.checked
            .insert(target.to_path_buf(), (inventory_hash(&rows), self.clock));
        Ok(())
    }
}

fn inventory_hash(rows: &Inventory) -> Vec<u8> {
    let mut digest = Sha256::new();
    for (path, hash) in rows {
        let name = path.to_string_lossy();
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(hash);
    }
    digest.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_local_player_and_companion_paths_are_candidates() {
        assert_eq!(
            player_path("C:/mods/png01393_geo_hair.iff"),
            Some(PathBuf::from("C:/mods/png01393.iff"))
        );
        for path in [
            "cmt-preview://png123.iff",
            "C:/mods/chest_color_03.iff",
            "C:/mods/pngx.iff",
            "C:/mods/png12.dds",
        ] {
            assert!(player_path(path).is_none(), "{path}");
        }
    }
    #[test]
    fn recovery_checks_new_and_changed_journals_without_deleting_evidence() -> Result<()> {
        let folder =
            std::env::temp_dir().join(format!("cmt-native-recovery-{}", crate::runtime::stamp()));
        fs::create_dir_all(&folder)?;
        let target = folder.join("png123.iff");
        let transaction = folder.join(".cmt-accessory-png123-test");
        fs::create_dir(&transaction)?;
        fs::write(transaction.join("journal.json"), b"prepared")?;
        fs::write(transaction.join("user.blend"), b"preserve")?;
        let payload = serde_json::json!({"target":target});
        let mut recovery = Recovery::default();
        assert_eq!(recovery.pending(&payload)?, vec![target.clone()]);
        recovery.remember(&target)?;
        assert!(recovery.pending(&payload)?.is_empty());
        fs::write(transaction.join("journal.json"), b"complete")?;
        assert_eq!(recovery.pending(&payload)?, vec![target]);
        assert_eq!(fs::read(transaction.join("user.blend"))?, b"preserve");
        fs::remove_file(transaction.join("user.blend"))?;
        fs::remove_file(transaction.join("journal.json"))?;
        fs::remove_dir(transaction)?;
        fs::remove_dir(folder)?;
        Ok(())
    }

    #[test]
    fn many_journals_for_multiple_players_do_not_evict_each_other() -> Result<()> {
        let folder =
            std::env::temp_dir().join(format!("cmt-native-many-{}", crate::runtime::stamp()));
        fs::create_dir_all(&folder)?;
        let targets = [folder.join("png1.iff"), folder.join("png2.iff")];
        for target in &targets {
            for index in 0..300 {
                let transaction = folder.join(format!(
                    ".cmt-accessory-{}-{index}",
                    target.file_stem().unwrap().to_string_lossy()
                ));
                fs::create_dir(&transaction)?;
                fs::write(transaction.join("journal.json"), b"retained")?;
            }
        }
        let payload = serde_json::json!({"target":targets[0],"source":targets[1]});
        let mut recovery = Recovery::default();
        assert_eq!(recovery.pending(&payload)?.len(), 2);
        for target in &targets {
            recovery.remember(target)?;
        }
        assert!(recovery.pending(&payload)?.is_empty());
        assert!(recovery.pending(&payload)?.is_empty());
        assert_eq!(recovery.checked.len(), 2);
        for entry in fs::read_dir(&folder)? {
            let transaction = entry?.path();
            fs::remove_file(transaction.join("journal.json"))?;
            fs::remove_dir(transaction)?;
        }
        fs::remove_dir(folder)?;
        Ok(())
    }

    #[test]
    fn player_file_alias_resolves_before_recovery_inventory() -> Result<()> {
        let folder =
            std::env::temp_dir().join(format!("cmt-native-alias-{}", crate::runtime::stamp()));
        fs::create_dir_all(folder.join("actual"))?;
        let actual = folder.join("actual/png123.iff");
        let alias = folder.join("png123.iff");
        fs::write(&actual, b"player")?;
        let transaction = actual.parent().unwrap().join(".cmt-accessory-png123-test");
        fs::create_dir(&transaction)?;
        fs::write(transaction.join("journal.json"), b"prepared")?;
        let linked = std::os::windows::fs::symlink_file(&actual, &alias);
        if let Err(error) = &linked {
            ensure!(
                error.raw_os_error() == Some(1314),
                "Could not create alias fixture: {error}"
            );
            // Directory junctions exercise the canonical path when file symlink privileges are unavailable.
            let junction = folder.join("alias");
            let output = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&junction)
                .arg(actual.parent().unwrap())
                .output()?;
            ensure!(output.status.success(), "Could not create junction fixture");
            assert_eq!(
                player_path(junction.join("png123.iff").to_str().unwrap()),
                Some(actual.canonicalize()?)
            );
            fs::remove_dir(junction)?;
        } else {
            assert_eq!(
                player_path(alias.to_str().unwrap()),
                Some(actual.canonicalize()?)
            );
            assert_eq!(
                Recovery::default().pending(&serde_json::json!({"target":alias}))?,
                vec![actual.canonicalize()?]
            );
            fs::remove_file(alias)?;
        }
        fs::remove_file(transaction.join("journal.json"))?;
        fs::remove_dir(transaction)?;
        fs::remove_file(actual)?;
        fs::remove_dir(folder.join("actual"))?;
        fs::remove_dir(folder)?;
        Ok(())
    }
}
