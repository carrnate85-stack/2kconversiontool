//! Read-only Steam library discovery. Parses Steam's libraryfolders.vdf so a
//! game installed in a secondary library can resolve to the Steam client that
//! owns that library. Nothing here launches, writes or trusts a path without
//! the exact-path identity checks in session_safety::resolve.
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_VDF_BYTES: u64 = 1024 * 1024;
const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 8192;
const MAX_LIBRARIES: usize = 64;

#[derive(Debug, PartialEq)]
enum Node {
    Text(String),
    Block(Vec<(String, Node)>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Library {
    pub path: PathBuf,
    /// App IDs Steam lists for this library; None for the legacy format.
    pub apps: Option<HashSet<String>>,
}

fn tokens(text: &str) -> Result<Vec<(bool, String)>> {
    // (quoted, value). Quoted strings only; braces are structural.
    let mut out = Vec::new();
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            for c in chars.by_ref() {
                if c == '\n' {
                    break;
                }
            }
            continue;
        }
        if c == '{' || c == '}' {
            out.push((false, c.to_string()));
            continue;
        }
        ensure!(c == '"', "Malformed Steam library file");
        let mut value = String::new();
        let mut ended = false;
        while let Some(c) = chars.next() {
            if c == '"' {
                ended = true;
                break;
            }
            if c == '\\' {
                match chars.next().context("Invalid Steam library escape")? {
                    '\\' => value.push('\\'),
                    '"' => value.push('"'),
                    'n' => value.push('\n'),
                    't' => value.push('\t'),
                    _ => bail!("Unsupported Steam library escape"),
                }
            } else {
                value.push(c);
            }
        }
        ensure!(ended, "Unterminated Steam library text");
        out.push((true, value));
        ensure!(out.len() <= MAX_ENTRIES * 2, "Steam library file too large");
    }
    Ok(out)
}

fn block(tokens: &[(bool, String)], at: &mut usize, depth: usize) -> Result<Vec<(String, Node)>> {
    ensure!(depth <= MAX_DEPTH, "Steam library nesting exceeds limit");
    let mut rows = Vec::new();
    loop {
        let Some((quoted, key)) = tokens.get(*at) else {
            ensure!(depth == 0, "Incomplete Steam library file");
            return Ok(rows);
        };
        if !quoted && key == "}" {
            ensure!(depth > 0, "Invalid Steam library nesting");
            *at += 1;
            return Ok(rows);
        }
        ensure!(*quoted, "Missing Steam library key");
        *at += 1;
        let (quoted, value) = tokens.get(*at).context("Missing Steam library value")?;
        *at += 1;
        let node = if *quoted {
            Node::Text(value.clone())
        } else {
            ensure!(value == "{", "Missing Steam library value");
            Node::Block(block(tokens, at, depth + 1)?)
        };
        rows.push((key.clone(), node));
    }
}

fn library_path(text: &str) -> Option<PathBuf> {
    let path = PathBuf::from(text.trim());
    (path.is_absolute() && !text.contains('\0')).then_some(path)
}

/// Parses both the current (`"libraryfolders"` with path/apps blocks) and the
/// legacy (`"LibraryFolders"` with numbered path strings) formats.
pub(crate) fn parse_library_folders(text: &str) -> Result<Vec<Library>> {
    ensure!(
        text.len() as u64 <= MAX_VDF_BYTES,
        "Steam library file too large"
    );
    let tokens = tokens(text)?;
    let mut at = 0;
    let root = block(&tokens, &mut at, 0)?;
    ensure!(at == tokens.len(), "Trailing Steam library data");
    let [(name, Node::Block(entries))] = root.as_slice() else {
        bail!("Missing libraryfolders root");
    };
    ensure!(
        name.eq_ignore_ascii_case("libraryfolders"),
        "Missing libraryfolders root"
    );
    let mut libraries = Vec::new();
    for (key, node) in entries {
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let library = match node {
            Node::Text(path) => library_path(path).map(|path| Library { path, apps: None }),
            Node::Block(fields) => {
                let mut path = None;
                let mut apps = None;
                for (field, value) in fields {
                    match (field.to_ascii_lowercase().as_str(), value) {
                        ("path", Node::Text(text)) => {
                            ensure!(path.is_none(), "Duplicate Steam library path");
                            path = Some(text.clone());
                        }
                        ("apps", Node::Block(ids)) => {
                            apps = Some(
                                ids.iter()
                                    .map(|(id, _)| id.clone())
                                    .filter(|id| {
                                        !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
                                    })
                                    .collect::<HashSet<_>>(),
                            );
                        }
                        _ => {}
                    }
                }
                path.as_deref()
                    .and_then(library_path)
                    .map(|path| Library { path, apps })
            }
        };
        if let Some(library) = library {
            libraries.push(library);
            ensure!(libraries.len() <= MAX_LIBRARIES, "Too many Steam libraries");
        }
    }
    Ok(libraries)
}

