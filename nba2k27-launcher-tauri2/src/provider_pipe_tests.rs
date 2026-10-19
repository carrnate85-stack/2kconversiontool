//! Only private files/pipes and explicitly spawned fixture children. No game access.
use super::*;
use crate::{
    backend,
    provider_access::{Acl, Process, Secret, verify_build},
    session_safety,
};
use std::{
    fs,
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Command, Stdio},
};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

struct Fixture {
    root: PathBuf,
    server: Option<Server>,
    data: Vec<u8>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_arena(false)
    }
    fn with_arena(active: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("rust-provider-pipe-{}", backend::unique_id()));
        fs::create_dir(&root).unwrap();
        let data = (0..wire::MAX_READ as usize + 17)
            .map(|i| (i % 251) as u8)
            .collect::<Vec<_>>();
        fs::write(root.join("test.iff"), &data).unwrap();
        let service = Arc::new(
            Service::open(
                &root,
                &[],
                &["alias.iff".into()],
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap(),
        );
        let gate = Arc::new(Gate::new(&own(), &root.join("NBA2K27.exe")).unwrap());
        let controls = if active {
            Some(Arc::new(
                crate::provider_launch::Control::fixture_active_arena(
                    crate::provider_launch::ContextData {
                        owner: crate::provider_launch::Owner {
                            portable: root.clone(),
                            session: backend::unique_id(),
                        },
                        parent: own(),
                        game_exe: root.join("NBA2K27.exe"),
                        mods_root: root.clone(),
                        features: Default::default(),
                    },
                )
                .unwrap(),
            ))
        } else {
            None
        };
        let server = Server::start_with(service, gate, controls).unwrap();
        Self {
            root,
            server: Some(server),
            data,
        }
    }
    fn server(&self) -> &Server {
        self.server.as_ref().unwrap()
    }
    fn bound(&self) {
        self.server().gate.bind_fixture(&own()).unwrap();
    }
    fn control(&self, operation: &str) -> Value {
        json!({"operation":operation,"sessionToken":self.server().gate.secret.text()})
    }
    fn json(&self, request: Value) -> Value {
        serde_json::from_slice(
            &exchange(
                &self.server().endpoint,
                &serde_json::to_vec(&request).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(mut server) = self.server.take() {
            let _ = server.shutdown_fixture();
            drop(server);
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn own() -> Identity {
    session_safety::identity(std::process::id()).unwrap()
}
#[test]
fn shutdown_releases_cached_mod_handles_before_cleanup_is_reported_complete() {
    use std::os::windows::fs::OpenOptionsExt;
    let mut fixture = Fixture::new();
    assert_eq!(
        fixture
            .server()
            .assets
            .read("test.iff", 0, 16)
            .unwrap()
            .1
            .len(),
        16
    );
    assert!(fixture.server().assets.release_closed_handles().is_err());
    fixture.server.as_mut().unwrap().shutdown_fixture().unwrap();
    let path = fixture.root.join("test.iff");
    let file = fs::OpenOptions::new()
        .read(true)
        .access_mode(0x80000000 | 0x10000)
        .share_mode(1)
        .open(&path)
        .unwrap();
    crate::loader_files::unlink(&file).unwrap();
    drop(file);
    // Server and its Arc<Service> are deliberately still alive here.
    assert!(!path.exists());
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
}
async fn connect(endpoint: &str) -> Result<NamedPipeClient> {
    let start = Instant::now();
    loop {
        match ClientOptions::new().open(endpoint) {
            Ok(pipe) => return Ok(pipe),
            Err(error)
                if error.raw_os_error() == Some(231)
                    && start.elapsed() < Duration::from_secs(3) =>
            {
                tokio::time::sleep(Duration::from_millis(5)).await
            }
            Err(error) => return Err(error.into()),
        }
    }
}
fn exchange(endpoint: &str, body: &[u8]) -> Result<Vec<u8>> {
    runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(4), async {
            let mut pipe = connect(endpoint).await?;
            pipe.write_all(&wire::encode_frame(body)?).await?;
            let mut h = [0; 4];
            pipe.read_exact(&mut h).await?;
            let size = u32::from_le_bytes(h) as usize;
            ensure!(
                (1..=wire::MAX_FRAME).contains(&size),
                "Bad fixture response"
            );
            let mut b = vec![0; size];
            pipe.read_exact(&mut b).await?;
            Ok(b)
        })
        .await?
    })
}
fn request(op: u16, offset: u64, length: u32, path: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(0x31504232u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(op.to_le_bytes());
    b.extend(offset.to_le_bytes());
    b.extend(length.to_le_bytes());
    b.extend((path.len() as u32).to_le_bytes());
    b.extend(path.as_bytes());
    b
}

#[test]
fn private_pipe_authenticates_parent_token_and_denies_unbound_game() {
    let f = Fixture::new();
    let endpoint = &f.server().endpoint;
    assert!(endpoint.starts_with(wire::PIPE_PREFIX));
    assert_eq!(endpoint.rsplit('-').next().unwrap().len(), 64);
    assert_eq!(
        exchange(endpoint, &request(1, 0, 0, "test.iff")).unwrap(),
        wire::encode_error(1)
    );
    let status = f.json(f.control("status"));
    assert_eq!(status["ok"], true);
    assert_eq!(status["response"]["indexedAssets"], 1);
    assert_eq!(status["response"]["security"]["currentUserOnly"], true);
    assert_eq!(
        f.json(json!({"operation":"status","sessionToken":"00".repeat(32)}))["ok"],
        false
    );
    assert_eq!(f.json(f.control("not-an-operation"))["ok"], false);
    let mut query = f.control("query");
    query["logicalPath"] = json!("test.iff");
    assert_eq!(f.json(query)["response"]["size"], f.data.len());
    assert_eq!(f.json(json!({"operation":"status"}))["ok"], false);
    let other = Fixture::new();
    assert_ne!(endpoint, &other.server().endpoint);
}
#[test]
fn arena_pipe_controls_are_authenticated_strict_and_refused_without_launch_ownership() {
    let f = Fixture::new();
    let request = json!({"operation":"arena-select","sessionToken":f.server().gate.secret.text(),"assetId":"arena_000_int","acknowledgement":"apply-one-arena-for-current-game"});
    assert_eq!(f.json(request.clone())["ok"], false);
    let mut bad = request.clone();
    bad["sessionToken"] = json!("00".repeat(32));
    assert_eq!(f.json(bad)["ok"], false);
    let mut bad = request.clone();
    bad["address"] = json!(12345);
    assert_eq!(f.json(bad)["ok"], false);
    let mut bad = request;
    bad["acknowledgement"] = json!("apply-all");
    assert_eq!(f.json(bad)["ok"], false);
    assert_eq!(f.json(f.control("status"))["ok"], true);
}
#[test]
fn arena_pipe_queues_exact_ids_only_after_retained_game_binding_without_memory_access() {
    let f = Fixture::with_arena(true);
    let mut r = f.control("arena-select");
    r["assetId"] = json!("arena_000_int");
    r["acknowledgement"] = json!("apply-one-arena-for-current-game");
    assert_eq!(f.json(r.clone())["ok"], false);
    f.bound();
    let queued = f.json(r.clone());
    assert_eq!(queued["ok"], true);
    assert_eq!(queued["response"]["phase"], "queued");
    assert_eq!(queued["response"]["activeId"], "default");
    assert_eq!(queued["response"]["revision"], 1);
    for id in [
        "../arena_000_int",
        "arena_reccenter_lobbyint",
        "1976–77 76ers / The Spectrum",
    ] {
        let mut bad = r.clone();
        bad["assetId"] = json!(id);
        assert_eq!(f.json(bad)["ok"], false);
    }
    let mut bad = r.clone();
    bad["address"] = json!(1234);
    assert_eq!(f.json(bad)["ok"], false);
    let mut bad = r.clone();
    bad["acknowledgement"] = json!("apply-all");
    assert_eq!(f.json(bad)["ok"], false);
    r["assetId"] = json!("default");
    assert_eq!(f.json(r)["response"]["revision"], 2);
    assert!(!f.root.join("Data/blacktop-arena-session.json").exists());
}

#[test]
fn outfit_pipe_is_game_bound_strict_acknowledged_and_only_queues_without_native_memory_access() {
    let f = Fixture::with_arena(true);
    let mut r = f.control("outfit-command");
    r["acknowledgement"] = json!("read-blacktop-outfits");
    r["command"] = json!({"action":"list"});
    assert_eq!(f.json(r.clone())["ok"], false);
    f.bound();
    for key in ["timestamp", "description", "slot"] {
        let mut bad = r.clone();
        bad["command"][key] = json!(12345);
        assert_eq!(f.json(bad)["ok"], false);
    }
    let mut bad = r.clone();
    bad["acknowledgement"] = json!("apply-one-outfit-for-current-game");
    assert_eq!(f.json(bad)["ok"], false);
    let mut bad = r.clone();
    bad["sessionToken"] = json!("wrong");
    assert_eq!(f.json(bad)["ok"], false);
    let queued = f.json(r.clone());
    assert_eq!(queued["ok"], true);
    assert_eq!(queued["response"]["phase"], "queued");
    assert!(queued["response"]["selection"].is_null());
    assert_eq!(f.json(r)["response"]["revision"], 1);
    assert!(
        !f.root
            .join("Data")
            .join(crate::outfit_journal::MARKER)
            .exists()
    );
    let f = Fixture::with_arena(true);
    f.bound();
    let mut r = f.control("outfit-command");
    r["command"] = json!({"action":"restore","playerBinding":"a".repeat(48)});
    r["acknowledgement"] = json!("read-blacktop-outfits");
    assert_eq!(f.json(r.clone())["ok"], false);
    r["acknowledgement"] = json!("apply-one-outfit-for-current-game");
    assert_eq!(f.json(r)["response"]["phase"], "queued");
    assert!(
        !f.root
            .join("Data")
            .join(crate::outfit_journal::MARKER)
            .exists()
    );
}
#[test]
fn bound_game_binary_ranges_alias_restore_and_error_bytes_match() {
    let f = Fixture::new();
    f.bound();
    let endpoint = &f.server().endpoint;
    assert_eq!(
        exchange(endpoint, &request(1, 0, 0, "test.iff")).unwrap(),
        wire::encode_success(1, f.data.len() as u64, &[]).unwrap()
    );
    for (offset, length) in [
        (0, wire::MAX_READ),
        (100, 190),
        (wire::MAX_READ as u64 + 10, 20),
        (f.data.len() as u64, 1),
    ] {
        let end = (offset as usize + length as usize).min(f.data.len());
        assert_eq!(
            exchange(endpoint, &request(2, offset, length, "TEST.IFF")).unwrap(),
            wire::encode_success(2, f.data.len() as u64, &f.data[offset as usize..end]).unwrap()
        );
    }
    for (path, status) in [("missing.iff", 2), ("../test.iff", 3)] {
        assert_eq!(
            exchange(endpoint, &request(1, 0, 0, path)).unwrap(),
            wire::encode_error(status)
        );
    }
    assert_eq!(
        exchange(endpoint, &request(2, 0, wire::MAX_READ + 1, "test.iff")).unwrap(),
        wire::encode_error(3)
    );
    let mut alias = f.control("set-alias");
    alias["logicalPath"] = json!("alias.iff");
    alias["sourceLogicalPath"] = json!("test.iff");
    assert_eq!(f.json(alias.clone())["ok"], true);
    assert_eq!(
        exchange(endpoint, &request(2, 2, 15, "alias.iff")).unwrap(),
        wire::encode_success(2, f.data.len() as u64, &f.data[2..17]).unwrap()
    );
    alias["restore"] = json!(true);
    assert_eq!(f.json(alias)["ok"], true);
    assert_eq!(
        exchange(endpoint, &request(1, 0, 0, "alias.iff")).unwrap(),
        wire::encode_error(2)
    );
    assert!(f.server().gate.bind_fixture(&own()).is_err());
    // A normal UI shutdown is not permitted to pull the provider from a live game.
    let mut f = f;
    assert!(f.server.as_mut().unwrap().shutdown().is_err());
    assert_eq!(f.json(f.control("status"))["ok"], true);
}

#[test]
fn ball_pipe_requires_bound_game_current_token_exact_choice_and_acknowledgement() {
    let f = Fixture::with_arena(true);
    let mut r = f.control("ball-select");
    r["assetId"] = json!(crate::blacktop_ball::test_support::id());
    r["acknowledgement"] = json!("apply-one-ball-for-current-game");
    assert_eq!(f.json(r.clone())["ok"], false);
    f.bound();
    let queued = f.json(r.clone());
    assert_eq!(queued["ok"], true);
    assert_eq!(queued["response"]["phase"], "queued");
    assert_eq!(queued["response"]["activeId"], "current");
    assert_eq!(queued["response"]["revision"], 1);
    for field in ["assetId", "address", "acknowledgement", "sessionToken"] {
        let mut bad = r.clone();
        bad[field] = match field {
            "assetId" => json!("../ball_x.iff"),
            "address" => json!(1234),
            "sessionToken" => json!("00".repeat(32)),
            _ => json!("apply-one-arena-for-current-game"),
        };
        assert_eq!(f.json(bad)["ok"], false);
    }
    r["assetId"] = json!("current");
    assert_eq!(f.json(r)["response"]["revision"], 2);
    assert!(!f.root.join("Data/blacktop-ball-session.json").exists());
}

#[test]
fn scoreboard_pipe_queues_exact_choices_without_memory_access_and_rejects_unsafe_layouts() {
    let f = Fixture::with_arena(true);
    let mut r = f.control("scoreboard-select");
    r["assetId"] = json!("broadcast");
    r["acknowledgement"] = json!("apply-one-scoreboard-for-current-game");
    assert_eq!(f.json(r.clone())["ok"], false);
    f.bound();
    let response = f.json(r.clone());
    assert_eq!(response["ok"], true);
    assert_eq!(response["response"]["phase"], "queued");
    assert_eq!(response["response"]["activeId"], "default");
    assert_eq!(response["response"]["gameplayVerified"], false);
    for id in [
        "mycareer-story",
        "flashback-openrun",
        "../broadcast",
        "mypark-simple",
    ] {
        let mut bad = r.clone();
        bad["assetId"] = json!(id);
        assert_eq!(f.json(bad)["ok"], false);
    }
    for field in ["address", "acknowledgement", "sessionToken"] {
        let mut bad = r.clone();
        bad[field] = match field {
            "address" => json!(12345),
            "sessionToken" => json!("00".repeat(32)),
            _ => json!("apply-one-ball-for-current-game"),
        };
        assert_eq!(f.json(bad)["ok"], false);
    }
    r["assetId"] = json!("default");
    assert_eq!(f.json(r)["response"]["revision"], 2);
    assert!(
        !f.root
            .join("Data")
            .join(crate::scoreboard_runtime::MARKER)
            .exists()
    );
}

#[test]
fn slow_oversized_and_partial_clients_do_not_hold_shutdown() {
    let mut f = Fixture::new();
    let endpoint = f.server().endpoint.clone();
    let rt = runtime();
    let mut clients = rt.block_on(async {
        let mut a = connect(&endpoint).await.unwrap();
        a.write_all(&[1, 0]).await.unwrap();
        let mut b = connect(&endpoint).await.unwrap();
        b.write_all(&100u32.to_le_bytes()).await.unwrap();
        b.write_all(&[1]).await.unwrap();
        let mut c = connect(&endpoint).await.unwrap();
        c.write_all(&((wire::MAX_FRAME + 1) as u32).to_le_bytes())
            .await
            .unwrap();
        let mut byte = [0];
        assert!(c.read(&mut byte).await.is_err() || byte == [0]);
        vec![a, b]
    });
    assert_eq!(f.json(f.control("status"))["ok"], true);
    let start = Instant::now();
    f.server.as_mut().unwrap().shutdown_fixture().unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    rt.block_on(async {
        for c in &mut clients {
            let mut b = [0];
            assert!(matches!(c.read(&mut b).await, Ok(0) | Err(_)));
        }
    });
    assert!(ClientOptions::new().open(&endpoint).is_err());
}

#[test]
fn abandoned_frame_has_deadline_and_server_keeps_serving() {
    let f = Fixture::new();
    let rt = runtime();
    let mut pipe = rt.block_on(connect(&f.server().endpoint)).unwrap();
    let start = Instant::now();
    rt.block_on(async {
        pipe.write_all(&[10, 0, 0, 0]).await.unwrap();
        let mut b = [0];
        let read = tokio::time::timeout(Duration::from_millis(3800), pipe.read(&mut b))
            .await
            .unwrap();
        assert!(matches!(read, Ok(0) | Err(_)));
    });
    assert!(start.elapsed() >= Duration::from_millis(2700));
    assert_eq!(f.json(f.control("status"))["ok"], true);
}

#[test]
fn concurrent_ranges_are_exact_and_draining_leases_prevent_false_cleanup() {
    let mut f = Fixture::new();
    f.bound();
    let work = (0..6)
        .map(|worker| {
            let e = f.server().endpoint.clone();
            let bytes = f.data.clone();
            thread::spawn(move || {
                for i in 0..12 {
                    let offset = (worker * 400 + i * 97) as u64;
                    assert_eq!(
                        exchange(&e, &request(2, offset, 333, "test.iff")).unwrap(),
                        wire::encode_success(
                            2,
                            bytes.len() as u64,
                            &bytes[offset as usize..offset as usize + 333]
                        )
                        .unwrap()
                    );
                }
            })
        })
        .collect::<Vec<_>>();
    for task in work {
        task.join().unwrap();
    }
    let permit = f.server().reads.clone().try_acquire_owned().unwrap();
    assert!(
        f.server
            .as_mut()
            .unwrap()
            .shutdown_fixture()
            .unwrap_err()
            .to_string()
            .contains("draining")
    );
    drop(permit);
    f.server.as_mut().unwrap().shutdown_fixture().unwrap();
}

#[test]
fn foreign_pid_is_denied_even_with_the_valid_session_token() {
    let f = Fixture::new();
    f.bound();
    // Fixed fixture script; all varying data is passed through child environment.
    let script = r#"$ErrorActionPreference='Stop'; $p=[IO.Pipes.NamedPipeClientStream]::new('.', $env:RUST_PROVIDER_PIPE, [IO.Pipes.PipeDirection]::InOut); try {$p.Connect(3000);$b=[Convert]::FromBase64String($env:RUST_PROVIDER_REQUEST);$h=[BitConverter]::GetBytes([uint32]$b.Length);$p.Write($h,0,4);$p.Write($b,0,$b.Length);$p.Flush();$r=[IO.BinaryReader]::new($p);$n=$r.ReadUInt32();if($n -gt 2097152){throw 'Too large'};$out=$r.ReadBytes([int]$n);if($out.Length -ne $n){throw 'Short response'};[BitConverter]::ToString($out).Replace('-','')}finally{$p.Dispose()}"#;
    // No extra dependency is needed for a fixture-only base64 transport.
    fn base64(b: &[u8]) -> String {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut s = String::new();
        for c in b.chunks(3) {
            let v = ((c[0] as u32) << 16)
                | ((c.get(1).copied().unwrap_or(0) as u32) << 8)
                | c.get(2).copied().unwrap_or(0) as u32;
            for shift in [18, 12, 6, 0] {
                s.push(alphabet[((v >> shift) & 63) as usize] as char);
            }
            if c.len() < 3 {
                s.pop();
                s.push('=');
            }
            if c.len() < 2 {
                let n = s.len();
                s.replace_range(n - 2..n - 1, "=");
            }
        }
        s
    }
    for body in [
        serde_json::to_vec(&f.control("status")).unwrap(),
        request(1, 0, 0, "test.iff"),
    ] {
        let mut child = Command::new("powershell.exe")
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                script,
            ])
            .env(
                "RUST_PROVIDER_PIPE",
                f.server().endpoint.strip_prefix(r"\\.\pipe\").unwrap(),
            )
            .env("RUST_PROVIDER_REQUEST", base64(&body))
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let start = Instant::now();
        while child.try_wait().unwrap().is_none() && start.elapsed() < Duration::from_secs(6) {
            thread::sleep(Duration::from_millis(10));
        }
        if child.try_wait().unwrap().is_none() {
            let _ = child.kill();
            let _ = child.wait();
            panic!("Owned provider fixture timed out");
        }
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let raw = String::from_utf8(out.stdout).unwrap();
        let response = raw
            .trim()
            .as_bytes()
            .chunks_exact(2)
            .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        if wire::has_magic(&body) {
            assert_eq!(response, wire::encode_error(1));
        } else {
            assert_eq!(
                serde_json::from_slice::<Value>(&response).unwrap()["ok"],
                false
            );
        }
    }
}

#[test]
fn tokens_acl_and_retained_process_birth_are_exact() {
    let s = Secret::new().unwrap();
    assert_eq!(s.text().len(), 64);
    assert!(s.matches(&s.text().to_uppercase()));
    for invalid in [
        String::new(),
        "x".repeat(64),
        "00".repeat(31),
        format!("{}x", s.text()),
    ] {
        assert!(!s.matches(&invalid));
    }
    assert_ne!(s.text(), Secret::new().unwrap().text());
    Acl::current_user().unwrap().audit().unwrap();
    let id = own();
    let p = Process::open(&id).unwrap();
    assert!(p.alive().unwrap());
    assert!(!p.accepts(id.pid.wrapping_add(1)).unwrap());
    let mut wrong = id.clone();
    wrong.started = "1".into();
    assert!(Process::open(&wrong).is_err());
    wrong = id;
    wrong.path.push_str(".other");
    assert!(Process::open(&wrong).is_err());
}

#[test]
fn exact_retained_fixture_game_exit_closes_provider_without_pid_only_inference() {
    let mut f = Fixture::new();
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Milliseconds 800",
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let result = (|| -> Result<()> {
        let id = session_safety::identity(child.id())?;
        let retained = Process::open(&id)?;
        f.server().gate.bind_fixture(&id)?;
        ensure!(
            f.server().gate.game_state()? == Some(true),
            "Fixture process did not bind"
        );
        ensure!(
            f.server.as_mut().unwrap().shutdown().is_err(),
            "Live fixture provider stopped"
        );
        let start = Instant::now();
        while child.try_wait()?.is_none() && start.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(10));
        }
        ensure!(child.try_wait()?.is_some(), "Owned fixture did not exit");
        ensure!(
            !retained.alive()?,
            "Retained fixture handle still signals alive"
        );
        while !f.server().finished() && start.elapsed() < Duration::from_secs(6) {
            thread::sleep(Duration::from_millis(10));
        }
        ensure!(
            f.server().finished(),
            "Provider did not observe retained exit"
        );
        f.server.as_mut().unwrap().shutdown()?;
        Ok(())
    })();
    if child.try_wait().unwrap().is_none() {
        let _ = child.kill();
        let _ = child.wait();
    }
    result.unwrap();
}

