//! Native mod provider process and parent supervisor. No loader installation or
//! game-memory access in this module. The helper is NOT in a kill-on-close Job:
//! after binding, its exact retained game handle controls its lifetime.
#![allow(dead_code)]
use crate::{
    provider_access::{self as access, Gate},
    provider_assets::Service,
    provider_pipe::Server,
    provider_protocol as wire,
    session_safety::{self, Identity},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    os::windows::{fs::OpenOptionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, OnceLock, atomic::AtomicBool, mpsc},
    thread,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::ClientOptions,
};
use windows::Win32::{
    Foundation::{CloseHandle, ERROR_NO_MORE_FILES},
    System::Diagnostics::ToolHelp::*,
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Bootstrap {
    schema: String,
    parent: Identity,
    game_exe: PathBuf,
    mods_root: PathBuf,
    aliases: Vec<(String, String)>,
    mutable_aliases: Vec<String>,
    #[serde(default)]
    launch_owner: Option<crate::provider_launch::Owner>,
    #[serde(default)]
    features: crate::feature_plan::Plan,
}
const SCHEMA: &str = "nba2k27-native-provider-worker-v1";
fn line(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut b = Vec::new();
    io::BufReader::new(reader.take(limit + 1)).read_until(b'\n', &mut b)?;
    ensure!(
        !b.is_empty() && b.len() as u64 <= limit && b.last() == Some(&b'\n'),
        "Missing or oversized private provider handshake"
    );
    Ok(b)
}
fn parent_pid() -> Result<u32> {
    let h = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    struct Snapshot(windows::Win32::Foundation::HANDLE);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    let h = Snapshot(h);
    let mut e = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe {
        Process32FirstW(h.0, &mut e)?;
    }
    loop {
        if e.th32ProcessID == std::process::id() {
            return Ok(e.th32ParentProcessID);
        }
        match unsafe { Process32NextW(h.0, &mut e) } {
            Ok(()) => {}
            Err(e) if e.code() == ERROR_NO_MORE_FILES.to_hresult() => break,
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("Provider parent identity unavailable")
}

pub fn run() -> Result<()> {
    let process_clock = Instant::now();
    let (tx, rx) = mpsc::sync_channel(1);
    // A malformed parent cannot leave an invisible helper waiting on stdin.
    thread::Builder::new()
        .name("provider-bootstrap".into())
        .spawn(move || {
            let _ = tx.send(line(io::stdin(), 32768));
        })?;
    let boot: Bootstrap = serde_json::from_slice(
        &rx.recv_timeout(Duration::from_secs(5))
            .context("Provider bootstrap timed out")??,
    )?;
    ensure!(
        boot.schema == SCHEMA && boot.parent.pid == parent_pid()?,
        "Unexpected provider parent"
    );
    if let Some(owner) = &boot.launch_owner {
        // A damaged/missing unused catalog never prevents ordinary mod loading.
        let _ = crate::custom_blacktop::load(&owner.portable);
    }
    boot.features.validate()?;
    ensure!(
        boot.launch_owner.is_some() || !boot.features.enabled(),
        "Features require a retained launch owner"
    );
    let gate = Arc::new(Gate::new(&boot.parent, &boot.game_exe)?);
    let bootstrap_ms = process_clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let index_clock = Instant::now();
    let assets = Arc::new(Service::open(
        &boot.mods_root,
        &boot.aliases,
        &boot.mutable_aliases,
        Arc::new(AtomicBool::new(false)),
    )?);
    if let Some(owner) = &boot.launch_owner {
        let _ = crate::custom_blacktop::publish(&owner.portable, &boot.mods_root, &assets, None);
    }
    let (indexed, aliases, _) = assets.counts()?;
    let index_ms = index_clock.elapsed().as_millis().min(u64::MAX as u128) as u64;
    let launch_context = boot
        .launch_owner
        .map(|owner| crate::provider_launch::ContextData {
            owner,
            parent: boot.parent.clone(),
            game_exe: boot.game_exe.clone(),
            mods_root: boot.mods_root.clone(),
            features: boot.features.clone(),
        });
    let controls = Arc::new(crate::provider_launch::Control::new_with_timing(
        launch_context,
        bootstrap_ms,
        index_ms,
    )?);
    let mut server = Server::start_with(assets.clone(), gate.clone(), Some(controls.clone()))?;
    // This secret travels only through the child's inherited stdout handle.
    // Never put it on the command line, in settings, a journal, or diagnostics.
    let ready = json!({"schema":SCHEMA,"ready":true,"endpoint":server.endpoint,"token":gate.secret.text(),"indexedAssets":indexed,"indexedAliases":aliases,"bootstrapMs":bootstrap_ms,"indexMs":index_ms});
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &ready)?;
    out.write_all(b"\n")?;
    out.flush()?;
    drop(out);
    while !server.finished() {
        if controls.claim() {
            return controls.execute(&assets, &gate, &server.endpoint.clone(), &mut server);
        }
        thread::sleep(Duration::from_millis(25));
    }
    server.shutdown()?;
    Ok(())
}

pub(crate) struct Worker {
    child: Child,
    pub identity: Identity,
    pub endpoint: String,
    token: String,
    pub indexed: usize,
    started: bool,
}
// Only exact children created and handshaken by this process are excluded from
// other-launcher conflicts. Never grant an exception by executable name/PID.
static OWNED: OnceLock<Mutex<Vec<access::Process>>> = OnceLock::new();
pub(crate) fn owned(identity: &Identity) -> bool {
    let Some(registry) = OWNED.get() else {
        return false;
    };
    let Ok(mut children) = registry.lock() else {
        return false;
    };
    children.retain(|p| p.alive().unwrap_or(true));
    children
        .iter()
        .any(|p| p.identity == *identity && p.alive().is_ok_and(|alive| alive))
}
impl Worker {
    pub fn start(
        game_exe: &Path,
        mods_root: &Path,
        aliases: &[(String, String)],
        mutable: &[String],
    ) -> Result<Self> {
        Self::start_with_owner(
            game_exe,
            mods_root,
            aliases,
            mutable,
            None,
            Default::default(),
        )
    }
    pub fn start_owned(
        game_exe: &Path,
        mods_root: &Path,
        owner: crate::provider_launch::Owner,
        features: crate::feature_plan::Plan,
    ) -> Result<Self> {
        Self::start_with_owner(game_exe, mods_root, &[], &[], Some(owner), features)
    }
    fn start_with_owner(
        game_exe: &Path,
        mods_root: &Path,
        aliases: &[(String, String)],
        mutable: &[String],
        launch_owner: Option<crate::provider_launch::Owner>,
        features: crate::feature_plan::Plan,
    ) -> Result<Self> {
        let exe = std::env::current_exe()?.canonicalize()?;
        let _pin = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&exe)?;
        let parent = session_safety::identity(std::process::id())?;
        let boot = Bootstrap {
            schema: SCHEMA.into(),
            parent,
            game_exe: game_exe.into(),
            mods_root: mods_root.into(),
            aliases: aliases.into(),
            mutable_aliases: mutable.into(),
            launch_owner,
            features,
        };
        let mut data = serde_json::to_vec(&boot)?;
        data.push(b'\n');
        ensure!(data.len() <= 32768, "Provider bootstrap exceeds limit");
        let mut child = Command::new(&exe)
            .arg("--native-mod-provider")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let initialize = (|| -> Result<(Identity, Value)> {
            let identity = session_safety::identity(child.id())?;
            ensure!(
                crate::native_session::same_path(&identity.path, &exe),
                "Provider executable changed"
            );
            let mut stdin = child
                .stdin
                .take()
                .context("Private provider stdin unavailable")?;
            stdin.write_all(&data)?;
            stdin.flush()?;
            drop(stdin);
            let stdout = child
                .stdout
                .take()
                .context("Private provider stdout unavailable")?;
            let (tx, rx) = mpsc::sync_channel(1);
            thread::Builder::new()
                .name("provider-ready".into())
                .spawn(move || {
                    let _ = tx.send(line(stdout, 8192));
                })?;
            let ready: Value = serde_json::from_slice(
                &rx.recv_timeout(Duration::from_secs(40))
                    .context("Provider indexing/ready deadline exceeded")??,
            )?;
            ensure!(
                ready["schema"] == SCHEMA && ready["ready"] == true,
                "Native provider did not become ready"
            );
            ensure!(
                child.try_wait()?.is_none(),
                "Native provider exited during bootstrap"
            );
            access::Process::open(&identity)?;
            validate_ready(&ready)?;
            Ok((identity, ready))
        })();
        let (identity, ready) = match initialize {
            Ok(v) => v,
            Err(e) => {
                // No credential has been published to a caller, so this newly
                // spawned, unbound helper cannot yet serve a game. Only our Child
                // handle is terminated; never a PID/name based process kill.
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let endpoint = ready["endpoint"]
            .as_str()
            .context("Provider endpoint missing")?
            .to_owned();
        let token = ready["token"]
            .as_str()
            .context("Provider credential missing")?
            .to_owned();
        let indexed = ready["indexedAssets"]
            .as_u64()
            .context("Provider index count missing")? as usize;
        let worker = Self {
            child,
            identity,
            endpoint,
            token,
            indexed,
            started: false,
        };
        let retained = access::Process::open(&worker.identity)?;
        let mut registry = OWNED
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .map_err(|_| anyhow::anyhow!("Provider ownership registry unavailable"))?;
        registry.retain(|p| p.alive().unwrap_or(true));
        ensure!(
            registry.len() < 16,
            "Native provider helper count exceeds limit"
        );
        registry.push(retained);
        Ok(worker)
    }
    pub fn launch(&mut self) -> Result<Value> {
        let state = self.control(json!({"operation":"modded-start"}))?;
        self.started = true;
        Ok(state)
    }
    pub fn control(&self, mut request: Value) -> Result<Value> {
        let o = request
            .as_object_mut()
            .context("Provider control must be an object")?;
        o.insert("sessionToken".into(), json!(self.token));
        let body = serde_json::to_vec(&request)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()?;
        let result = runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(4), async {
                let started = Instant::now();
                let mut pipe = loop {
                    match ClientOptions::new().open(&self.endpoint) {
                        Ok(p) => break p,
                        Err(e)
                            if e.raw_os_error() == Some(231)
                                && started.elapsed() < Duration::from_secs(1) =>
                        {
                            tokio::time::sleep(Duration::from_millis(5)).await
                        }
                        Err(e) => return Err(anyhow::Error::from(e)),
                    }
                };
                pipe.write_all(&wire::encode_frame(&body)?).await?;
                let mut h = [0; 4];
                pipe.read_exact(&mut h).await?;
                let size = u32::from_le_bytes(h) as usize;
                ensure!(
                    (1..=wire::MAX_FRAME).contains(&size),
                    "Invalid provider reply size"
                );
                let mut b = vec![0; size];
                pipe.read_exact(&mut b).await?;
                let reply: Value = serde_json::from_slice(&b)?;
                ensure!(
                    reply["ok"] == true,
                    "Native provider refused control request"
                );
                Ok(reply["response"].clone())
            })
            .await
            .context("Native provider control timed out")?
        });
        runtime.shutdown_timeout(Duration::from_millis(50));
        result
    }
    pub fn bind_game(&self, identity: &Identity) -> Result<()> {
        self.control(json!({"operation":"bind-game","identity":identity}))?;
        Ok(())
    }
    pub fn exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
    pub fn stop(&mut self) -> Result<()> {
        if self.exited()? {
            return Ok(());
        }
        self.control(json!({"operation":"stop"}))?;
        let start = Instant::now();
        while !self.exited()? && start.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(10));
        }
        ensure!(
            self.exited()?,
            "Provider still owns resources; keep recovery pending"
        );
        Ok(())
    }
}
fn validate_ready(ready: &Value) -> Result<()> {
    let endpoint = ready["endpoint"]
        .as_str()
        .context("Provider endpoint missing")?;
    let token = ready["token"]
        .as_str()
        .context("Provider credential missing")?;
    ensure!(
        endpoint
            .strip_prefix(wire::PIPE_PREFIX)
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            && token.len() == 64
            && token.bytes().all(|b| b.is_ascii_hexdigit())
            && ready["indexedAssets"]
                .as_u64()
                .is_some_and(|n| n <= 100_000)
            && ready["bootstrapMs"].as_u64().is_some_and(|n| n <= 40_000)
            && ready["indexMs"].as_u64().is_some_and(|n| n <= 40_000),
        "Invalid private provider handshake"
    );
    Ok(())
}
impl Drop for Worker {
    fn drop(&mut self) {
        // A live game deliberately refuses stop. Dropping this Child handle
        // does not kill it; the retained game handle in the worker decides.
        if !self.started {
            let _ = self.stop();
        }
    }
}

