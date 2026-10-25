use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub id: usize,
    pub name: String,
    pub archive: String,
    pub offset: u64,
    pub size: u64,
}
pub struct Catalog {
    pub root: PathBuf,
    pub entries: Vec<Resource>,
    pub sorted: Vec<usize>,
    pub folders: BTreeMap<String, usize>,
}

pub fn safe_relative(name: &str) -> Result<PathBuf> {
    let normalized = name.replace('\\', "/");
    ensure!(
        !normalized.is_empty()
            && !normalized.starts_with('/')
            && !normalized.contains(':')
            && !normalized.contains('\0'),
        "Unsafe virtual path"
    );
    let mut out = PathBuf::new();
    for part in normalized.split('/') {
        ensure!(
            !part.is_empty() && part != "." && part != "..",
            "Unsafe virtual path component"
        );
        // Win32 trims dots/spaces and treats these names as devices, even with extensions.
        let stem = part.split('.').next().unwrap().to_uppercase();
        ensure!(
            !part.ends_with(['.', ' ']) && !part.chars().any(|c| c < ' ' || "<>\"|?*".contains(c)),
            "Unsafe Windows filename"
        );
        ensure!(
            !["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit()),
            "Reserved Windows filename"
        );
        out.push(part);
    }
    Ok(out)
}

pub fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let aa = a.as_bytes();
    let bb = b.as_bytes();
    let (mut i, mut j) = (0, 0);
    while i < aa.len() && j < bb.len() {
        if aa[i].is_ascii_digit() && bb[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < aa.len() && aa[i].is_ascii_digit() {
                i += 1;
            }
            while j < bb.len() && bb[j].is_ascii_digit() {
                j += 1;
            }
            let x = a[si..i].trim_start_matches('0');
            let y = b[sj..j].trim_start_matches('0');
            let cmp = x.len().cmp(&y.len()).then(x.cmp(y));
            if !cmp.is_eq() {
                return cmp;
            }
        } else {
            let cmp = aa[i].to_ascii_lowercase().cmp(&bb[j].to_ascii_lowercase());
            if !cmp.is_eq() {
                return cmp;
            }
            i += 1;
            j += 1;
        }
    }
    (aa.len() - i).cmp(&(bb.len() - j))
}

