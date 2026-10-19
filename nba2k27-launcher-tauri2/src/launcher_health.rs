//! WPF Settings "Diagnostics & compatibility" card and Developer tools.
//! Reference: core\launcher-health.js, core\game-compatibility.js,
//! core\support-bundle.js (redaction), core\bridge-audit.js, main.js:171-177.
//! Every action here is read-only toward the game; the only writes are the
//! launcher's own Logs/launcher-performance.json and the resolver cache.
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) fn now_iso() -> String {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    iso(d.as_millis() as i64)
}
fn iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Civil-from-days (H. Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms.rem_euclid(1000)
    )
}

/// Builds this port's native guards accept (`provider_access::verify_build`).
pub(crate) fn known_build(key: &str) -> Option<&'static str> {
    match key.to_ascii_uppercase().as_str() {
        "6AC53765:35BDE000" => Some("Version 2"),
        "6AA2013D:36558000" => Some("September build"),
        "6A974D13:368C3000" => Some("Earlier provider build"),
        _ => None,
    }
}

// ---------- Helper states (launcher-health.js:4-23) ----------

/// WPF labels, in WPF order, with the Rust status locations that carry the
/// same helper. Rust exposes helper states through the session status view
/// instead of per-channel events.
const HELPERS: [(&str, &str, &[&str]); 13] = [
    ("session", "Game session", &[""]),
    ("loader", "Session loader", &["/nativeProvider"]),
    ("camera", "Cameras", &["/nativeProvider/v2Features/features/cameras"]),
    ("scaler", "Scaler", &["/nativeProvider/v2Features/features/scaler"]),
    ("arena", "Arena", &["/nativeProvider/arena"]),
    ("ball", "Ball", &["/nativeProvider/ball"]),
    ("scoreboard", "Scoreboard", &["/nativeProvider/scoreboard"]),
    ("carry", "Outfit carry-over", &["/nativeProvider/outfits"]),
    ("crowd", "Crowd removal", &["/nativeProvider/v2Features/features/crowd", "/nativeProvider/presentation/crowd"]),
    ("presentation", "Drone / DJ / MC", &["/nativeProvider/v2Features/features/presentation", "/nativeProvider/presentation"]),
    ("freestyle", "Freestyle unlock", &["/nativeProvider/v2Features/features/wnba-freestyle"]),
    ("blacktop", "Blacktop teams", &["/nativeProvider/v2Features/features/wnba-blacktop"]),
    ("debug", "Debug menus", &[]),
];