pub fn self_check(root: &Path) -> Result<()> {
    ensure!(root.is_absolute(), "Isolated fixture root required");
    std::fs::create_dir_all(root.join("Logs"))?;
    let fixture = root.join(format!("ProviderFixture-{}", crate::backend::unique_id()));
    std::fs::create_dir_all(&fixture)?;
    std::fs::write(fixture.join("fixture.iff"), b"native worker fixture")?;
    let outcome = (|| -> Result<Value> {
        let mut worker = Worker::start(&fixture.join("NBA2K27.exe"), &fixture, &[], &[])?;
        let identity = worker.identity.clone();
        ensure!(
            owned(&identity),
            "Private child ownership was not registered"
        );
        let mut wrong = identity.clone();
        wrong.started = "1".into();
        ensure!(!owned(&wrong), "Reused child identity accepted");
        let status = worker.control(json!({"operation":"status"}))?;
        ensure!(
            status["indexedAssets"] == 1 && status["gameBound"] == false,
            "Unexpected fixture provider state"
        );
        let queried = worker.control(json!({"operation":"query","logicalPath":"fixture.iff"}))?;
        ensure!(queried["size"] == 21, "Private fixture query differs");
        // Synthetic filename only; this does not open a game or enumerate it.
        ensure!(
            worker.bind_game(&identity).is_err(),
            "Non-game identity was accepted"
        );
        let start = Instant::now();
        worker.stop()?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        ensure!(worker.exited()?, "Fixture helper remains alive");
        ensure!(!owned(&identity), "Exited child ownership remained valid");
        let mut prepared = Worker::start_owned(
            &fixture.join("NBA2K27.exe"),
            &fixture,
            crate::provider_launch::Owner {
                portable: root.into(),
                session: crate::backend::unique_id(),
            },
            Default::default(),
        )?;
        ensure!(
            prepared.control(json!({"operation":"modded-status"}))?["phase"] == "ready",
            "Owned launch was not idle before explicit start"
        );
        ensure!(
            !fixture.join("version.dll").exists(),
            "Unstarted fixture modified game files"
        );
        prepared.stop()?;
        Ok(
            json!({"schema":"nba2k27-provider-worker-check-v1","passed":true,"workerPid":identity.pid,"workerExited":true,"indexedAssets":1,"foreignGameIdentityDenied":true,"exactChildRegistered":true,"reusedChildIdentityDenied":true,"exitedChildOwnershipCleared":true,"ownedLaunchRequiresExplicitStart":true,"preparedUnstartedHelperExited":prepared.exited()?,"closeMs":elapsed,"tokenOnCommandLine":false,"tokenInReport":false,"gameAccess":false,"gameFilesChanged":false,"bundledRuntimeAdded":false}),
        )
    })();
    // This exact uniquely-created fixture is the only deletion target.
    let cleanup = std::fs::remove_dir_all(&fixture);
    let report = outcome?;
    cleanup?;
    crate::backend::atomic_json(&root.join("Logs/provider-worker-check.json"), &report)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bootstrap_line_is_small_complete_and_does_not_accept_trailing_partial_input() {
        assert_eq!(line(&b"{}\n"[..], 3).unwrap(), b"{}\n");
        for bytes in [&b"{}"[..], &b""[..], &b"abcd\n"[..]] {
            assert!(line(bytes, 3).is_err());
        }
        assert_eq!(line(&b"{}\nignored"[..], 3).unwrap(), b"{}\n");
        assert!(serde_json::from_value::<Bootstrap>(json!({"schema":SCHEMA,"parent":own_fixture(),"gameExe":"C:\\Fixture\\NBA2K27.exe","modsRoot":"C:\\Fixture","aliases":[],"mutableAliases":[],"token":"no-cli-secrets"})).is_err());
    }
    fn own_fixture() -> Value {
        json!({"pid":123,"started":"1","path":"C:\\Fixture\\launcher.exe"})
    }
}
