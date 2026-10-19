//! Saved logical arena choices. A profile never stores memory addresses or grants
//! native authority: each future launch must freshly admit the exact native row.
use crate::{backend, roster_v2_arena_clone as clone};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Read,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
};
const LIMIT: usize = 128 * 1024;
const SCHEMA: &str = "nba2k27-custom-arena-profiles-v2";
#[path = "custom_arena_profiles_legacy.rs"]
mod legacy;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Profile {
    pub id: String,
    pub team_id: u16,
    pub expected_team_name: String,
    pub donor_arena_id: u16,
    pub target_arena_id: u16,
    pub arena_name: String,
    pub arena_file: String,
    pub floor_file: String,
    pub enabled: bool,
}
impl Profile {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.id.len() == 48 && self.id.bytes().all(|b| b.is_ascii_hexdigit()),
            "Invalid custom arena profile identity"
        );
        // Reuse the exact native selector text and reserved-slot policy. Catalog and
        // team index here are placeholders only; a fresh native capture supplies both.
        clone::Edit::parse(
            &json!({"recordIndex":0,"teamId":self.team_id,"expectedTeamName":self.expected_team_name,"donorArenaId":self.donor_arena_id,"targetArenaId":self.target_arena_id,"arenaName":self.arena_name,"arenaFile":self.arena_file,"floorFile":self.floor_file,"catalogId":"0".repeat(64)}),
        )?;
        Ok(())
    }
    pub(crate) fn from_verified(edit: &clone::Edit) -> Self {
        Self {
            id: backend::unique_id(),
            team_id: edit.team_id,
            expected_team_name: edit.expected_team_name.clone(),
            donor_arena_id: edit.donor_arena_id,
            target_arena_id: edit.target_arena_id,
            arena_name: edit.arena_name.clone(),
            arena_file: edit.arena_file.clone(),
            floor_file: edit.floor_file.clone(),
            enabled: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Store {
    schema: String,
    profiles: Vec<Profile>,
}
impl Default for Store {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            profiles: Vec::new(),
        }
    }
}
impl Store {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA && self.profiles.len() <= 50,
            "Unsupported custom arena profile store; left untouched"
        );
        let (mut ids, mut targets, mut teams) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
        for p in &self.profiles {
            p.validate()?;
            ensure!(
                ids.insert(&p.id)
                    && targets.insert(p.target_arena_id)
                    && teams.insert((p.team_id, p.expected_team_name.clone())),
                "Ambiguous custom arena profiles; left untouched"
            );
        }
        Ok(())
    }
}
fn path(root: &Path) -> PathBuf {
    root.join("Data/custom-arena-profiles-v2.json")
}
fn read(root: &Path) -> Result<Store> {
    let p = path(root);
    if !p.try_exists()? {
        return Ok(Store::default());
    }
    let _dirs =
        crate::loader_files::Directories::pin(p.parent().context("Profile parent missing")?)?;
    let mut f = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&p)?;
    let m = f.metadata()?;
    ensure!(
        m.is_file() && m.file_attributes() & 0x400 == 0 && m.len() <= LIMIT as u64,
        "Custom arena profiles must be a bounded ordinary file"
    );
    let mut bytes = Vec::new();
    f.by_ref()
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= LIMIT,
        "Custom arena profile store exceeds limit"
    );
    let s: Store = serde_json::from_slice(&bytes)?;
    s.validate()?;
    Ok(s)
}
fn save(root: &Path, store: &Store) -> Result<()> {
    store.validate()?;
    ensure!(
        serde_json::to_vec_pretty(store)?.len() <= LIMIT,
        "Custom arena profile store exceeds limit"
    );
    backend::atomic_json(&path(root), store)
}
#[derive(Default)]
pub(crate) struct Service {
    gate: Mutex<()>,
    launch: Mutex<Option<FrozenLaunch>>,
}
impl Service {
    pub(crate) fn view(&self, root: &Path) -> Result<Value> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Custom arena profile service unavailable"))?;
        let store = read(root)?;
        Ok(
            json!({"schema":SCHEMA,"profiles":store.profiles,"legacyReviewAvailable":root.join("Data/player-research/custom-arena-profiles.json").try_exists()?,"memoryWrites":0}),
        )
    }
    /// Only call after the native helper's verified clone receipt, never for an
    /// arbitrary browser profile object. New profiles require explicit opt-in.
    pub(crate) fn record_verified(&self, root: &Path, edit: &clone::Edit) -> Result<Profile> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Custom arena profile service unavailable"))?;
        let mut s = read(root)?;
        let profile = Profile::from_verified(edit);
        profile.validate()?;
        s.profiles.retain(|p| {
            p.target_arena_id != profile.target_arena_id
                && (p.team_id != profile.team_id
                    || p.expected_team_name != profile.expected_team_name)
        });
        s.profiles.push(profile.clone());
        save(root, &s)?;
        Ok(profile)
    }
    pub(crate) fn enabled(&self, root: &Path) -> Result<Vec<Profile>> {
        let _gate = self
            .gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Custom arena profile service unavailable"))?;
        Ok(read(root)?
            .profiles
            .into_iter()
            .filter(|p| p.enabled)
            .collect())
    }
    pub(crate) fn request(
        &self,
        root: &Path,
        input: &Value,
        confirm: &dyn Fn(&str) -> bool,
    ) -> Result<Value> {
        self.request_with_available(root, input, confirm, &crate::owned_transaction::available)
    }
    fn request_with_available(
        &self,
        root: &Path,
        input: &Value,
        confirm: &dyn Fn(&str) -> bool,
        available: &dyn Fn() -> Result<()>,
    ) -> Result<Value> {
        if input["action"] == "read" {
            ensure!(
                input.as_object().is_some_and(|o| o.len() == 1),
                "Unexpected profile options"
            );
            return self.view(root);
        }
        let _gate = self
            .gate
            .lock()
            .map_err(|_| anyhow::anyhow!("Custom arena profile service unavailable"))?;
        let object = input
            .as_object()
            .context("Invalid custom arena profile request")?;
        let action = input["action"].as_str().context("Missing profile action")?;
        if action == "reviewLegacy" {
            ensure!(object.len() == 1, "Unexpected legacy review options");
            return legacy::review(root);
        }
        if action == "importLegacy" {
            ensure!(
                object.len() == 3
                    && object.keys().all(|key| matches!(
                        key.as_str(),
                        "action" | "reviewToken" | "selectedIds"
                    )),
                "Unexpected legacy import options"
            );
            let mut store = read(root)?;
            let result = legacy::import(
                root,
                &json!({"reviewToken":input["reviewToken"],"selectedIds":input["selectedIds"]}),
                &mut store,
            )?;
            save(root, &store)?;
            return Ok(result);
        }
        ensure!(
            matches!(action, "enable" | "disable" | "remove")
                && object.len() == 2
                && object.keys().all(|k| matches!(k.as_str(), "action" | "id")),
            "Unsupported custom arena profile action"
        );
        let id = input["id"].as_str().context("Missing profile identity")?;
        let mut s = read(root)?;
        let index = s
            .profiles
            .iter()
            .position(|p| p.id == id)
            .context("Custom arena profile changed. Refresh the list.")?;
        if action == "enable" {
            available()?;
            ensure!(
                confirm(
                    "Apply this saved custom arena on future Play sessions? The game is briefly paused while its roster is updated. If a partial update cannot be safely restored, the launcher may close that exact game to protect the roster. Existing games are not affected by changing this preference."
                ),
                "Custom arena startup application canceled"
            );
        }
        match action {
            "remove" => {
                s.profiles.remove(index);
            }
            "enable" => s.profiles[index].enabled = true,
            "disable" => s.profiles[index].enabled = false,
            _ => unreachable!(),
        };
        save(root, &s)?;
        Ok(json!({"schema":SCHEMA,"profiles":s.profiles,"memoryWrites":0}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        Profile {
            id: "a".repeat(48),
            team_id: 7,
            expected_team_name: "Fixture".into(),
            donor_arena_id: 17,
            target_arena_id: 999,
            arena_name: "Fixture Court".into(),
            arena_file: "arena_999_int".into(),
            floor_file: "arena_999_int_floor".into(),
            enabled: false,
        }
    }
    #[test]
    fn strict_logical_profiles_cannot_embed_live_addresses_or_escape_assets() {
        let p = profile();
        p.validate().unwrap();
        let mut value = serde_json::to_value(&p).unwrap();
        value["address"] = json!(0x140000000u64);
        assert!(serde_json::from_value::<Profile>(value).is_err());
        for stem in ["../arena", "C:\\arena", "arena.iff", ""] {
            let mut p = p.clone();
            p.arena_file = stem.into();
            assert!(p.validate().is_err());
        }
        for id in [995, 1046] {
            let mut p = p.clone();
            p.target_arena_id = id;
            assert!(p.validate().is_err());
        }
    }
    #[test]
    fn stores_refuse_ambiguous_ids_slots_or_teams() {
        let p = profile();
        let mut s = Store {
            schema: SCHEMA.into(),
            profiles: vec![p.clone()],
        };
        s.validate().unwrap();
        for mut other in [
            p.clone(),
            {
                let mut v = p.clone();
                v.id = "b".repeat(48);
                v.target_arena_id = 1000;
                v
            },
            {
                let mut v = p.clone();
                v.id = "b".repeat(48);
                v.team_id = 8;
                v
            },
        ] {
            s.profiles.push(other.clone());
            assert!(s.validate().is_err());
            s.profiles.pop();
            other.enabled = true;
        }
    }
    #[test]
    fn verified_profile_starts_disabled_and_contains_no_session_authority() {
        let p = profile();
        let value = serde_json::to_value(&p).unwrap();
        assert_eq!(value["enabled"], false);
        for key in ["address", "session", "game", "catalogId", "recordIndex"] {
            assert!(value.get(key).is_none());
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenLaunch {
    id: String,
    profiles: Vec<Profile>,
    game_root: PathBuf,
    mods_root: PathBuf,
    requested_ms: u64,
    attempted: BTreeSet<String>,
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
impl Service {
    pub(crate) fn freeze_launch(
        &self,
        root: &Path,
        game_root: &Path,
        mods_root: &Path,
    ) -> Result<FrozenLaunch> {
        self.freeze_launch_with_available(
            root,
            game_root,
            mods_root,
            &crate::owned_transaction::available,
        )
    }
    fn freeze_launch_with_available(
        &self,
        root: &Path,
        game_root: &Path,
        mods_root: &Path,
        available: &dyn Fn() -> Result<()>,
    ) -> Result<FrozenLaunch> {
        let profiles = self.enabled(root)?;
        ensure!(
            game_root.is_absolute() && mods_root.is_absolute(),
            "Saved arena launch folders must be absolute"
        );
        if !profiles.is_empty() {
            available()?;
        }
        Ok(FrozenLaunch {
            id: backend::unique_id(),
            profiles,
            game_root: game_root.into(),
            mods_root: mods_root.into(),
            requested_ms: now_ms(),
            attempted: BTreeSet::new(),
        })
    }
    pub(crate) fn disarm(&self) -> Result<()> {
        *self
            .launch
            .lock()
            .map_err(|_| anyhow::anyhow!("Saved arena launch state unavailable"))? = None;
        Ok(())
    }
    pub(crate) fn arm(&self, plan: FrozenLaunch) -> Result<()> {
        *self
            .launch
            .lock()
            .map_err(|_| anyhow::anyhow!("Saved arena launch state unavailable"))? = Some(plan);
        Ok(())
    }
    fn is_armed(&self, launch_id: &str) -> Result<bool> {
        Ok(self
            .launch
            .lock()
            .map_err(|_| anyhow::anyhow!("Saved arena launch state unavailable"))?
            .as_ref()
            .is_some_and(|p| p.id == launch_id))
    }
    fn claim(&self, launch_id: &str, key: String) -> Result<bool> {
        let mut state = self
            .launch
            .lock()
            .map_err(|_| anyhow::anyhow!("Saved arena launch state unavailable"))?;
        let Some(plan) = state.as_mut().filter(|p| p.id == launch_id) else {
            return Ok(false);
        };
        Ok(plan.attempted.insert(key))
    }
    pub(crate) fn start(backend: &std::sync::Arc<crate::backend::Backend>) -> Result<()> {
        let weak = std::sync::Arc::downgrade(backend);
        std::thread::Builder::new().name("custom-arena-startup".into()).spawn(move||loop{
   let Some(backend)=weak.upgrade()else{break};
   if backend.closing.load(std::sync::atomic::Ordering::Acquire){break;}
   let plan=backend.arena_profiles.launch.lock().ok().and_then(|v|v.clone());
   if let Some(plan)=plan.filter(|p|!p.profiles.is_empty()){
    if let Ok(binding)=backend.session.roster_v2_binding(&plan.game_root){
     if crate::roster_v2_access::profile_binding_is_newer(&binding,plan.requested_ms){
      for profile in &plan.profiles {
       if backend.closing.load(std::sync::atomic::Ordering::Acquire){break;}
       let result=(||->Result<Option<Value>>{
        let gate=crate::session_safety::write_gate()?;
        ensure!(!backend.closing.load(std::sync::atomic::Ordering::Acquire)&&backend.arena_profiles.is_armed(&plan.id)?,"Saved arena launch ended before admission");
        ensure!(!crate::roster_v2_write::any_pending(&backend.root)?,"Saved arena application held by pending native recovery");
        let(key,needs_write)=crate::roster_v2_access::profile_admission(&binding,profile,&plan.mods_root)?;
        ensure!(!backend.closing.load(std::sync::atomic::Ordering::Acquire),"Launcher is closing; saved arena effect refused");
        if !backend.arena_profiles.claim(&plan.id,key)?{return Ok(None);}
        if !needs_write{return Ok(Some(json!({"ok":true,"noChanges":true,"readBackVerified":true})));}
        // Claim precedes the first helper call. Any error or lost response is
        // never retried for this profile during this owned Play session.
        ensure!(!backend.closing.load(std::sync::atomic::Ordering::Acquire)&&backend.arena_profiles.is_armed(&plan.id)?,"Saved arena launch ended before dispatch");
        Ok(Some(crate::roster_v2_access::reapply_profile(&backend.root,&binding,profile,&plan.mods_root,&gate)?))
       })();
       match result {
        Ok(Some(report))=>{let _=backend::atomic_json(&backend.root.join("Logs/custom-arena-startup.json"),&json!({"profileId":profile.id,"teamId":profile.team_id,"report":report,"atMs":now_ms(),"initialApplicationOnly":true}));},
        Ok(None)=>{},
        Err(error)=>{let _=backend::atomic_json(&backend.root.join("Logs/custom-arena-startup.json"),&json!({"profileId":profile.id,"teamId":profile.team_id,"error":format!("{error:#}"),"atMs":now_ms(),"initialApplicationOnly":true}));},
       }
      }
     }
    }
   }
   drop(backend);std::thread::sleep(std::time::Duration::from_secs(2));
  })?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "custom_arena_profiles_launch_tests.rs"]
mod launch_tests;
#[cfg(test)]
#[path = "custom_arena_profiles_legacy_service_tests.rs"]
mod legacy_service_tests;
