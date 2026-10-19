//! Explicit, read-only review of the frozen WPF v1 logical profile store.
//! No legacy preference activates itself. Only selected, freshly revalidated
//! rows enter the v2 store, always disabled. No game/process capabilities.
use super::*;
use crate::{file_identity::Stable, loader_files::Directories};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Seek, SeekFrom},
};
const LEGACY_SCHEMA: &str = "nba2k27-custom-arena-profiles-v1";
const REVIEW_SCHEMA: &str = "nba2k27-custom-arena-profiles-review-v1";
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Legacy {
    schema: String,
    #[serde(default)]
    updated_at: Option<String>,
    profiles: Vec<Value>,
}
#[derive(Serialize)]
struct Issue {
    field: String,
    message: String,
}
struct Row {
    id: String,
    index: usize,
    profile: Option<Profile>,
    issues: Vec<Issue>,
}
struct Source {
    _file: File,
    _dirs: Directories,
    token: String,
    rows: Vec<Row>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_source(root: &Path) -> Result<Option<Source>> {
    let path = root.join("Data/player-research/custom-arena-profiles.json");
    // A missing legacy file is absence, not an invalid/empty import authority.
    let mut file = match OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let dirs = Directories::pin(path.parent().context("Legacy profile parent missing")?)?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file() && meta.file_attributes() & 0x400 == 0 && meta.len() <= LIMIT as u64,
        "Legacy profiles must be a bounded ordinary file"
    );
    let id = Stable::read(&file)?;
    let mut bytes = Vec::new();
    (&mut file).take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() == meta.len() as usize,
        "Legacy profile file was partially read"
    );
    file.seek(SeekFrom::Start(0))?;
    let mut again = Vec::new();
    (&mut file).take(LIMIT as u64 + 1).read_to_end(&mut again)?;
    ensure!(
        bytes == again
            && file.metadata()?.len() == meta.len()
            && file.metadata()?.last_write_time() == meta.last_write_time()
            && Stable::read(&file)? == id,
        "Legacy profiles changed while reading"
    );
    let document: Legacy = serde_json::from_slice(&bytes)
        .context("Unsupported or malformed WPF legacy profiles; source left untouched")?;
    ensure!(
        document.schema == LEGACY_SCHEMA
            && document.profiles.len() <= 50
            && document.updated_at.as_ref().is_none_or(|s| s.len() <= 128),
        "Unsupported legacy profile schema or size"
    );
    let token = digest(&serde_json::to_vec(
        &json!({"source":"wpf-fixed-arena-profiles-v1","fileId":id,"sha256":digest(&bytes)}),
    )?);
    let mut rows = document
        .profiles
        .iter()
        .enumerate()
        .map(|(index, v)| {
            let id = digest(format!("{token}:{index}").as_bytes());
            let (profile, issues) = decode(v);
            Row {
                id,
                index,
                profile,
                issues,
            }
        })
        .collect::<Vec<_>>();
    // Reject BOTH sides of ambiguous source rows. Selection never resolves an
    // ambiguous slot/team by list order or silently choosing the first copy.
    let mut ambiguous = BTreeSet::new();
    for a in 0..rows.len() {
        for b in a + 1..rows.len() {
            if let (Some(x), Some(y)) = (&rows[a].profile, &rows[b].profile) {
                if x.target_arena_id == y.target_arena_id
                    || (x.team_id == y.team_id && x.expected_team_name == y.expected_team_name)
                {
                    ambiguous.insert(a);
                    ambiguous.insert(b);
                }
            }
        }
    }
    for index in ambiguous {
        rows[index].profile = None;
        rows[index].issues.push(Issue {
            field: "targetArenaId/teamId".into(),
            message: "Ambiguous legacy target or team; resolve the source entries before importing"
                .into(),
        });
    }
    Ok(Some(Source {
        _file: file,
        _dirs: dirs,
        token,
        rows,
    }))
}
fn decode(value: &Value) -> (Option<Profile>, Vec<Issue>) {
    let mut issues = Vec::new();
    let mut bad = |field: &str, message: &str| {
        issues.push(Issue {
            field: field.into(),
            message: message.into(),
        })
    };
    let Some(object) = value.as_object() else {
        bad("entry", "Profile must be an object");
        return (None, issues);
    };
    const KEYS: &[&str] = &[
        "teamId",
        "expectedTeamName",
        "targetArenaId",
        "donorArenaId",
        "arenaName",
        "arenaFile",
        "floorFile",
    ];
    for key in object.keys() {
        if !KEYS.contains(&key.as_str()) {
            bad(
                key,
                "Unsupported legacy field; live addresses and activation flags are never imported",
            );
        }
    }
    for key in ["teamId", "targetArenaId", "donorArenaId"] {
        if value[key].as_u64().is_none_or(|n| n > 65535) {
            bad(key, "Expected an unsigned 16-bit integer");
        }
    }
    if value["targetArenaId"]
        .as_u64()
        .is_some_and(|n| !(996..=1045).contains(&n))
    {
        bad(
            "targetArenaId",
            "Only reserved arena IDs 996 through 1045 are supported",
        );
    }
    if value["donorArenaId"].as_u64().is_some() && value["donorArenaId"] == value["targetArenaId"] {
        bad("donorArenaId", "Donor and target must be different");
    }
    for (key, max) in [("expectedTeamName", 24), ("arenaName", 34)] {
        if value[key].as_str().is_none_or(|s| {
            s.trim().is_empty() || s.encode_utf16().count() > max || s.chars().any(char::is_control)
        }) {
            bad(
                key,
                "Expected a nonempty bounded name without control characters",
            );
        }
    }
    for (key, max) in [("arenaFile", 37), ("floorFile", 39)] {
        if value[key]
            .as_str()
            .is_none_or(|s| clone::stem(s, max).is_err())
        {
            bad(
                key,
                "Expected an asset filename stem, without path or extension",
            );
        }
    }
    if !issues.is_empty() {
        return (None, issues);
    }
    let profile = Profile {
        id: "0".repeat(48),
        team_id: value["teamId"].as_u64().unwrap() as u16,
        expected_team_name: value["expectedTeamName"].as_str().unwrap().into(),
        donor_arena_id: value["donorArenaId"].as_u64().unwrap() as u16,
        target_arena_id: value["targetArenaId"].as_u64().unwrap() as u16,
        arena_name: value["arenaName"].as_str().unwrap().into(),
        arena_file: value["arenaFile"].as_str().unwrap().into(),
        floor_file: value["floorFile"].as_str().unwrap().into(),
        enabled: false,
    };
    if let Err(e) = profile.validate() {
        issues.push(Issue {
            field: "entry".into(),
            message: format!("{e:#}"),
        });
        return (None, issues);
    }
    (Some(profile), issues)
}
pub(super) fn review(root: &Path) -> Result<Value> {
    let Some(source) = read_source(root)? else {
        return Ok(
            json!({"schema":REVIEW_SCHEMA,"legacyPresent":false,"reviewToken":null,"entries":[],"memoryWrites":0,"activation":false}),
        );
    };
    let rows=source.rows.iter().map(|r|{
        let profile=r.profile.as_ref().map(|p|json!({"teamId":p.team_id,"expectedTeamName":p.expected_team_name,"donorArenaId":p.donor_arena_id,"targetArenaId":p.target_arena_id,"arenaName":p.arena_name,"arenaFile":p.arena_file,"floorFile":p.floor_file,"enabled":false}));
        json!({"id":r.id,"index":r.index,"valid":r.profile.is_some(),"issues":r.issues,"profile":profile})
    }).collect::<Vec<_>>();
    Ok(
        json!({"schema":REVIEW_SCHEMA,"legacyPresent":true,"reviewToken":source.token,"entries":rows,"memoryWrites":0,"activation":false}),
    )
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    review_token: String,
    selected_ids: Vec<String>,
}
pub(super) fn import(root: &Path, input: &Value, store: &mut Store) -> Result<Value> {
    store.validate()?;
    let selected: Selection = serde_json::from_value(input.clone())?;
    ensure!(
        !selected.selected_ids.is_empty() && selected.selected_ids.len() <= 50,
        "Select at least one reviewed legacy profile"
    );
    let source = read_source(root)?.context("Legacy profiles disappeared. Review them again.")?;
    ensure!(
        selected.review_token == source.token,
        "Legacy profiles changed. Review them again before importing."
    );
    let mut ids = BTreeSet::new();
    let mut next = store.clone();
    let mut imported = Vec::new();
    for id in &selected.selected_ids {
        ensure!(ids.insert(id), "Duplicate selected legacy profile");
        let row = source
            .rows
            .iter()
            .find(|r| &r.id == id)
            .context("Selected legacy entry does not belong to this review")?;
        let mut profile = row
            .profile
            .clone()
            .context("Selected legacy entry is invalid or ambiguous")?;
        ensure!(
            !next
                .profiles
                .iter()
                .any(|p| p.target_arena_id == profile.target_arena_id
                    || (p.team_id == profile.team_id
                        && p.expected_team_name == profile.expected_team_name)),
            "A selected legacy target or team conflicts with an existing v2 profile; no entries were imported"
        );
        profile.id = backend::unique_id();
        profile.enabled = false;
        profile.validate()?;
        imported.push(json!({"sourceId":id,"id":profile.id,"targetArenaId":profile.target_arena_id,"enabled":false}));
        next.profiles.push(profile);
    }
    next.validate()?;
    *store = next;
    Ok(
        json!({"schema":REVIEW_SCHEMA,"imported":imported,"importedCount":imported.len(),"memoryWrites":0,"activation":false,"legacyUnchanged":true}),
    )
}
#[cfg(test)]
#[path = "custom_arena_profiles_legacy_tests.rs"]
mod tests;