#[derive(Clone)]
struct Observed {
    status: &'static str,
    phase: String,
    message: String,
    updated_at: String,
}
static OBSERVED: Mutex<BTreeMap<&'static str, Observed>> = Mutex::new(BTreeMap::new());

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}
/// `launcher-health.js:16-19` classification, first match wins.
pub(crate) fn classify(phase: &str) -> &'static str {
    static RES: OnceLock<[Regex; 4]> = OnceLock::new();
    let r = RES.get_or_init(|| {
        [
            re(r"(?i)error|failed|unsupported|attention|blocked"),
            re(r"(?i)^(off|inactive|disabled|idle|closed|stopped|restored|configured|saved)$"),
            re(r"(?i)pending|wait|prepar|arm|start|load|restor|monitor"),
            re(r"(?i)active|running|applied|ready"),
        ]
    });
    ["Failed", "Inactive", "Pending", "Active"]
        .into_iter()
        .zip(r.iter())
        .find(|(_, r)| r.is_match(phase))
        .map(|(s, _)| s)
        .unwrap_or("Unknown")
}
fn cap(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
/// Feed one status view (WPF `observe(channel, value)`).
pub(crate) fn observe(session: &Value) {
    let Ok(mut seen) = OBSERVED.lock() else {
        return;
    };
    for (key, _, pointers) in HELPERS {
        let Some(v) = pointers
            .iter()
            .filter_map(|p| session.pointer(p))
            .find(|v| v.is_object())
        else {
            continue;
        };
        let phase = v["phase"]
            .as_str()
            .or_else(|| v["status"].as_str())
            .unwrap_or("")
            .to_string();
        if phase.is_empty() {
            continue;
        }
        let message = v["message"]
            .as_str()
            .or_else(|| v["error"].as_str())
            .unwrap_or("")
            .to_string();
        let (phase, message) = (cap(&phase, 60), cap(&redact_text(&message, &[]), 1000));
        let changed = seen
            .get(key)
            .is_none_or(|o| o.phase != phase || o.message != message);
        if changed {
            seen.insert(
                key,
                Observed {
                    status: classify(&phase),
                    phase,
                    message,
                    updated_at: now_iso(),
                },
            );
        }
    }
}
fn helpers() -> Vec<Value> {
    let seen = OBSERVED.lock().map(|s| s.clone()).unwrap_or_default();
    HELPERS
        .iter()
        .map(|(key, label, _)| match (seen.get(key), *key) {
            (Some(o), _) => json!({"key":key,"label":label,"status":o.status,"phase":o.phase,"message":o.message,"updatedAt":o.updated_at}),
            // WPF core\debug-menus.js initial state; the Rust switch is disabled.
            (None, "debug") => json!({"key":key,"label":label,"status":"Inactive","phase":"off","message":"Debug menus are off.","updatedAt":null}),
            (None, _) => json!({"key":key,"label":label,"status":"Unknown","phase":"not-observed","message":"No status received this launcher session.","updatedAt":null}),
        })
        .collect()
}

// ---------- Performance (launcher-health.js:5-12, catalog-performance.js) ----------

const METRICS: [&str; 7] = [
    "startup",
    "shutdown",
    "catalog.ball",
    "catalog.scoreboard",
    "catalog.arena",
    "catalog.outfit",
    "catalog.rules",
];
fn perf_path(root: &Path) -> PathBuf {
    root.join("Logs/launcher-performance.json")
}
fn samples(root: &Path) -> Vec<Value> {
    let path = perf_path(root);
    match fs::metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= 256 * 1024 => {}
        _ => return vec![],
    }
    fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .filter(|v| v["schema"] == 1)
        .and_then(|v| v["samples"].as_array().cloned())
        .unwrap_or_default()
}
static PERF_LOCK: Mutex<()> = Mutex::new(());
pub(crate) fn record(root: &Path, name: &str, ms: f64, smoke: bool) -> Result<()> {
    ensure!(METRICS.contains(&name), "Unsupported timing event.");
    ensure!(ms.is_finite() && (0.0..=600000.0).contains(&ms), "Unsupported timing event.");
    let _g = PERF_LOCK.lock().map_err(|_| anyhow::anyhow!("Timing unavailable"))?;
    let mut all = samples(root);
    all.push(json!({"name":name,"ms":(ms*10.).round()/10.,"version":crate::backend::VERSION,"lane":if smoke {"qa"} else {"normal"},"at":now_iso()}));
    let skip = all.len().saturating_sub(300);
    let all: Vec<Value> = all.into_iter().skip(skip).collect();
    fs::create_dir_all(root.join("Logs"))?;
    crate::backend::atomic_json(&perf_path(root), &json!({"schema":1,"samples":all}))
}
fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let m = v.len() / 2;
    Some(if v.len() % 2 == 1 { v[m] } else { (v[m - 1] + v[m]) / 2. })
}
/// `summarize` (launcher-health.js:8-12): per metric and lane, last 20 of
/// this version; baseline = last 20 of the newest prior version.
fn summarize(all: &[Value]) -> Vec<Value> {
    let mut out = vec![];
    for lane in ["normal", "qa"] {
        for name in METRICS {
            let rows: Vec<&Value> = all
                .iter()
                .filter(|s| s["name"] == name && s["lane"] == lane && s["ms"].is_number())
                .collect();
            let current: Vec<f64> = rows
                .iter()
                .filter(|s| s["version"] == crate::backend::VERSION)
                .filter_map(|s| s["ms"].as_f64())
                .collect();
            let current: Vec<f64> = current[current.len().saturating_sub(20)..].to_vec();
            let prior_version = rows
                .iter()
                .rev()
                .find(|s| s["version"] != crate::backend::VERSION)
                .and_then(|s| s["version"].as_str());
            let prior: Vec<f64> = prior_version
                .map(|v| {
                    let p: Vec<f64> = rows
                        .iter()
                        .filter(|s| s["version"] == v)
                        .filter_map(|s| s["ms"].as_f64())
                        .collect();
                    p[p.len().saturating_sub(20)..].to_vec()
                })
                .unwrap_or_default();
            if current.is_empty() && prior.is_empty() && lane == "qa" {
                continue;
            }
            let (med, base) = (median(current.clone()), median(prior.clone()));
            let regression = current.len() >= 3
                && prior.len() >= 3
                && matches!((med, base), (Some(m), Some(b)) if m > b * 1.3 && m - b > 100.);
            out.push(json!({
                "name": name, "lane": lane, "count": current.len(),
                "lastMs": current.last(), "medianMs": med,
                "maxMs": current.iter().cloned().reduce(f64::max),
                "baselineMs": base, "baselineVersion": prior_version,
                "regression": regression,
            }));
        }
    }
    out
}