#[test]
fn supported_pe_build_policy_rejects_wrong_architecture_and_bounds() {
    let f = Fixture::new();
    let path = f.root.join("NBA2K27.exe");
    fn image(stamp: u32, size: u32) -> Vec<u8> {
        let mut b = vec![0; 4096];
        b[..2].copy_from_slice(b"MZ");
        b[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
        b[128..132].copy_from_slice(b"PE\0\0");
        b[132..134].copy_from_slice(&0x8664u16.to_le_bytes());
        b[136..140].copy_from_slice(&stamp.to_le_bytes());
        b[152..154].copy_from_slice(&0x20bu16.to_le_bytes());
        b[208..212].copy_from_slice(&size.to_le_bytes());
        b
    }
    for (stamp, size) in [(0x6AA2013D, 0x36558000), (0x6A974D13, 0x368C3000)] {
        fs::write(&path, image(stamp, size)).unwrap();
        let pinned = verify_build(&path).unwrap();
        // The Version 2 outfit admission is never implied by another build.
        assert_eq!(pinned.build["version2OutfitCarryMapped"], false);
        assert!(pinned.build["version2OutfitCarryError"].is_null());
        assert!(fs::write(&path, b"replace").is_err());
        drop(pinned);
    }
    for mut b in [
        // A Version 2 header alone never grants access without the disk ABI proof.
        image(0x6AC53765, 0x35BDE000),
        image(0, 0x36558000),
        image(0x6AA2013D, 0),
        image(0x6AA2013D, 0x36558000),
    ] {
        if b[136..140] == 0x6AA2013Du32.to_le_bytes() {
            b[132..134].copy_from_slice(&0x14cu16.to_le_bytes());
        }
        fs::write(&path, b).unwrap();
        assert!(verify_build(&path).is_err());
    }
    let mut b = image(0x6AA2013D, 0x36558000);
    b[0x3c..0x40].copy_from_slice(&u32::MAX.to_le_bytes());
    fs::write(&path, b).unwrap();
    assert!(verify_build(&path).is_err());
    fs::write(&path, [0; 40]).unwrap();
    assert!(verify_build(&path).is_err());
}