impl Catalog {
    pub fn load(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("Game folder is not accessible")?;
        ensure!(
            root.join("NBA2K27.exe").is_file(),
            "Choose the NBA 2K27 install folder containing NBA2K27.exe"
        );
        ensure!(
            root.join("data/oodle/oo2core_9_win64.dll").is_file(),
            "Installed game Oodle decoder not found"
        );
        let text = fs::read_to_string(root.join("manifest"))
            .context("Could not read installed manifest")?;
        let mut entries = Vec::new();
        let mut folders = BTreeMap::new();
        for line in text.lines() {
            let parts: Vec<_> = line.rsplitn(4, ',').collect();
            if parts.len() != 4 {
                continue;
            }
            let (Ok(size), Ok(offset)) = (parts[0].parse::<u64>(), parts[1].parse::<u64>()) else {
                continue;
            };
            let archive = parts[2];
            if archive.len() != 2
                || !archive
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            {
                continue;
            }
            let name = parts[3].replace('\\', "/");
            // Keep names searchable, but unsafe entries can never become filesystem paths.
            let mut folder = name.as_str();
            while let Some((parent, _)) = folder.rsplit_once('/') {
                *folders.entry(parent.to_string()).or_insert(0) += 1;
                folder = parent;
            }
            entries.push(Resource {
                id: entries.len(),
                name,
                archive: archive.to_owned(),
                offset,
                size,
            });
        }
        ensure!(
            !entries.is_empty(),
            "Manifest contains no recognized resources"
        );
        let mut sorted: Vec<_> = (0..entries.len()).collect();
        sorted.sort_by(|a, b| natural(&entries[*a].name, &entries[*b].name).then(a.cmp(b)));
        Ok(Self {
            root,
            entries,
            sorted,
            folders,
        })
    }
    pub fn entry(&self, id: usize) -> Result<&Resource> {
        self.entries
            .get(id)
            .context("Resource no longer exists; reload the catalog")
    }
    pub fn resolve(&self, name: &str) -> Result<&Resource> {
        let normalized = name.replace('\\', "/").to_lowercase();
        let leaf = normalized.rsplit('/').next().unwrap_or(&normalized);
        let mut candidates = vec![normalized.clone()];
        if let Some(hash) = leaf
            .rsplit('.')
            .nth(1)
            .filter(|s| s.len() == 16 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        {
            candidates.push(format!("shared/{}/{leaf}", &hash[..2]));
        }
        for candidate in candidates {
            if let Ok(position) = self
                .sorted
                .binary_search_by(|i| natural(&self.entries[*i].name, &candidate))
            {
                let entry = &self.entries[self.sorted[position]];
                if entry.name.eq_ignore_ascii_case(&candidate) {
                    return Ok(entry);
                }
            }
        }
        // For non-hashed references require an unambiguous leaf, never a fuzzy match.
        let mut matches = self.entries.iter().filter(|e| {
            e.name
                .rsplit('/')
                .next()
                .unwrap_or(&e.name)
                .eq_ignore_ascii_case(leaf)
        });
        let entry = matches
            .next()
            .context(format!("Unresolved manifest reference: {name}"))?;
        ensure!(
            matches.next().is_none(),
            "Ambiguous manifest reference: {name}"
        );
        Ok(entry)
    }
    pub fn search(&self, opts: &Value) -> Result<Value> {
        let query = opts["query"].as_str().unwrap_or("");
        ensure!(query.len() <= 512, "Search is limited to 512 characters");
        let regex = if opts["regex"].as_bool().unwrap_or(false) {
            Some(
                regex::RegexBuilder::new(query)
                    .case_insensitive(true)
                    .size_limit(2_000_000)
                    .build()
                    .context("Invalid regex")?,
            )
        } else {
            None
        };
        let needle = query.to_lowercase();
        let folder = opts["folder"].as_str().unwrap_or("").trim_matches('/');
        let prefix = format!("{folder}/");
        let kind = opts["kind"].as_str().unwrap_or("all");
        let offset = opts["offset"].as_u64().unwrap_or(0) as usize;
        let limit = opts["limit"].as_u64().unwrap_or(150).clamp(1, 500) as usize;
        let mut matches = Vec::new();
        for id in &self.sorted {
            let e = &self.entries[*id];
            if !folder.is_empty() && !e.name.starts_with(&prefix) {
                continue;
            }
            if kind != "all" && resource_kind(&e.name) != kind {
                continue;
            }
            if !needle.is_empty()
                && !regex
                    .as_ref()
                    .map(|r| r.is_match(&e.name))
                    .unwrap_or_else(|| e.name.to_lowercase().contains(&needle))
            {
                continue;
            }
            matches.push(*id);
        }
        if opts["sort"].as_str() == Some("manifest") {
            matches.sort_unstable();
        }
        if opts["descending"].as_bool().unwrap_or(false) {
            matches.reverse();
        }
        let items: Vec<_> = matches
            .iter()
            .skip(offset)
            .take(limit)
            .map(|id| &self.entries[*id])
            .collect();
        Ok(json!({"entries":items,"total":matches.len(),"offset":offset,"limit":limit}))
    }
    pub fn folder_list(&self, parent: &str) -> Value {
        let prefix = if parent.is_empty() {
            String::new()
        } else {
            format!("{}/", parent.trim_matches('/'))
        };
        let items: Vec<_> = self
            .folders
            .iter()
            .filter_map(|(path, count)| {
                let rest = path.strip_prefix(&prefix)?;
                (!rest.contains('/') && !rest.is_empty())
                    .then(|| json!({"name":rest,"path":path,"count":count}))
            })
            .collect();
        json!({"parent":parent,"folders":items})
    }
    pub fn path(&self, e: &Resource) -> Result<PathBuf> {
        if e.archive.len() != 2
            || !e
                .archive
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        {
            bail!("Unsafe archive id");
        }
        Ok(self.root.join(&e.archive))
    }
}

pub fn resource_kind(name: &str) -> &str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "iff" => "iff",
        "dds" => "dds",
        "tld" => "tld",
        "scne" => "scne",
        "txtr" => "txtr",
        "json" => "json",
        "txt" => "txt",
        "xml" => "xml",
        "lua" => "lua",
        "bin" => "bin",
        "gz" => "gz",
        "mip0" => "mip0",
        _ => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traversal_and_device_names_are_rejected() {
        for name in [
            "../x",
            "a/../../x",
            "/x",
            "C:/x",
            "a//x",
            "NUL.txt",
            "a/COM1.iff",
            "a/x.",
            "a/x ",
        ] {
            assert!(safe_relative(name).is_err(), "{name}");
        }
        assert!(safe_relative("char/sig/png04090.iff").is_ok());
    }
    #[test]
    fn natural_sort_is_numeric() {
        assert!(natural("png2.iff", "png10.iff").is_lt());
        assert!(natural("PNG2", "png02").is_eq());
    }
    #[test]
    fn kind_filter_and_manifest_order_match_explorer() {
        let c = Catalog {
            root: PathBuf::new(),
            entries: vec![
                Resource {
                    id: 0,
                    name: "b.iff".into(),
                    archive: "0A".into(),
                    offset: 0,
                    size: 1,
                },
                Resource {
                    id: 1,
                    name: "a.TXTR".into(),
                    archive: "0A".into(),
                    offset: 1,
                    size: 1,
                },
                Resource {
                    id: 2,
                    name: "a.iff".into(),
                    archive: "0B".into(),
                    offset: 0,
                    size: 1,
                },
            ],
            sorted: vec![2, 1, 0],
            folders: BTreeMap::new(),
        };
        assert_eq!(
            c.search(&json!({"kind":"iff","sort":"manifest"})).unwrap()["entries"][0]["id"],
            0
        );
        assert_eq!(c.search(&json!({"kind":"txtr"})).unwrap()["total"], 1);
        assert_eq!(
            c.search(&json!({"kind":"iff","descending":true})).unwrap()["entries"][0]["id"],
            0
        );
    }
}