// ---------- Compatibility (game-compatibility.js) ----------

fn pe_identity(exe: &Path) -> Result<String> {
    let mut f = fs::File::open(exe)?;
    let mut h = [0u8; 4096];
    f.read_exact(&mut h)?;
    ensure!(&h[..2] == b"MZ", "Unsupported game executable");
    let pe = u32::from_le_bytes(h[0x3C..0x40].try_into()?) as usize;
    ensure!(
        pe >= 0x40
            && pe + 84 <= h.len()
            && &h[pe..pe + 4] == b"PE\0\0"
            && u16::from_le_bytes(h[pe + 4..pe + 6].try_into()?) == 0x8664
            && u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20B,
        "Unsupported game executable"
    );
    Ok(format!(
        "{:08X}:{:08X}",
        u32::from_le_bytes(h[pe + 8..pe + 12].try_into()?),
        u32::from_le_bytes(h[pe + 80..pe + 84].try_into()?)
    ))
}
/// Rows in WPF order. Support comes from this port's own build guards, so a
/// row is never marked supported on recognition alone (game-compatibility.js:4-5).
fn features(build: &Value) -> Vec<(&'static str, &'static str, bool)> {
    let label = build["label"].as_str().unwrap_or("");
    let v2 = label == "Version 2";
    let writes = build["nativeWritesMapped"] == true;
    let flag = |k: &str| build[k] == true;
    vec![
        ("session", "Game session", build["modProviderMapped"] == true),
        ("roster", "Roster / Team editor", v2),
        ("seasons", "Season history", false),
        ("camera", "Cameras", v2 || writes),
        ("arena", "Arena", flag("version2ArenaBallMapped") || writes),
        ("scoreboard", "Scoreboard", flag("version2ScoreboardMapped") || writes),
        ("outfits", "Outfits", flag("version2OutfitCarryMapped") || writes),
        ("ball", "Ball selection", flag("version2GameplayBallMapped") || writes),
        ("rules", "Game rules", v2 || writes),
        ("crowd", "Crowd removal", flag("observerMapped")),
        ("debug", "Debug menus", false),
        ("scaler", "Scaler", v2 || writes),
    ]
}
pub(crate) fn compatibility(game_root: &Path) -> Value {
    let exe = game_root.join("NBA2K27.exe");
    let checked = now_iso();
    if !exe.is_file() {
        return json!({"build":null,"checkedAt":checked,"features":[],"known":false,"message":"Choose the installed NBA 2K27 folder."});
    }
    let key = pe_identity(&exe).ok();
    let guards = crate::provider_access::verify_build(&exe).map(|v| v.build);
    let known = guards.is_ok();
    let build = guards.as_ref().cloned().unwrap_or(json!({}));
    let rows: Vec<Value> = features(&build)
        .into_iter()
        .map(|(key, label, supported)| {
            let reason = if !known {
                "This game build is not supported by this feature. No new edit will be attempted."
            } else if supported {
                "Build recognized; native safety checks still apply."
            } else if key == "debug" || key == "seasons" {
                "Not available in this launcher for this game build yet. No game write is attempted."
            } else {
                "This game build is not supported by this feature. No new edit will be attempted."
            };
            json!({"key":key,"label":label,"supported":known && supported,"reason":reason})
        })
        .collect();
    json!({
        "build": key,
        "buildLabel": build["label"].as_str().map(str::to_string).or_else(|| key.as_deref().and_then(known_build).map(str::to_string)),
        "checkedAt": checked,
        "features": rows,
        "known": known,
        "message": if known {
            "Compatibility checked against the native helper build guards.".to_string()
        } else {
            "Unrecognized game update. Library management and recovery remain available.".to_string()
        },
        "guardError": guards.err().map(|e| format!("{e:#}")),
    })
}

