use super::*;
use std::sync::atomic::AtomicUsize;
fn binding() -> Binding {
    Binding::from_launched(
        Identity {
            pid: 11,
            started: "100".into(),
            path: r"C:\Fixture\NBA2K27.exe".into(),
        },
        Identity {
            pid: 22,
            started: "200".into(),
            path: r"C:\Portable\2k Launcher.exe".into(),
        },
        PathBuf::from(r"C:\Fixture"),
        "a".repeat(48),
    )
    .unwrap()
}
fn row(name: &str, id: u32) -> Process {
    Process {
        name: name.into(),
        pid: id,
        parent: 0,
        identity: match id {
            11 => Some(binding().game),
            22 => Some(binding().parent),
            _ => None,
        },
    }
}
#[test]
fn only_exact_authenticated_parent_is_exempt_from_other_launcher_guard() {
    let b = binding();
    assert!(guard(&[row("NBA2K27.exe", 11), row("2k Launcher.exe", 22)], &b).is_ok());
    assert!(guard(&[row("NBA2K27.exe", 11)], &b).is_err());
    assert!(guard_with_current(&[row("NBA2K27.exe", 11)], &b, Some(&b.parent)).is_ok());
    let mut foreign = b.parent.clone();
    foreign.started = "201".into();
    assert!(guard_with_current(&[row("NBA2K27.exe", 11)], &b, Some(&foreign)).is_err());
    let mut parent = row("2k Launcher.exe", 22);
    parent.identity.as_mut().unwrap().started = "201".into();
    assert!(guard(&[row("NBA2K27.exe", 11), parent], &b).is_err());
    assert!(
        guard(
            &[
                row("NBA2K27.exe", 11),
                row("2k Launcher.exe", 22),
                row("2k Launcher.exe", 99)
            ],
            &b
        )
        .is_err()
    );
    assert!(
        guard(
            &[
                row("NBA2K27.exe", 11),
                row("2k Launcher.exe", 22),
                row("EasyAntiCheat_EOS.exe", 66)
            ],
            &b
        )
        .is_err()
    );
    let mut game = row("NBA2K27.exe", 11);
    game.identity.as_mut().unwrap().started = "101".into();
    assert!(guard(&[game, row("2k Launcher.exe", 22)], &b).is_err());
}
fn region(base: u64, len: u64) -> Region {
    Region {
        base,
        allocation: 0x200000000,
        len,
        state: MEM_COMMIT.0,
        kind: MEM_PRIVATE.0,
        protection: PAGE_READWRITE.0,
    }
}
#[test]
fn native_allocation_walk_is_bounded_to_one_committed_private_owner() {
    let base = 0x200000000;
    let mut calls = Vec::new();
    let owned = allocation(base + 0x680, |at| {
        calls.push(at);
        Ok(if at < base + 0x400000 {
            region(base, 0x400000)
        } else if at < base + 0x800000 {
            region(base + 0x400000, 0x400000)
        } else {
            Region {
                allocation: base + 0x800000,
                ..region(base + 0x800000, 0x400000)
            }
        })
    })
    .unwrap();
    assert_eq!(
        owned,
        profile::OwnedRegion {
            base,
            len: 0x800000
        }
    );
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|at| *at >= base && *at <= base + 0x800000));
    for case in 0..5 {
        let result = allocation(base + 0x680, |at| {
            Ok(match case {
                0 => Region {
                    protection: PAGE_EXECUTE_READWRITE.0,
                    ..region(base, 0x800000)
                },
                1 => Region {
                    state: MEM_RESERVE.0,
                    ..region(base, 0x800000)
                },
                2 => Region {
                    kind: MEM_IMAGE.0,
                    ..region(base, 0x800000)
                },
                3 => region(base, MAX_ALLOCATION + 4096),
                _ => {
                    if at == base + 0x800000 {
                        Region {
                            allocation: base + 0x800000,
                            ..region(at, 0x800000)
                        }
                    } else {
                        region(base, 0x1000)
                    }
                }
            })
        });
        assert!(result.is_err(), "case{case}");
    }
}
#[test]
fn native_access_has_no_write_or_process_control_rights() {
    assert_eq!(RIGHTS, PROCESS_QUERY_INFORMATION | PROCESS_VM_READ);
    assert_eq!(
        RIGHTS.0
            & (PROCESS_VM_WRITE.0
                | PROCESS_VM_OPERATION.0
                | PROCESS_CREATE_THREAD.0
                | PROCESS_TERMINATE.0
                | PROCESS_SUSPEND_RESUME.0),
        0
    );
    let stop = Arc::new(AtomicBool::new(true));
    assert!(Access::open(&binding(), stop).is_err());
}
fn player() -> Value {
    json!({"index":7,"name":"Test Player","firstName":"Test","lastName":"Player","metadata":{"GraphicId":1234,"UniqueId":900,"BirthYear":2000,"BirthMonth":2,"BirthDay":29,"YearsPro":5,"Position":0,"SecondaryPosition":1,"JerseyNumber":100},"ratingsRaw":vec![0u64;455]})
}
#[test]
fn browser_player_contract_uses_only_proven_fields_and_never_exposes_addresses() {
    let p = player_document(&player()).unwrap();
    assert_eq!(p["recordIndex"], 7);
    assert_eq!(p["source"], "live-roster");
    assert_eq!(p["cyberfaceId"], 1234);
    assert_eq!(p["position"], "PG");
    assert_eq!(p["secondaryPosition"], "SG");
    assert_eq!(p["jerseyNumber"], "00");
    assert_eq!(p["birthDate"], "2000-02-29");
    assert_eq!(p["yearsPro"], 5);
    assert_eq!(p["ratingsRaw"].as_array().unwrap().len(), 455);
    // Potential comes from the proven PotentialRaw bitfield (WPF display formula).
    assert_eq!(p["potentialRaw"], 0);
    assert_eq!(p["potential"], 25);
    assert_eq!(p["readOnly"], false);
    assert_eq!(p["writeAvailable"], true);
    for key in [
        "recordAddress",
        "teamAddress",
        "heightCm",
        "wingspanCm",
        "weightLbs",
        "audio",
        "overall",
        "teamIndex",
    ] {
        assert!(p[key].is_null(), "{key}");
    }
    assert!(birthday(1900, 2, 29).is_null());
    assert_eq!(birthday(2000, 2, 29), "2000-02-29");
    assert!(birthday(2000, 13, 1).is_null());
    let mut p = player();
    p["ratingsRaw"] = json!([1]);
    assert!(player_document(&p).is_err());
}
struct FakeEngine {
    calls: Arc<AtomicUsize>,
    details: Arc<AtomicUsize>,
    stale: Arc<AtomicBool>,
}
struct FakeCapture {
    document: Value,
    details: Arc<AtomicUsize>,
    stale: Arc<AtomicBool>,
}
impl Captured for FakeCapture {
    fn document(&self) -> &Value {
        &self.document
    }
    fn detail(&mut self, index: usize) -> Result<Value> {
        self.details.fetch_add(1, Ordering::SeqCst);
        ensure!(
            !self.stale.load(Ordering::Acquire) && index == 7,
            "Stale or invalid fake player"
        );
        player_document(&player())
    }
}
impl Engine for FakeEngine {
    fn capture(&self, _: &Binding, stop: Arc<AtomicBool>) -> Result<Box<dyn Captured>> {
        ensure!(!stop.load(Ordering::Acquire), "Cancelled");
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeCapture {
            document: json!({"status":"ready","readOnly":true,"writeAvailable":false,"writes":0,"players":[player_document(&player())?],"teams":[]}),
            details: self.details.clone(),
            stale: self.stale.clone(),
        }))
    }
}
fn service() -> (Service, Arc<AtomicUsize>, Arc<AtomicUsize>, Arc<AtomicBool>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let details = Arc::new(AtomicUsize::new(0));
    let stale = Arc::new(AtomicBool::new(false));
    let mut s = Service::new();
    s.engine = Arc::new(FakeEngine {
        calls: calls.clone(),
        details: details.clone(),
        stale: stale.clone(),
    });
    (s, calls, details, stale)
}
const TAB: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
#[test]
fn browser_bootstrap_status_and_auto_attach_share_one_retained_capture() {
    let (s, calls, details, _) = service();
    let b = binding();
    assert_eq!(s.request("bootstrap", TAB, &b).unwrap()["status"], "idle");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        s.request("rosterAutoAttach", TAB, &b).unwrap()["status"],
        "ready"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        s.request("playerResearchStatus", TAB, &b).unwrap()["writeAvailable"],
        false
    );
    s.request("rosterAutoAttach", OTHER, &b).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(s.player_detail(TAB, &b, 7).unwrap()["writeAvailable"], true);
    assert_eq!(details.load(Ordering::SeqCst), 1);
    s.request("scanPlayerResearch", OTHER, &b).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(s.request("status", TAB, &b).unwrap()["status"], "idle");
    assert!(s.player_detail(TAB, &b, 7).is_err());
}
#[test]
fn stale_rows_new_sessions_invalid_tabs_and_writes_never_adopt_or_mutate() {
    let (s, calls, _, stale) = service();
    let b = binding();
    s.request("rosterAutoAttach", TAB, &b).unwrap();
    stale.store(true, Ordering::Release);
    assert!(s.player_detail(TAB, &b, 7).is_err());
    assert!(s.request("savePlayer", TAB, &b).is_err());
    assert!(s.request("saveTeam", TAB, &b).is_err());
    assert!(s.reject_write().is_err());
    assert!(
        s.request("scanPlayerResearch", "../../address", &b)
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut replacement = b.clone();
    replacement.game.started = "101".into();
    assert_eq!(
        s.request("status", TAB, &replacement).unwrap()["status"],
        "idle"
    );
    assert!(s.player_detail(TAB, &replacement, 7).is_err());
}
#[test]
fn concurrent_capture_reports_scanning_and_cancel_never_waits_for_a_lock() {
    let (s, calls, _, _) = service();
    let b = binding();
    let lock = s.operation.lock().unwrap();
    assert_eq!(
        s.request("scanPlayerResearch", TAB, &b).unwrap()["status"],
        "scanning"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    s.cancel();
    assert!(s.request("bootstrap", TAB, &b).is_err());
    drop(lock);
}
struct SaveEngine {
    saves: Arc<AtomicUsize>,
    mode: Arc<AtomicUsize>,
}
struct SaveCapture {
    document: Value,
    saves: Arc<AtomicUsize>,
    mode: Arc<AtomicUsize>,
    stale: bool,
}
impl Captured for SaveCapture {
    fn document(&self) -> &Value {
        &self.document
    }
    fn detail(&mut self, _: usize) -> Result<Value> {
        anyhow::bail!("Unused")
    }
    fn save(&mut self, request: &Save<'_>) -> Result<Value> {
        self.saves.fetch_add(1, Ordering::SeqCst);
        ensure!(request.kind == Kind::Team, "Unexpected kind");
        match self.mode.load(Ordering::SeqCst) {
            0 => {
                self.document["teams"][0]["name"] = json!("Saved");
                Ok(json!({"writes":1,"readBackVerified":true}))
            }
            1 => anyhow::bail!("Synthetic failure; all attempted changes are verified rolled back"),
            _ => {
                self.stale = true;
                anyhow::bail!("Saved and verified, but the editor could not reload it")
            }
        }
    }
    fn usable(&self) -> bool {
        !self.stale
    }
}
impl Engine for SaveEngine {
    fn capture(&self, _: &Binding, _: Arc<AtomicBool>) -> Result<Box<dyn Captured>> {
        Ok(Box::new(SaveCapture {
            document: json!({"status":"ready","players":[],"teams":[{"source":"live-team","recordIndex":0,"name":"Team"}]}),
            saves: self.saves.clone(),
            mode: self.mode.clone(),
            stale: false,
        }))
    }
}
#[test]
fn saves_need_the_current_tab_adopt_verified_documents_and_expire_other_tabs() {
    let root = std::env::temp_dir().join(format!("roster-v2-save-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Data")).unwrap();
    let saves = Arc::new(AtomicUsize::new(0));
    let mode = Arc::new(AtomicUsize::new(0));
    let s = Service::with_engine(Arc::new(SaveEngine {
        saves: saves.clone(),
        mode: mode.clone(),
    }));
    let b = binding();
    let input = json!({});
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert!(s.save(Kind::Team, "../tab", &root, &input).is_err());
    s.request("rosterAutoAttach", TAB, &b).unwrap();
    s.request("rosterAutoAttach", OTHER, &b).unwrap();
    let saved = s.save(Kind::Team, TAB, &root, &input).unwrap();
    assert_eq!(saved["report"]["readBackVerified"], true);
    assert_eq!(saved["playerResearch"]["teams"][0]["name"], "Saved");
    assert_eq!(s.status(TAB).unwrap()["teams"][0]["name"], "Saved");
    // Only the saving tab advances; the other must refresh before it saves.
    assert_eq!(s.status(OTHER).unwrap()["status"], "idle");
    assert!(s.save(Kind::Team, OTHER, &root, &input).is_err());
    assert_eq!(saves.load(Ordering::SeqCst), 1);
    mode.store(1, Ordering::SeqCst);
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert_eq!(s.status(TAB).unwrap()["teams"][0]["name"], "Saved");
    mode.store(2, Ordering::SeqCst);
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert_eq!(s.status(TAB).unwrap()["status"], "idle");
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert_eq!(saves.load(Ordering::SeqCst), 3);
    // Any unreadable or pending native roster journal blocks before the lease.
    mode.store(0, Ordering::SeqCst);
    s.request("scanPlayerResearch", TAB, &b).unwrap();
    std::fs::write(writer::path(&root), b"{broken").unwrap();
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert_eq!(saves.load(Ordering::SeqCst), 3);
    std::fs::remove_file(writer::path(&root)).unwrap();
    assert!(s.save(Kind::Team, TAB, &root, &input).is_ok());
    s.cancel();
    assert!(s.save(Kind::Team, TAB, &root, &input).is_err());
    assert_eq!(saves.load(Ordering::SeqCst), 4);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn write_lease_rights_and_destinations_stay_bounded() {
    assert_eq!(
        WRITE_RIGHTS.0 & (PROCESS_CREATE_THREAD.0 | PROCESS_TERMINATE.0 | PROCESS_SUSPEND_RESUME.0),
        0
    );
    let base = 0x2_0000_0000;
    assert!(writable_region(region(base, 0x800000), 0x2_0000_0000));
    for bad in [
        Region {
            protection: PAGE_EXECUTE_READWRITE.0,
            ..region(base, 0x800000)
        },
        Region {
            protection: PAGE_WRITECOPY.0,
            ..region(base, 0x800000)
        },
        Region {
            protection: PAGE_READONLY.0,
            ..region(base, 0x800000)
        },
        Region {
            kind: MEM_IMAGE.0,
            ..region(base, 0x800000)
        },
        Region {
            state: MEM_RESERVE.0,
            ..region(base, 0x800000)
        },
    ] {
        assert!(!writable_region(bad, 0x2_0000_0000));
    }
    assert!(!writable_region(region(base, 0x800000), 0x3_0000_0000));
}
#[test]
fn editor_services_need_a_current_v2_tab_and_saves_respect_journals() {
    let root = std::env::temp_dir().join(format!("roster-v2-editor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("Data")).unwrap();
    let (s, _, _, _) = service();
    let b = binding();
    let yes = |_: &str| true;
    assert!(
        s.editor("teamStaff", TAB, &root, &json!({}), &yes).is_err(),
        "no capture yet"
    );
    s.request("rosterAutoAttach", TAB, &b).unwrap();
    // The fake capture has no editor services: honest refusal, capture kept.
    let e = s
        .editor("teamStaff", TAB, &root, &json!({}), &yes)
        .unwrap_err();
    assert!(e.to_string().contains("no editor services"));
    assert_eq!(s.status(TAB).unwrap()["status"], "ready");
    std::fs::write(writer::path(&root), b"{broken").unwrap();
    assert!(
        s.editor("saveTeamStaff", TAB, &root, &json!({}), &yes)
            .is_err()
    );
    let blocked = s
        .editor("playerSeasons", TAB, &root, &json!({"action":"save"}), &yes)
        .unwrap_err();
    assert!(
        !blocked.to_string().contains("no editor services"),
        "journal checked first"
    );
    // Reads never consult the journal.
    let read = s
        .editor("playerSeasons", TAB, &root, &json!({"action":"read"}), &yes)
        .unwrap_err();
    assert!(read.to_string().contains("no editor services"));
    assert!(
        s.editor("teamStaff", "bad", &root, &json!({}), &yes)
            .is_err()
    );
    std::fs::remove_dir_all(root).unwrap();
}

fn dna_player() -> Value {
    let mut value = player();
    let ratings: Vec<Value> = crate::player_dna::schema()["fields"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let maximum = field["maximum"].as_i64().unwrap();
            json!(if maximum < 0 {
                0
            } else {
                (index as u64 * 17 + 3) % (maximum as u64 + 1)
            })
        })
        .collect();
    value["ratingsRaw"] = json!(ratings);
    value
}
struct DnaEngine(Arc<AtomicUsize>);
struct DnaCapture(Value);
impl Captured for DnaCapture {
    fn document(&self) -> &Value {
        &self.0
    }
    fn detail(&mut self, _: usize) -> Result<Value> {
        panic!("DNA must not read details")
    }
    fn save(&mut self, _: &Save<'_>) -> Result<Value> {
        panic!("DNA must not save")
    }
    fn editor(&mut self, _: &Editor<'_>) -> Result<Value> {
        panic!("DNA must not invoke editor")
    }
}
impl Engine for DnaEngine {
    fn capture(&self, _: &Binding, _: Arc<AtomicBool>) -> Result<Box<dyn Captured>> {
        assert_eq!(
            self.0.fetch_add(1, Ordering::SeqCst),
            0,
            "DNA must not recapture"
        );
        Ok(Box::new(DnaCapture(
            json!({"status":"ready", "players":[player_document(&dna_player())?]}),
        )))
    }
}
#[test]
fn dna_copy_uses_only_leased_snapshot_preserves_transferable_raw_fields_and_omits_unproven_values()
{
    let calls = Arc::new(AtomicUsize::new(0));
    let s = Service::with_engine(Arc::new(DnaEngine(calls.clone())));
    s.request("rosterAutoAttach", TAB, &binding()).unwrap();
    let before = {
        let state = s.state.lock().unwrap();
        (state.view.clone(), state.revision, state.tabs.clone())
    };
    // Even the engine-operation lock is irrelevant: copy never accesses it.
    let _busy_engine = s.operation.lock().unwrap();
    let rows = s.player_dna(TAB, &json!({"action":"players"})).unwrap();
    assert_eq!(rows[0]["recordIndex"], 7);
    assert_eq!(rows[0]["name"], "Test Player");
    let sections: Vec<_> = crate::player_dna::schema()["groups"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|g| **g != "seasons")
        .cloned()
        .collect();
    let request =
        json!({"action":"copy", "recordIndex":7, "key":rows[0]["key"], "sections":sections});
    let dna = s.player_dna(TAB, &request).unwrap();
    let raw = before.0["players"][0]["ratingsRaw"].as_array().unwrap();
    assert_eq!(raw.len(), 455);
    let schema = crate::player_dna::schema()["fields"].as_array().unwrap();
    let expected: serde_json::Map<String, Value> = schema
        .iter()
        .zip(raw)
        .filter(|(field, _)| field["maximum"].as_i64().unwrap() >= 0)
        .map(|(field, value)| (field["key"].as_str().unwrap().to_owned(), value.clone()))
        .collect();
    assert_eq!(
        expected.len(),
        441,
        "14 unsupported raw fields stay excluded"
    );
    assert_eq!(
        dna["fields"].as_object().unwrap(),
        &expected,
        "each allowed schema key must preserve its captured raw-index value"
    );
    for field in schema
        .iter()
        .filter(|field| field["maximum"].as_i64().unwrap() < 0)
    {
        assert!(dna["fields"].get(field["key"].as_str().unwrap()).is_none());
    }
    for key in ["heightCm", "wingspanCm", "weightLbs", "fullNameId"] {
        assert!(dna["info"].get(key).is_none(), "{key}");
    }
    for key in [
        "recordIndex",
        "recordAddress",
        "teamAddress",
        "processId",
        "source",
    ] {
        assert!(dna.get(key).is_none(), "{key}");
    }
    assert_eq!(dna["info"]["cyberfaceId"], 1234);
    assert_eq!(dna["info"]["birthDate"], "2000-02-29");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let state = s.state.lock().unwrap();
    assert_eq!(
        (state.view.clone(), state.revision, state.tabs.clone()),
        before
    );
    drop(state);
    for (key, value) in [
        ("key", json!("changed")),
        ("recordIndex", json!(7.5)),
        ("sections", json!(["seasons"])),
        ("sections", json!([])),
        ("sections", json!(["unknown"])),
    ] {
        let mut bad = request.clone();
        bad[key] = value;
        assert!(s.player_dna(TAB, &bad).is_err());
    }
    assert!(s.player_dna("invalid", &request).is_err());
    assert!(s.player_dna(OTHER, &request).is_err());
    {
        let mut state = s.state.lock().unwrap();
        state.view["players"][0]["lastName"] = json!("Replacement");
    }
    assert!(
        s.player_dna(TAB, &request).is_err(),
        "changed player key must refuse"
    );
}
#[test]
fn dna_copy_refuses_expired_rebound_cancelled_saving_and_scanning_views() {
    let (s, calls, details, _) = service();
    let b = binding();
    s.request("rosterAutoAttach", TAB, &b).unwrap();
    let request = json!({"action":"players"});
    {
        let mut state = s.state.lock().unwrap();
        state.saving = true;
    }
    assert!(s.player_dna(TAB, &request).is_err());
    {
        let mut state = s.state.lock().unwrap();
        state.saving = false;
        state.view["status"] = json!("scanning");
    }
    assert!(s.player_dna(TAB, &request).is_err());
    {
        s.state.lock().unwrap().view["status"] = json!("ready");
    }
    s.request("scanPlayerResearch", OTHER, &b).unwrap();
    assert!(
        s.player_dna(TAB, &request).is_err(),
        "expired lease cannot copy"
    );
    assert!(s.player_dna(OTHER, &request).is_ok());
    let mut replacement = b.clone();
    replacement.game.started = "101".into();
    s.request("status", OTHER, &replacement).unwrap();
    assert!(
        s.player_dna(OTHER, &request).is_err(),
        "rebound capture is gone"
    );
    s.request("rosterAutoAttach", TAB, &replacement).unwrap();
    s.cancel();
    assert!(s.player_dna(TAB, &request).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(details.load(Ordering::SeqCst), 0);
}

#[test]
fn season_import_review_is_readonly_and_unknown_structure_actions_stay_guarded() {
    for action in ["read", "teams", "review-existing-import", "review-import"] {
        assert!(!editor_writes("playerSeasons", &json!({"action":action})));
    }
    for action in [
        "save",
        "apply-import",
        "import-seasons",
        "copy-shared",
        "add-older",
        "remove-oldest",
        "unknown",
    ] {
        assert!(editor_writes("playerSeasons", &json!({"action":action})));
    }
    assert!(editor_writes("playerSeasons", &json!({})));
    assert!(!editor_writes("teamRotations", &json!({"action":"read"})));
    assert!(editor_writes("teamRotations", &json!({"action":"teams"})));
    assert!(editor_writes("saveTeamStaff", &json!({})));
}

#[test]
fn import_link_guard_tracks_every_player_slot_and_private_history_but_not_other_fields() {
    let stride = Kind::Player.stride();
    let mut rows = vec![0; stride * 2];
    rows[services::PLAYER_SEASONS..services::PLAYER_SEASONS + 2]
        .copy_from_slice(&4i16.to_le_bytes());
    let before = season_link_projection(&rows).unwrap();
    assert_eq!(before.len(), 148);
    let mut unrelated = rows.clone();
    unrelated[stride] = 1; // Another player's name is outside season ownership.
    assert_eq!(season_link_projection(&unrelated).unwrap(), before);
    for offset in [
        stride + services::PLAYER_SEASONS + 64,
        stride + services::PLAYER_PRIVATE_SEASONS,
    ] {
        let mut changed = rows.clone();
        changed[offset] ^= 1;
        assert_ne!(season_link_projection(&changed).unwrap(), before);
    }
    assert!(season_link_projection(&[]).is_err());
    assert!(season_link_projection(&rows[..rows.len() - 1]).is_err());
}

#[test]
fn ordinary_season_reads_cannot_replace_an_import_token_with_weaker_ownership() {
    let row = vec![0; Kind::Player.stride()];
    let raw = vec![0; services::SEASON_STRIDE];
    let links = vec![0; 148];
    let ordinary = season_token("exact-owned-source", &row, &raw, None);
    let imported = season_token("exact-owned-source", &row, &raw, Some(&links));
    assert_ne!(ordinary, imported);
    let mut changed = links.clone();
    changed[147] = 1;
    assert_ne!(
        imported,
        season_token("exact-owned-source", &row, &raw, Some(&changed))
    );
    let import_cache = std::collections::HashMap::from([(imported.clone(), true)]);
    let ordinary_cache = std::collections::HashMap::from([(ordinary, false)]);
    assert!(import_cache.contains_key(&imported));
    assert!(!ordinary_cache.contains_key(&imported));
}

#[test]
fn full_selected_record_witness_rejects_all_other_changes() {
    let before = vec![0; Kind::Team.stride()];
    for at in [
        0,
        0x2fa,
        writer::TEAM_ID,
        services::TEAM_STAFF_SLOTS,
        Kind::Team.stride() - 1,
    ] {
        let mut now = before.clone();
        now[at] = 1;
        assert!(!retained_matches(&before, &now, None));
    }
    let slot = writer::UNIFORM_SLOTS + 8;
    let mut now = before.clone();
    now[slot..slot + 8].copy_from_slice(&0x123400u64.to_le_bytes());
    assert!(retained_matches(&before, &now, Some(slot)));
    now[slot - 1] = 1;
    assert!(!retained_matches(&before, &now, Some(slot)));
    assert!(!retained_matches(&before, &before, Some(before.len())));
}
#[test]
fn uniform_reference_guard_accepts_only_exact_owned_before_after_and_rejects_aliases() {
    let stride = Kind::Team.stride();
    let candidate = 0x123400u64;
    let mut rows = vec![0; stride * 2];
    let slot = writer::UNIFORM_SLOTS + 3 * 8;
    assert!(uniform_references_valid(&rows, 0, 3, candidate).unwrap());
    rows[slot..slot + 8].copy_from_slice(&candidate.to_le_bytes());
    assert!(uniform_references_valid(&rows, 0, 3, candidate).unwrap());
    rows[stride + slot..stride + slot + 8].copy_from_slice(&candidate.to_le_bytes());
    assert!(!uniform_references_valid(&rows, 0, 3, candidate).unwrap());
    rows[stride + slot..stride + slot + 8].fill(0);
    rows[slot..slot + 8].copy_from_slice(&0x123408u64.to_le_bytes());
    assert!(!uniform_references_valid(&rows, 0, 3, candidate).unwrap());
    assert!(uniform_references_valid(&rows, 2, 3, candidate).is_err());
}
#[test]
fn prepared_reviews_are_bounded_atomic_and_do_not_evict_discarded_drafts() {
    use std::collections::HashMap;
    let mut ordinary: HashMap<String, i32> = HashMap::from([("read".into(), 1)]);
    let mut prepared = HashMap::from([("prior-import".into(), 2)]);
    adopt_prepared(
        &mut prepared,
        HashMap::from([("abandoned-import".into(), 3)]),
    )
    .unwrap();
    ordinary.insert("new-read".into(), 4);
    assert_eq!(prepared["prior-import"], 2);
    assert_eq!(prepared["abandoned-import"], 3);
    let before = prepared.clone();
    let overflow = (0..50).map(|i| (format!("review-{i}"), i)).collect();
    assert!(adopt_prepared(&mut prepared, overflow).is_err());
    assert_eq!(prepared, before);
    adopt_prepared(&mut prepared, HashMap::from([("prior-import".into(), 2)])).unwrap();
    assert_eq!(prepared, before);
}

#[test]
fn stable_record_reader_rejects_torn_short_and_cancelled_rows() {
    use std::cell::Cell;
    struct M {
        reads: Cell<usize>,
        short: bool,
        cancel: bool,
    }
    impl Memory for M {
        fn base(&self) -> u64 {
            0
        }
        fn checkpoint(&self) -> Result<()> {
            anyhow::ensure!(!self.cancel, "cancelled");
            Ok(())
        }
        fn read(&self, _: u64, n: usize) -> Result<Vec<u8>> {
            let c = self.reads.get();
            self.reads.set(c + 1);
            Ok(vec![c as u8; if self.short { n - 1 } else { n }])
        }
    }
    let torn = M {
        reads: Cell::new(0),
        short: false,
        cancel: false,
    };
    assert!(read_record(&torn, 0x10000, 64).is_err());
    assert_eq!(torn.reads.get(), 2);
    let short = M {
        reads: Cell::new(0),
        short: true,
        cancel: false,
    };
    assert!(read_record(&short, 0x10000, 64).is_err());
    assert_eq!(short.reads.get(), 1);
    let cancel = M {
        reads: Cell::new(0),
        short: false,
        cancel: true,
    };
    assert!(read_record(&cancel, 0x10000, 64).is_err());
    assert_eq!(cancel.reads.get(), 0);
}
#[test]
fn season_scope_catches_selected_row_drift_link_swap_and_repeated_slot() {
    let stride = Kind::Player.stride();
    let mut rows = vec![0; stride * 2];
    for row in rows.chunks_exact_mut(stride) {
        row[services::PLAYER_SEASONS..services::PLAYER_SEASONS + 66].fill(255);
    }
    let slot = services::PLAYER_SEASONS;
    rows[slot..slot + 2].copy_from_slice(&7i16.to_le_bytes());
    let projection = season_link_projection(&rows).unwrap();
    let selected = rows[..stride].to_vec();
    assert!(season_witness_matches(&rows, &projection, 0, &selected, Some(7)).is_ok());
    let mut drift = rows.clone();
    drift[44] ^= 1;
    assert!(season_witness_matches(&drift, &projection, 0, &selected, None).is_err());
    let mut swap = rows.clone();
    swap[slot..slot + 2].fill(255);
    swap[stride + slot..stride + slot + 2].copy_from_slice(&7i16.to_le_bytes());
    assert!(season_witness_matches(&swap, &projection, 0, &selected, None).is_err());
    rows[slot + 50..slot + 52].copy_from_slice(&7i16.to_le_bytes());
    let p = season_link_projection(&rows).unwrap();
    assert!(season_witness_matches(&rows, &p, 0, &rows[..stride], Some(7)).is_err());
    assert!(season_witness_matches(&rows, &p, 0, &rows[..stride], None).is_ok()); // ordinary shared policy unchanged
}
#[test]
fn failed_native_import_review_does_not_publish_credentials() {
    let mut cache = std::collections::HashMap::from([("visible".into(), 1)]);
    let candidate = std::collections::HashMap::from([("not-adopted".into(), 2)]);
    assert!(
        adopt_successful_review(
            &mut cache,
            candidate,
            Err(anyhow::anyhow!("planner refusal"))
        )
        .is_err()
    );
    assert_eq!(cache.len(), 1);
    assert_eq!(cache["visible"], 1);
    adopt_successful_review(
        &mut cache,
        std::collections::HashMap::from([("discarded-client-closure".into(), 3)]),
        Ok(json!({"rows":[]})),
    )
    .unwrap();
    assert_eq!(cache["visible"], 1);
    assert_eq!(cache["discarded-client-closure"], 3);
}
#[test]
fn only_full_name_audio_scalar_grants_a_browser_write_capability() {
    let mut source = player();
    source["fullNameId"] = json!(42);
    let out = player_document(&source).unwrap();
    assert_eq!(out["audio"]["fullNameId"], 42);
    assert_eq!(out["editCapabilities"]["fullNameId"], true);
    for field in ["heightCm", "wingspanCm", "fromName"] {
        assert_eq!(out["editCapabilities"][field], false);
    }
}

#[test]
fn unknown_negative_full_name_audio_is_visible_but_readonly() {
    let mut source = player();
    source["fullNameId"] = json!(-9);
    let out = player_document(&source).unwrap();
    assert_eq!(out["audio"]["fullNameId"], -9);
    assert_eq!(out["editCapabilities"]["fullNameId"], false);
}

#[test]
fn origin_browser_projection_keeps_legacy_neutral_and_only_known_members_editable() {
    for state in ["unassigned", "member", "invalid", "unavailable"] {
        let mut source = player();
        source["nativeOriginCatalogId"] = json!("a".repeat(64));
        source["originChoicesAvailable"] = json!(true);
        source["originState"] = json!(state);
        source["nativeOriginIndex"] = if state == "member" {
            json!(514)
        } else {
            Value::Null
        };
        source["collegeName"] = json!("Existing origin");
        let out = player_document(&source).unwrap();
        assert!(out["fromIndex"].is_null());
        assert_eq!(out["nativeOriginIndex"], source["nativeOriginIndex"]);
        assert_eq!(
            out["editCapabilities"]["fromName"],
            matches!(state, "unassigned" | "member")
        );
        assert_eq!(out["collegeName"], "Existing origin");
    }
}

#[test]
fn origin_capability_refuses_zero_choices_and_unknown_current_text() {
    let mut source = player();
    source["nativeOriginCatalogId"] = json!("a".repeat(64));
    source["nativeOriginIndex"] = json!(0);
    source["originState"] = json!("member");
    source["originChoicesAvailable"] = json!(true);
    assert_eq!(
        player_document(&source).unwrap()["editCapabilities"]["fromName"],
        false
    );
    source["collegeName"] = json!("Known current");
    assert_eq!(
        player_document(&source).unwrap()["editCapabilities"]["fromName"],
        true
    );
    source["originChoicesAvailable"] = json!(false);
    assert_eq!(
        player_document(&source).unwrap()["editCapabilities"]["fromName"],
        false
    );
    source["originState"] = json!("unassigned");
    source["nativeOriginIndex"] = Value::Null;
    assert_eq!(
        player_document(&source).unwrap()["editCapabilities"]["fromName"],
        false
    );
    source["originChoicesAvailable"] = json!(true);
    assert_eq!(
        player_document(&source).unwrap()["editCapabilities"]["fromName"],
        true
    );
}
