//! App-local WPF-compatible browsing. The shared asset resolver/order is intact.
use anyhow::{Context, Result, ensure};
pub use nba2k_assets::catalog::{Resource, natural, resource_kind, safe_relative};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    ops::Deref,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::{Duration, Instant},
};
const SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const SEARCH_DEADLINE: Duration = Duration::from_secs(8);

pub struct Catalog {
    inner: nba2k_assets::catalog::Catalog,
    browse_sorted: Vec<usize>,
    browse_folders: BTreeMap<String, FolderNode>,
    cache: Mutex<Option<(SearchKey, Arc<Vec<usize>>)>>,
    pub invalid_rows: usize,
}
#[derive(Default)]
struct FolderNode {
    direct_files: usize,
    children: BTreeMap<String, (String, usize)>,
}
#[derive(Clone, PartialEq, Eq)]
struct SearchKey {
    query: String,
    kind: String,
    folder: String,
    regex: bool,
    sorted: bool,
    descending: bool,
}
impl Deref for Catalog {
    type Target = nba2k_assets::catalog::Catalog;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Keep ordinal literal/folder matching separate from char-based natural order.
fn fold(value: &str) -> String {
    crate::catalog_casing::ordinal_fold(value)
}
pub fn classify(name: &str) -> String {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    match leaf
        .rsplit_once('.')
        .map(|(_, s)| s.to_ascii_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "iff" => "IFF".into(),
        "scne" => "SCNE".into(),
        "dds" | "png" | "jpg" | "jpeg" | "tga" => "Texture".into(),
        "wav" | "ogg" | "wem" | "bnk" => "Audio".into(),
        "json" | "xml" | "txt" | "csv" => "Data".into(),
        "bin" | "gz" | "vcz" => "Buffer".into(),
        "" => "Resource".into(),
        extension => extension.to_ascii_uppercase(),
    }
}
fn compare_units(a: &[u16], b: &[u16]) -> std::cmp::Ordering {
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if (48..=57).contains(&a[i]) && (48..=57).contains(&b[j]) {
            let (mut si, mut sj) = (i, j);
            while i < a.len() && (48..=57).contains(&a[i]) {
                i += 1;
            }
            while j < b.len() && (48..=57).contains(&b[j]) {
                j += 1;
            }
            while si < i && a[si] == 48 {
                si += 1;
            }
            while sj < j && b[sj] == 48 {
                sj += 1;
            }
            let order = (i - si).cmp(&(j - sj)).then(a[si..i].cmp(&b[sj..j]));
            if !order.is_eq() {
                return order;
            }
        } else {
            let order = a[i].cmp(&b[j]);
            if !order.is_eq() {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}
fn browse_natural(a: &str, b: &str) -> std::cmp::Ordering {
    // Case ordering around '_'/'[' differs from the shared resolver's lowercase
    // comparison. Do not change that resolver's sorted array to fix UI ordering.
    if a.is_ascii() && b.is_ascii() {
        let (aa, bb) = (a.as_bytes(), b.as_bytes());
        let (mut i, mut j) = (0, 0);
        while i < aa.len() && j < bb.len() {
            if aa[i].is_ascii_digit() && bb[j].is_ascii_digit() {
                let (mut si, mut sj) = (i, j);
                while i < aa.len() && aa[i].is_ascii_digit() {
                    i += 1;
                }
                while j < bb.len() && bb[j].is_ascii_digit() {
                    j += 1;
                }
                while si < i && aa[si] == b'0' {
                    si += 1;
                }
                while sj < j && bb[sj] == b'0' {
                    sj += 1;
                }
                let order = (i - si).cmp(&(j - sj)).then(aa[si..i].cmp(&bb[sj..j]));
                if !order.is_eq() {
                    return order;
                }
            } else {
                let order = aa[i].to_ascii_uppercase().cmp(&bb[j].to_ascii_uppercase());
                if !order.is_eq() {
                    return order;
                }
                i += 1;
                j += 1;
            }
        }
        return (aa.len() - i).cmp(&(bb.len() - j));
    }
    compare_units(
        &a.encode_utf16()
            .map(crate::catalog_casing::upper_unit)
            .collect::<Vec<_>>(),
        &b.encode_utf16()
            .map(crate::catalog_casing::upper_unit)
            .collect::<Vec<_>>(),
    )
}
fn folder_path(parent: &str) -> Result<String> {
    let normalized = parent.replace('\\', "/").trim_matches('/').to_owned();
    ensure!(
        !normalized
            .split('/')
            .any(|part| part == ".." || part.contains(':')),
        "Unsafe virtual folder path."
    );
    Ok(normalized)
}
fn integer(opts: &Value, field: &str, fallback: i32) -> i32 {
    opts[field]
        .as_i64()
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(fallback)
}
impl SearchKey {
    fn new(opts: &Value) -> Result<Self> {
        Ok(Self {
            query: opts["query"].as_str().unwrap_or("").into(),
            kind: opts["kind"].as_str().unwrap_or("all").into(),
            folder: folder_path(opts["folder"].as_str().unwrap_or(""))?,
            regex: opts["regex"].as_bool().unwrap_or(false),
            sorted: opts["sort"].as_str() == Some("name"),
            descending: opts["descending"].as_bool().unwrap_or(false)
                || opts["direction"].as_str() == Some("desc"),
        })
    }
}
enum AsciiPattern {
    Fast(regex::Regex),
    Fancy(fancy_regex::Regex),
}
struct Pattern {
    exact: crate::wpf_regex::Regex,
    ascii: Option<AsciiPattern>,
}
impl Pattern {
    fn new(pattern: &str) -> Result<Self> {
        ensure!(
            pattern.encode_utf16().count() <= 256,
            "Regex patterns are limited to 256 characters."
        );
        let exact =
            crate::wpf_regex::Regex::new(pattern).context("Invalid or unsupported regex")?;
        let ascii = if !exact.ascii_fast_safe() {
            None
        } else if let Ok(regex) = regex::RegexBuilder::new(pattern)
            .case_insensitive(true)
            .size_limit(2_000_000)
            .build()
        {
            Some(AsciiPattern::Fast(regex))
        } else {
            fancy_regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .delegate_size_limit(2_000_000)
                .backtrack_limit(100_000)
                .build()
                .ok()
                .map(AsciiPattern::Fancy)
        };
        Ok(Self { exact, ascii })
    }
    fn matches(&self, name: &str) -> Result<bool> {
        if name.is_ascii() && name.bytes().all(|c| c >= 32 && c != 127) {
            if !self.exact.ascii_possible() {
                return Ok(false);
            }
            match &self.ascii {
                Some(AsciiPattern::Fast(regex)) => return Ok(regex.is_match(name)),
                Some(AsciiPattern::Fancy(regex)) => {
                    return regex.is_match(name).context(
                        "Regex search exceeded its backtracking limit; simplify the pattern",
                    );
                }
                None => {}
            }
        }
        self.exact.is_match(name)
    }
}
impl Catalog {
    /// Developer fault injection for the existing range-guard CLI, not an IPC
    /// command. Size does not affect browser ordering or cached matching IDs.
    pub(crate) fn diagnostic_size(&mut self, id: usize, size: u64) -> Result<()> {
        self.inner
            .entries
            .get_mut(id)
            .context("Diagnostic resource missing")?
            .size = size;
        Ok(())
    }
    pub fn load(root: &Path) -> Result<Self> {
        Self::load_with_cancel(root, &AtomicBool::new(false))
    }
    pub fn load_with_cancel(root: &Path, cancel: &AtomicBool) -> Result<Self> {
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
        let (entries, invalid) =
            Self::parse_manifest(text.strip_prefix('\u{feff}').unwrap_or(&text), cancel)?;
        Self::from_entries(root, entries, invalid, cancel)
    }
    fn parse_manifest(text: &str, cancel: &AtomicBool) -> Result<(Vec<Resource>, usize)> {
        let (mut entries, mut invalid) = (Vec::new(), 0);
        for (position, line) in text.lines().enumerate() {
            if position & 255 == 0 {
                crate::codec::cancelled(cancel)?;
            }
            // Exact row grammar without a regex capture allocation per row.
            // rsplitn keeps commas within the authored resource name intact.
            let entry = (|| {
                let parts: Vec<_> = line.rsplitn(4, ',').collect();
                if parts.len() != 4
                    || parts[2].len() != 2
                    || !parts[2]
                        .bytes()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    || parts[..2]
                        .iter()
                        .any(|part| part.is_empty() || !part.bytes().all(|c| c.is_ascii_digit()))
                {
                    return None;
                }
                let offset = parts[1].parse::<u64>().ok()?;
                let size = parts[0].parse::<u64>().ok()?;
                (offset <= SAFE_INTEGER && size <= SAFE_INTEGER).then(|| Resource {
                    id: entries.len(),
                    name: parts[3].replace('\\', "/"),
                    archive: parts[2].into(),
                    offset,
                    size,
                })
            })();
            if let Some(entry) = entry {
                entries.push(entry);
            } else {
                invalid += 1;
            }
        }
        Ok((entries, invalid))
    }
    pub(crate) fn from_entries(
        root: PathBuf,
        entries: Vec<Resource>,
        invalid_rows: usize,
        cancel: &AtomicBool,
    ) -> Result<Self> {
        let mut sorted: Vec<_> = (0..entries.len()).collect();
        sorted.sort_by(|a, b| natural(&entries[*a].name, &entries[*b].name).then(a.cmp(b)));
        crate::codec::cancelled(cancel)?;
        let mut browse_sorted: Vec<_> = (0..entries.len()).collect();
        browse_sorted
            .sort_by(|a, b| browse_natural(&entries[*a].name, &entries[*b].name).then(a.cmp(b)));
        crate::codec::cancelled(cancel)?;
        let mut folders = BTreeMap::new();
        let mut browse_folders = BTreeMap::<String, FolderNode>::new();
        browse_folders.insert(String::new(), FolderNode::default());
        for (position, entry) in entries.iter().enumerate() {
            if position & 255 == 0 {
                crate::codec::cancelled(cancel)?;
            }
            let mut source_folder = entry.name.as_str();
            while let Some((parent, _)) = source_folder.rsplit_once('/') {
                *folders.entry(parent.to_owned()).or_insert(0) += 1;
                source_folder = parent;
            }
            let mut parent = String::new();
            let parts: Vec<_> = entry.name.split('/').collect();
            for part in parts.iter().take(parts.len().saturating_sub(1)) {
                let node = browse_folders.entry(fold(&parent)).or_default();
                let child = node
                    .children
                    .entry(fold(part))
                    .or_insert_with(|| ((*part).into(), 0));
                child.1 += 1;
                if !parent.is_empty() {
                    parent.push('/');
                }
                parent.push_str(part);
                browse_folders.entry(fold(&parent)).or_default();
            }
            browse_folders
                .entry(fold(&parent))
                .or_default()
                .direct_files += 1;
        }
        Ok(Self {
            inner: nba2k_assets::catalog::Catalog {
                root,
                entries,
                sorted,
                folders,
            },
            browse_sorted,
            browse_folders,
            cache: Mutex::new(None),
            invalid_rows,
        })
    }
    pub fn kinds(&self) -> Value {
        let mut counts = BTreeMap::<String, usize>::new();
        for entry in &self.entries {
            *counts.entry(classify(&entry.name)).or_default() += 1;
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        json!(
            counts
                .into_iter()
                .map(|(name, count)| json!({"name":name,"count":count}))
                .collect::<Vec<_>>()
        )
    }
    pub fn match_ids(&self, opts: &Value, cancel: &AtomicBool) -> Result<Arc<Vec<usize>>> {
        crate::codec::cancelled(cancel)?;
        let key = SearchKey::new(opts)?;
        {
            let cache = self
                .cache
                .lock()
                .map_err(|_| anyhow::anyhow!("Catalog search cache unavailable"))?;
            if let Some((old, matches)) = &*cache {
                if old == &key {
                    return Ok(matches.clone());
                }
            }
        }
        let query = key.query.trim();
        let pattern = if key.regex && !query.is_empty() {
            Some(Pattern::new(query)?)
        } else {
            None
        };
        let terms: Vec<_> = query.split_whitespace().map(fold).collect();
        let prefix = fold(&format!("{}/", key.folder));
        let deadline = Instant::now() + SEARCH_DEADLINE;
        let mut matches = Vec::new();
        for position in 0..self.entries.len() {
            if position & 63 == 0 {
                crate::codec::cancelled(cancel)?;
                ensure!(
                    Instant::now() <= deadline,
                    "Search took too long. Simplify the regex."
                );
            }
            let id = if key.sorted {
                self.browse_sorted[if key.descending {
                    self.entries.len() - 1 - position
                } else {
                    position
                }]
            } else {
                position
            };
            let entry = &self.entries[id];
            let folded = if !key.folder.is_empty() || pattern.is_none() && !terms.is_empty() {
                Some(fold(&entry.name))
            } else {
                None
            };
            if !key.folder.is_empty() && !folded.as_ref().unwrap().starts_with(&prefix) {
                continue;
            }
            if key.kind != "all"
                && key.kind != "All"
                && classify(&entry.name) != key.kind
                && !(key.kind == key.kind.to_ascii_lowercase()
                    && resource_kind(&entry.name) == key.kind)
            {
                continue;
            }
            if let Some(pattern) = &pattern {
                if !pattern.matches(&entry.name)? {
                    continue;
                }
            } else if !terms
                .iter()
                .all(|term| folded.as_ref().is_some_and(|name| name.contains(term)))
            {
                continue;
            }
            matches.push(id);
        }
        crate::codec::cancelled(cancel)?;
        let matches = Arc::new(matches);
        *self
            .cache
            .lock()
            .map_err(|_| anyhow::anyhow!("Catalog search cache unavailable"))? =
            Some((key, matches.clone()));
        Ok(matches)
    }
    pub fn search_with_cancel(&self, opts: &Value, cancel: &AtomicBool) -> Result<Value> {
        let matches = self.match_ids(opts, cancel)?;
        let offset = integer(opts, "offset", 0).max(0) as usize;
        let limit = integer(opts, "limit", 100).clamp(1, 250) as usize;
        let items: Vec<_> = matches
            .iter()
            .skip(offset)
            .take(limit)
            .map(|id| {
                let mut entry = serde_json::to_value(&self.entries[*id]).unwrap();
                entry["kind"] = json!(classify(&self.entries[*id].name));
                entry
            })
            .collect();
        Ok(json!({"entries":items,"total":matches.len(),"offset":offset,"limit":limit}))
    }
    pub fn search(&self, opts: &Value) -> Result<Value> {
        Self::search_with_cancel(self, opts, &AtomicBool::new(false))
    }
    pub fn folder_list(&self, parent: &str) -> Result<Value> {
        let normalized = folder_path(parent)?;
        let node = self.browse_folders.get(&fold(&normalized));
        let prefix = if normalized.is_empty() {
            String::new()
        } else {
            format!("{normalized}/")
        };
        let mut children: Vec<_> = node
            .into_iter()
            .flat_map(|node| node.children.values())
            .collect();
        children.sort_by(|a, b| crate::catalog_casing::ordinal_compare(&a.0, &b.0));
        let items: Vec<_> = children
            .into_iter()
            .map(
                |(name, count)| json!({"name":name,"path":format!("{prefix}{name}"),"count":count}),
            )
            .collect();
        Ok(
            json!({"parent":normalized,"directFiles":node.map_or(0,|node|node.direct_files),"folders":items}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_regex_path_matches_expanded_wpf_matrix() {
        let doc: Value = serde_json::from_str(include_str!(
            "../Tests/fixtures/wpf-regex-routing-cases.json"
        ))
        .unwrap();
        let mut cache =
            std::collections::HashMap::<String, std::result::Result<Pattern, String>>::new();
        let mut failures = Vec::new();
        for case in doc["cases"].as_array().unwrap() {
            let source = case["pattern"].as_str().unwrap();
            let compiled = cache
                .entry(source.into())
                .or_insert_with(|| Pattern::new(source).map_err(|e| e.to_string()));
            if case["compileError"] == true {
                if compiled.is_ok() {
                    failures.push(json!({"pattern":source,"unexpectedCompileSuccess":true}));
                }
                continue;
            }
            let input = case["input"].as_str().unwrap();
            let result = match compiled {
                Ok(pattern) => pattern.matches(input).map_err(|e| e.to_string()),
                Err(error) => Err(error.clone()),
            };
            if result.as_ref().ok().copied() != case["expected"].as_bool() {
                failures.push(json!({"pattern":source,"input":input,"expected":case["expected"],"actual":format!("{result:?}")}));
            }
        }
        assert!(
            failures.is_empty(),
            "{} shipping-path mismatches: {}",
            failures.len(),
            serde_json::to_string_pretty(&failures[..failures.len().min(24)]).unwrap()
        );
    }
    fn fixture() -> Catalog {
        let entries = [
            "Folder/A_Z.dds",
            "folder/AA.dds",
            "FOLDER/arena_2_int.iff",
            "Folder/arena_10_floor.iff",
            "folder/sub/eye.png",
            "folder/sub/eye.wav",
            "foo_foo.bin",
            "root.json",
            "root",
            "folder/face.txtr",
        ]
        .iter()
        .enumerate()
        .map(|(id, name)| Resource {
            id,
            name: (*name).into(),
            archive: "0A".into(),
            offset: 0,
            size: 0,
        })
        .collect();
        Catalog::from_entries(PathBuf::new(), entries, 0, &AtomicBool::new(false)).unwrap()
    }
    #[test]
    fn browse_sort_is_separate_from_asset_resolver_order() {
        let c = fixture();
        assert_eq!(
            c.search(&json!({"sort":"name"})).unwrap()["entries"][0]["id"],
            1
        );
        assert_eq!(c.sorted[0], 0);
        assert_eq!(browse_natural("a02", "A2"), std::cmp::Ordering::Equal);
        assert!(browse_natural("a2", "a10").is_lt());
        assert!(browse_natural("école", "Éclair").is_gt());
        // Natural sorting uppercases UTF16 chars, not whole supplementary runes.
        assert!(browse_natural("𐐀02", "𐐨2").is_lt());
        assert_eq!(browse_natural("ᾀ2", "ᾈ02"), std::cmp::Ordering::Equal);
        assert!(browse_natural("𐐨", "\u{e000}").is_lt());
    }
    #[test]
    fn unicode_literal_search_and_folder_identity_follow_ordinal_not_natural_casing() {
        let names = [
            "i/istanbul.iff",
            "ı/ıstanbul.iff",
            "s/sample.iff",
            "ſ/ſample.iff",
            "Σ/σ.iff",
            "ς/ς.iff",
            "𐐨/ᾀ.iff",
            "𐐀/ᾈ.iff",
            "\u{e000}/last.iff",
        ];
        let entries = names
            .iter()
            .enumerate()
            .map(|(id, name)| Resource {
                id,
                name: (*name).into(),
                archive: "0A".into(),
                offset: 0,
                size: 0,
            })
            .collect();
        let c = Catalog::from_entries(PathBuf::new(), entries, 0, &AtomicBool::new(false)).unwrap();
        assert_eq!(c.search(&json!({"query":"istanbul"})).unwrap()["total"], 1);
        assert_eq!(c.search(&json!({"query":"sample"})).unwrap()["total"], 1);
        assert_eq!(c.folder_list("ı").unwrap()["directFiles"], 1);
        assert_eq!(c.folder_list("i").unwrap()["directFiles"], 1);
        assert_eq!(c.folder_list("σ").unwrap()["directFiles"], 2);
        assert_eq!(c.folder_list("𐐨").unwrap()["directFiles"], 2);
        assert_eq!(c.search(&json!({"query":"ᾀ"})).unwrap()["total"], 2);
        let folders = c.folder_list("").unwrap()["folders"]
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(folders[folders.len() - 2]["name"], "\u{e000}");
        assert_eq!(folders[folders.len() - 1]["name"], "𐐨");
    }
    #[test]
    fn multi_terms_folder_case_and_grouped_kinds() {
        let c = fixture();
        assert_eq!(
            c.search(&json!({"query":"  arena int  ","folder":"/FOLDER/"}))
                .unwrap()["entries"][0]["id"],
            2
        );
        assert_eq!(c.search(&json!({"kind":"Texture"})).unwrap()["total"], 3);
        assert_eq!(c.search(&json!({"kind":"dds"})).unwrap()["total"], 2);
        assert_eq!(c.search(&json!({"kind":"Audio"})).unwrap()["total"], 1);
        assert_eq!(c.search(&json!({"kind":"Resource"})).unwrap()["total"], 1);
        assert_eq!(classify("folder.dot/root"), "Resource");
    }
    #[test]
    fn folders_merge_case_count_direct_files_and_reject_traversal() {
        let c = fixture();
        let root = c.folder_list("").unwrap();
        assert_eq!(root["directFiles"], 3);
        assert_eq!(root["folders"][0]["count"], 7);
        let folder = c.folder_list("/FOLDER\\sub/").unwrap();
        assert_eq!(folder["parent"], "FOLDER/sub");
        assert_eq!(folder["directFiles"], 2);
        assert!(c.folder_list("a/../b").is_err());
        assert!(c.folder_list("C:/game").is_err());
    }
    #[test]
    fn bounded_regex_supports_lookaround_backrefs_and_empty_trim() {
        let c = fixture();
        assert_eq!(
            c.search(&json!({"query":r"  (?<=folder/)AA\.dds$  ","regex":true}))
                .unwrap()["total"],
            1
        );
        assert_eq!(
            c.search(&json!({"query":r"(foo)_\1","regex":true}))
                .unwrap()["total"],
            1
        );
        assert_eq!(
            c.search(&json!({"query":r"arena_(?!10)","regex":true}))
                .unwrap()["total"],
            1
        );
        assert_eq!(
            c.search(&json!({"query":"   ","regex":true})).unwrap()["total"],
            10
        );
        assert!(c.search(&json!({"query":"[","regex":true})).is_err());
        assert!(
            c.search(&json!({"query":"a".repeat(257),"regex":true}))
                .is_err()
        );
    }
    #[test]
    fn warm_pages_reuse_only_current_snapshot_and_cancel_is_checked() {
        let c = fixture();
        let opts = json!({"sort":"name"});
        let first = c.match_ids(&opts, &AtomicBool::new(false)).unwrap();
        let second = c
            .match_ids(&json!({"sort":"name","offset":4}), &AtomicBool::new(false))
            .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert!(c.match_ids(&opts, &AtomicBool::new(true)).is_err());
        let next = c
            .match_ids(&json!({"query":"missing"}), &AtomicBool::new(false))
            .unwrap();
        assert!(!Arc::ptr_eq(&first, &next));
        assert_eq!(next.len(), 0);
    }
    #[test]
    fn paging_limits_offsets_and_manifest_direction_match_baseline() {
        let c = fixture();
        let p = c
            .search(&json!({"limit":500,"offset":-4,"descending":true,"sort":"manifest"}))
            .unwrap();
        assert_eq!(p["limit"], 250);
        assert_eq!(p["offset"], 0);
        assert_eq!(p["entries"][0]["id"], 0);
        assert_eq!(c.search(&json!({"limit":0})).unwrap()["limit"], 1);
    }
    #[test]
    fn manifest_rows_keep_ids_names_ranges_and_reject_unrepresentable_numbers() {
        let rows = "a.iff,0A,0,1\nwith,comma.iff,0A,1,2\nbad,0A,+1,2\nbad,0A,9007199254740992,2\nbad,0a,1,2";
        let (entries, invalid) = Catalog::parse_manifest(rows, &AtomicBool::new(false)).unwrap();
        assert_eq!(invalid, 3);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].id, 1);
        assert_eq!(entries[1].name, "with,comma.iff");
        assert!(Catalog::parse_manifest(rows, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn adversarial_regex_is_bounded_not_a_native_hang() {
        let pattern = Pattern::new(r"^(a|aa)+\1b$").unwrap();
        let started = Instant::now();
        let result = pattern.matches(&"a".repeat(128));
        assert!(matches!(result, Err(_) | Ok(false)));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