/// `launcher:health` result.
pub(crate) fn report(root: &Path, game_root: &Path, session: Option<&Value>) -> Value {
    if let Some(s) = session {
        observe(s);
    }
    json!({
        "version": crate::backend::VERSION,
        "helpers": helpers(),
        "performance": summarize(&samples(root)),
        "compatibility": compatibility(game_root),
        "measuredAt": now_iso(),
    })
}

// ---------- Redaction (support-bundle.js redactValue) ----------

fn redact_text(text: &str, known: &[String]) -> String {
    static USER: OnceLock<Regex> = OnceLock::new();
    static TOKEN: OnceLock<Regex> = OnceLock::new();
    let mut out = text.to_string();
    for k in known.iter().filter(|k| k.len() > 3) {
        out = Regex::new(&format!("(?i){}", regex::escape(k)))
            .map(|r| r.replace_all(&out, "<redacted-path>").into_owned())
            .unwrap_or(out);
    }
    out = USER
        .get_or_init(|| re(r#"(?i)[A-Z]:[\\/]+Users[\\/]+[^\\/\r\n"']+"#))
        .replace_all(&out, "<user-profile>")
        .into_owned();
    TOKEN
        .get_or_init(|| re(r"(?i)\b((?:token|secret|password|nonce)=)[^\s&]+"))
        .replace_all(&out, "${1}<redacted>")
        .into_owned()
}
fn redact(value: &Value, key: &str, known: &[String]) -> Value {
    static SECRET: OnceLock<Regex> = OnceLock::new();
    static PATHKEY: OnceLock<Regex> = OnceLock::new();
    let secret = SECRET.get_or_init(|| re(r"(?i)(token|secret|password|credential|authorization|cookie)$"));
    let pathkey = PATHKEY.get_or_init(|| re(r"(?i)(path|root|directory|folder|file|target|executable)$"));
    match value {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| (k.clone(), redact(v, k, known)))
                .collect::<Map<_, _>>(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(|v| redact(v, key, known)).collect()),
        // Deliberate divergence: WPF's `key$` rule also blanked each feature
        // row's `key` field; Rust keeps those ids readable.
        Value::String(_) if secret.is_match(key) => json!("<redacted>"),
        Value::String(s) if pathkey.is_match(key) && Path::new(s).is_absolute() => {
            let leaf = Path::new(s)
                .file_name()
                .map(|l| l.to_string_lossy().to_string())
                .unwrap_or_default();
            json!(format!("<redacted-path>\\{leaf}"))
        }
        Value::String(s) => json!(redact_text(s, known)),
        v => v.clone(),
    }
}
/// `launcher:copy-health` text: the same report, redacted, pretty JSON with
/// two-space indent and no trailing newline. The UI writes it to the clipboard.
pub(crate) fn copy_text(report: &Value, known: &[String]) -> Result<String> {
    Ok(serde_json::to_string_pretty(&redact(report, "", known))?)
}

// ---------- Developer tools ----------

/// WPF "Test provider core" (main.js:1082-1091), Rust mechanism: the native
/// provider worker indexes the mods folder in a private child, answers one
/// query, must refuse a foreign game identity, then exits. No game is
/// launched, attached or written.
pub(crate) fn provider_test(root: &Path, mods_root: &Path, logical: &str) -> Value {
    let logical = logical.trim().replace('\\', "/");
    let outcome = (|| -> Result<Value> {
        ensure!(
            !logical.is_empty()
                && logical.len() <= 260
                && !logical.split('/').any(|p| p.is_empty() || p == "." || p == "..")
                && !logical.contains(':'),
            "Enter a relative asset path inside the mods folder"
        );
        ensure!(mods_root.is_dir(), "Mods folder not found");
        let fixture = root.join("Cache/ProviderTest");
        fs::create_dir_all(&fixture)?;
        let started = std::time::Instant::now();
        let mut worker =
            crate::provider_worker::Worker::start(&fixture.join("NBA2K27.exe"), mods_root, &[], &[])?;
        let result = (|| -> Result<Value> {
            let status = worker.control(json!({"operation":"status"}))?;
            let query = worker.control(json!({"operation":"query","logicalPath":logical}));
            let mut foreign = worker.identity.clone();
            foreign.started = "1".into();
            let denied = worker.bind_game(&foreign).is_err();
            Ok(json!({"status":status,"query":query.map_err(|e| format!("{e:#}")),"foreignDenied":denied}))
        })();
        worker.stop()?;
        let exited = worker.exited()?;
        let r = result?;
        let size = r["query"]["Ok"]["size"].as_u64();
        let passed = size.is_some() && r["foreignDenied"] == true && exited;
        Ok(json!({
            "passed": passed, "path": logical, "bytes": size,
            "indexedAssets": r["status"]["indexedAssets"],
            "foreignGameIdentityDenied": r["foreignDenied"], "helperExited": exited,
            "elapsedMs": started.elapsed().as_millis() as u64,
            "error": r["query"]["Err"],
            "message": if passed {
                format!("PASS · {logical} · {} bytes indexed · foreign game identity rejected YES · helper closed.", size.unwrap_or(0))
            } else {
                format!("FAIL · {}", r["query"]["Err"].as_str().unwrap_or(if exited {"provider check did not pass"} else {"helper did not exit"}))
            },
        }))
    })();
    outcome.unwrap_or_else(|e| json!({"passed":false,"path":logical,"message":format!("FAIL · {e:#}")}))
}

fn sha256_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
/// WPF "Run bridge audit" (bridge-audit.js:124-248), offline and read-only:
/// exact build identity, stock manifest listing, loose asset presence and the
/// resolver outcome. It never launches the game, attaches, or writes the
/// game folder.
pub(crate) fn bridge_audit(root: &Path, game_root: &Path, mods_root: &Path, logical: &str) -> Result<Value> {
    let exe = game_root.join("NBA2K27.exe");
    ensure!(exe.is_file(), "Choose the installed NBA 2K27 folder.");
    let logical = logical.trim().replace('\\', "/");
    ensure!(
        !logical.is_empty() && !logical.split('/').any(|p| p.is_empty() || p == ".."),
        "Enter a relative asset path"
    );
    let identity = pe_identity(&exe)?;
    let sha = sha256_file(&exe)?;
    let label = known_build(&identity);
    let leaf = logical.rsplit('/').next().unwrap_or(&logical).to_ascii_lowercase();
    let manifest = game_root.join("manifest");
    let listed = if manifest.is_file() {
        let mut text = String::new();
        fs::File::open(&manifest)?
            .take(64 * 1024 * 1024)
            .read_to_string(&mut text)
            .ok();
        let lower = text.to_ascii_lowercase();
        Some(lower.contains(&logical.to_ascii_lowercase()) || lower.contains(&leaf))
    } else {
        None
    };
    let loose = mods_root.join(&logical);
    let resolver = crate::build_resolver::status(root, game_root, false)?;
    let line = format!(
        "READ-ONLY RESULT · exact build {} · stock manifest {} · loose asset {} · provider layout {} · live registration result PENDING.",
        if label.is_some() { "YES" } else { "NO" },
        match listed { Some(true) => "LISTED", Some(false) => "ABSENT", None => "NOT FOUND" },
        if loose.is_file() { "PRESENT" } else { "MISSING" },
        resolver["status"].as_str().unwrap_or("unknown").to_ascii_uppercase(),
    );
    Ok(json!({
        "mode": "offline-read-only",
        "build": {"identity": identity, "label": label, "sha256": sha, "recognized": label.is_some()},
        "manifest": {"present": listed.is_some(), "listed": listed},
        "asset": {"path": logical, "present": loose.is_file(), "bytes": fs::metadata(&loose).ok().map(|m| m.len())},
        "resolver": {"status": resolver["status"], "message": resolver["message"]},
        "safety": {"launchedGame":false,"attachedToProcess":false,"wroteGameFolder":false,"modifiedManifest":false},
        "message": line,
    }))
}

pub(crate) fn known_paths(settings: &crate::backend::Settings, root: &Path) -> Vec<String> {
    let mut v = vec![
        settings.game_root.clone(),
        settings.mods_root.clone(),
        root.to_string_lossy().to_string(),
    ];
    v.sort_by_key(|s| std::cmp::Reverse(s.len()));
    v
}
pub(crate) fn require_text(args: &Value, key: &str, default: &str) -> Result<String> {
    Ok(match args.get(key) {
        None | Some(Value::Null) => default.into(),
        Some(v) => v.as_str().context("Invalid text argument")?.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classification_matches_wpf_order() {
        assert_eq!(classify("needs-review"), "Unknown");
        assert_eq!(classify("failed"), "Failed");
        assert_eq!(classify("idle"), "Inactive");
        assert_eq!(classify("waiting-for-game"), "Pending");
        assert_eq!(classify("game-running"), "Active");
        assert_eq!(classify("restored"), "Inactive");
        assert_eq!(classify("unsupported-build"), "Failed");
    }
    #[test]
    fn regression_needs_three_samples_and_both_thresholds() {
        let v = crate::backend::VERSION;
        let mk = |ver: &str, ms: f64| json!({"name":"catalog.ball","ms":ms,"version":ver,"lane":"normal"});
        let mut all: Vec<Value> = (0..3).map(|_| mk("0.0.1", 100.)).collect();
        all.extend((0..3).map(|_| mk(v, 250.)));
        let row = summarize(&all).into_iter().find(|r| r["name"] == "catalog.ball").unwrap();
        assert_eq!(row["regression"], true);
        assert_eq!(row["baselineMs"], 100.);
        all.truncate(5);
        let row = summarize(&all).into_iter().find(|r| r["name"] == "catalog.ball").unwrap();
        assert_eq!(row["regression"], false);
    }
    #[test]
    fn copy_report_redacts_paths_and_secrets() {
        let report = json!({"gameExecutable":"C:\\Games\\NBA 2K27\\NBA2K27.exe","message":"C:\\Users\\Alice\\x and D:\\Mods\\a token=abc","authToken":"x","key":"session"});
        let t = copy_text(&report, &["D:\\Mods".into()]).unwrap();
        assert!(t.contains("<redacted-path>\\\\NBA2K27.exe"));
        assert!(t.contains("<user-profile>"));
        assert!(!t.contains("Alice") && !t.contains("D:\\\\Mods") && !t.contains("abc"));
        assert!(t.contains("\"key\": \"session\""));
        assert!(!t.ends_with('\n'));
    }
    #[test]
    fn iso_dates_are_utc() {
        assert_eq!(iso(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso(1_792_195_200_123), "2026-10-17T00:00:00.123Z");
    }
}