fn bounded_text(path: &Path) -> Result<String> {
    let file = fs::File::open(path)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.len() <= MAX_VDF_BYTES,
        "Steam library file exceeds its limit"
    );
    let mut bytes = Vec::new();
    file.take(MAX_VDF_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_VDF_BYTES,
        "Steam library file changed or exceeds its limit"
    );
    Ok(String::from_utf8(bytes)?)
}

/// Libraries listed by one Steam install (current then legacy location).
pub(crate) fn libraries_for(steam_root: &Path) -> Result<Vec<Library>> {
    for candidate in [
        steam_root.join("steamapps").join("libraryfolders.vdf"),
        steam_root.join("config").join("libraryfolders.vdf"),
    ] {
        if candidate.is_file() {
            return parse_library_folders(&bounded_text(&candidate)?);
        }
    }
    bail!("Steam libraryfolders.vdf not found")
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Finds the Steam client whose libraryfolders.vdf lists `library` (the folder
/// that contains `steamapps`). Returns steam.exe and the listed app IDs.
pub(crate) fn steam_for_library(
    library: &Path,
    roots: &[PathBuf],
) -> Result<(PathBuf, Option<HashSet<String>>)> {
    let mut found = None;
    for root in roots {
        let steam = root.join("steam.exe");
        if !steam.is_file() {
            continue;
        }
        let Ok(libraries) = libraries_for(root) else {
            continue;
        };
        for entry in libraries {
            if same_dir(&entry.path, library) {
                let steam = steam.canonicalize()?;
                match &found {
                    Some((existing, _)) if existing != &steam => {
                        bail!("This library is listed by more than one Steam install")
                    }
                    Some(_) => {}
                    None => found = Some((steam, entry.apps)),
                }
                break;
            }
        }
    }
    found.context("steam.exe not found above this library and no Steam libraryfolders.vdf lists it")
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        hkey: isize,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut core::ffi::c_void,
        len: *mut u32,
    ) -> i32;
}

fn registry_text(hkey: u32, subkey: &str, value: &str) -> Option<String> {
    const RRF_RT_REG_SZ: u32 = 0x2;
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (subkey, value) = (wide(subkey), wide(value));
    let mut buffer = vec![0u16; 1024];
    let mut len = (buffer.len() * 2) as u32;
    // Predefined HKEY values are sign-extended 32-bit constants.
    let status = unsafe {
        RegGetValueW(
            hkey as i32 as isize,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if status != 0 {
        return None;
    }
    let chars = (len as usize / 2).min(buffer.len());
    let text = String::from_utf16(&buffer[..chars]).ok()?;
    let text = text.trim_end_matches('\0').trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Candidate Steam install roots: registry first, then default locations.
pub(crate) fn steam_roots() -> Vec<PathBuf> {
    const HKCU: u32 = 0x8000_0001;
    const HKLM: u32 = 0x8000_0002;
    let mut candidates = Vec::new();
    candidates.extend(registry_text(HKCU, r"Software\Valve\Steam", "SteamPath"));
    candidates.extend(registry_text(
        HKLM,
        r"SOFTWARE\WOW6432Node\Valve\Steam",
        "InstallPath",
    ));
    candidates.extend(registry_text(HKLM, r"SOFTWARE\Valve\Steam", "InstallPath"));
    for var in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(base) = std::env::var_os(var) {
            candidates.push(
                Path::new(&base)
                    .join("Steam")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.join("steam.exe").is_file())
        .filter(|path| seen.insert(path.canonicalize().unwrap_or_else(|_| path.clone())))
        .collect()
}

/// Game roots named by NBA 2K27 app manifests across every listed library.
/// Callers must still pass each candidate through session_safety::resolve.
pub(crate) fn candidate_game_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut libraries = Vec::new();
    for root in roots {
        libraries.push(root.clone());
        if let Ok(rows) = libraries_for(root) {
            libraries.extend(rows.into_iter().map(|row| row.path));
        }
    }
    let mut seen = HashSet::new();
    let mut games = Vec::new();
    for library in libraries {
        let Ok(library) = library.canonicalize() else {
            continue;
        };
        if !seen.insert(library.clone()) {
            continue;
        }
        let apps = library.join("steamapps");
        let Ok(rows) = fs::read_dir(&apps) else {
            continue;
        };
        for row in rows.flatten().take(512) {
            let name = row.file_name().to_string_lossy().to_ascii_lowercase();
            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = bounded_text(&row.path()) else {
                continue;
            };
            let Ok(manifest) = crate::session_safety::manifest(&text) else {
                continue;
            };
            if manifest.name.eq_ignore_ascii_case("NBA 2K27") {
                let game = apps.join("common").join(&manifest.install_dir);
                if game.join("NBA2K27.exe").is_file() {
                    games.push(game);
                }
            }
        }
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    const CURRENT: &str = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"contentid"		"123"
		"apps"
		{
			"228980"		"1000"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"label"		"Games \"fast\""
		"apps"
		{
			"3472040"		"150000000000"
			"not-an-id"		"1"
		}
	}
}
"#;

    #[test]
    fn parses_current_library_format_with_apps() {
        let rows = parse_library_folders(CURRENT).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].path, PathBuf::from(r"C:\Program Files (x86)\Steam"));
        assert_eq!(rows[1].path, PathBuf::from(r"D:\SteamLibrary"));
        let apps = rows[1].apps.as_ref().unwrap();
        assert!(apps.contains("3472040"));
        assert!(!apps.contains("not-an-id"));
    }

    #[test]
    fn parses_legacy_library_format() {
        let text = "\u{feff}\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"1\"\n\t\"ContentStatsID\"\t\"-1\"\n\t\"1\"\t\"E:\\\\Steam Games\"\n\t// comment\n\t\"2\"\t\"relative\\\\ignored\"\n}\n";
        let rows = parse_library_folders(text).unwrap();
        assert_eq!(
            rows,
            vec![Library {
                path: PathBuf::from(r"E:\Steam Games"),
                apps: None
            }]
        );
    }

    #[test]
    fn rejects_malformed_or_hostile_library_files() {
        for bad in [
            "",
            "\"libraryfolders\"",
            "\"libraryfolders\" {",
            "\"libraryfolders\" { \"0\" { \"path\" \"C:\\\\A\" } } }",
            "\"other\" { }",
            "libraryfolders { }",
            "\"libraryfolders\" { \"0\" \"C:\\x\" }",
            "\"libraryfolders\" { \"0\" { \"path\" \"C:\\\\A\" \"path\" \"D:\\\\B\" } }",
            "\"a\" { } \"b\" { }",
        ] {
            assert!(parse_library_folders(bad).is_err(), "{bad:?}");
        }
        let deep = format!(
            "\"libraryfolders\" {}{}",
            "{ \"k\" ".repeat(MAX_DEPTH + 2),
            "}".repeat(MAX_DEPTH + 3)
        );
        assert!(parse_library_folders(&deep).is_err());
    }

    #[test]
    fn steam_for_library_matches_only_listed_libraries() {
        let root = std::env::temp_dir().join(format!(
            "launcher-steam-libraries-{}",
            crate::backend::unique_id()
        ));
        let steam = root.join("Steam");
        let library = root.join("Library Two");
        let other = root.join("Unlisted");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::create_dir_all(library.join("steamapps")).unwrap();
        fs::create_dir_all(other.join("steamapps")).unwrap();
        fs::write(steam.join("steam.exe"), b"MZ").unwrap();
        let escaped = library.to_string_lossy().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!("\"libraryfolders\" {{ \"1\" {{ \"path\" \"{escaped}\" \"apps\" {{ \"42\" \"1\" }} }} }}"),
        )
        .unwrap();
        let (exe, apps) = steam_for_library(&library, &[steam.clone()]).unwrap();
        assert_eq!(exe, steam.join("steam.exe").canonicalize().unwrap());
        assert!(apps.unwrap().contains("42"));
        assert!(steam_for_library(&other, &[steam.clone()]).is_err());
        assert!(steam_for_library(&library, &[]).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
