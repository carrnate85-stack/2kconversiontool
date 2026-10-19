//! Private, bounded provider transport owned by the modded-launch worker until
//! verified game exit. It deliberately does not use the inert QA probe's
//! kill-on-launcher-close Job policy: the game may still retain served assets.
#![allow(dead_code)]
use crate::{
    provider_access::{self as access, Gate},
    provider_assets::Service,
    provider_protocol as wire,
    session_safety::Identity,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    os::windows::io::AsRawHandle,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{NamedPipeServer, ServerOptions},
    sync::Semaphore,
    task::JoinSet,
};
use windows::Win32::Foundation::HANDLE;
const CLIENTS: usize = 8;
const REQUEST_TIME: Duration = Duration::from_secs(3);
struct RequestGuard(Arc<AtomicBool>);
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
pub(crate) struct Server {
    pub endpoint: String,
    pub gate: Arc<Gate>,
    stop: Arc<AtomicBool>,
    assets: Arc<Service>,
    reads: Arc<Semaphore>,
    terminal_error: Option<String>,
    thread: Option<JoinHandle<Result<()>>>,
}
impl Server {
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(|t| t.is_finished())
    }
    pub fn start(assets: Arc<Service>, gate: Arc<Gate>) -> Result<Self> {
        Self::start_with(assets, gate, None)
    }
    pub fn start_with(
        assets: Arc<Service>,
        gate: Arc<Gate>,
        controls: Option<Arc<crate::provider_launch::Control>>,
    ) -> Result<Self> {
        let endpoint = format!("{}{}", wire::PIPE_PREFIX, access::hex(&access::random()?));
        let stop = Arc::new(AtomicBool::new(false));
        let reads = Arc::new(Semaphore::new(CLIENTS));
        let permits = reads.clone();
        let (a, g, s, e) = (assets.clone(), gate.clone(), stop.clone(), endpoint.clone());
        let (tx, rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("native-mod-provider".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build()?;
                let result = runtime.block_on(async move {
                    let acl = access::Acl::current_user()?;
                    let mut listener = create(&e, &acl, true)?;
                    tx.send(())
                        .map_err(|_| anyhow::anyhow!("Provider startup was cancelled"))?;
                    let mut workers = JoinSet::new();
                    let outcome = async {
                        while !s.load(Ordering::Acquire) {
                            while workers.try_join_next().is_some() {}
                            // The retained game handle, never a PID-only absence,
                            // determines whether this provider's game has ended.
                            let game = g.game_state()?;
                            if game == Some(false) {
                                break;
                            }
                            if !g.parent_alive()?
                                && game.is_none()
                                && !controls.as_ref().is_some_and(|c| c.pending())
                            {
                                break;
                            }
                            match tokio::time::timeout(
                                Duration::from_millis(50),
                                listener.connect(),
                            )
                            .await
                            {
                                Err(_) => continue,
                                Ok(result) => result?,
                            }
                            let next = create(&e, &acl, false)?;
                            let connected = std::mem::replace(&mut listener, next);
                            if workers.len() >= CLIENTS {
                                drop(connected);
                                continue;
                            }
                            let (a, g, s, p, c) = (
                                a.clone(),
                                g.clone(),
                                s.clone(),
                                permits.clone(),
                                controls.clone(),
                            );
                            workers.spawn(async move {
                                let _ = tokio::time::timeout(
                                    REQUEST_TIME,
                                    serve(connected, a, g, s, p, c),
                                )
                                .await;
                            });
                        }
                        Ok::<(), anyhow::Error>(())
                    }
                    .await;
                    s.store(true, Ordering::Release);
                    a.cancel();
                    workers.abort_all();
                    while workers.join_next().await.is_some() {}
                    drop(listener);
                    outcome
                });
                // A timed-out file read keeps its semaphore lease until completion.
                // Never wait forever on a slow physical disk while closing transport.
                runtime.shutdown_timeout(Duration::from_millis(200));
                result
            })?;
        if rx.recv_timeout(Duration::from_secs(5)).is_err() {
            stop.store(true, Ordering::Release);
            assets.cancel();
            if thread.is_finished() {
                match thread.join() {
                    Ok(Err(error)) => return Err(error.context("Native provider startup failed")),
                    Err(_) => anyhow::bail!("Native provider startup worker failed"),
                    Ok(Ok(())) => {}
                }
            }
            anyhow::bail!("Native provider startup failed or timed out");
        }
        Ok(Self {
            endpoint,
            gate,
            stop,
            assets,
            reads,
            terminal_error: None,
            thread: Some(thread),
        })
    }
    pub fn shutdown(&mut self) -> Result<()> {
        ensure!(
            self.gate.game_state()? != Some(true),
            "Keep the provider alive until the game exits"
        );
        self.shutdown_transport()
    }
    fn shutdown_transport(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        self.assets.cancel();
        let started = Instant::now();
        while self.thread.as_ref().is_some_and(|t| !t.is_finished())
            && started.elapsed() < Duration::from_secs(1)
        {
            thread::sleep(Duration::from_millis(5));
        }
        ensure!(
            self.thread.as_ref().is_none_or(|t| t.is_finished()),
            "Provider is still closing; keep recovery ownership"
        );
        ensure!(
            self.reads.available_permits() == CLIENTS,
            "Provider file operations are still draining; keep recovery ownership"
        );
        if let Some(thread) = self.thread.take() {
            match thread.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => self.terminal_error = Some(format!("{e:#}")),
                Err(_) => self.terminal_error = Some("Provider worker failed".into()),
            }
        }
        ensure!(
            self.terminal_error.is_none(),
            "Provider stopped with an error: {}",
            self.terminal_error.as_deref().unwrap_or("")
        );
        self.assets.release_closed_handles()?;
        Ok(())
    }
    #[cfg(test)]
    fn shutdown_fixture(&mut self) -> Result<()> {
        self.shutdown_transport()
    }
}

#[cfg(test)]
#[path = "provider_pipe_tests.rs"]
mod tests;
impl Drop for Server {
    fn drop(&mut self) {
        // Do not stop a provider used by a live game. The eventual dedicated
        // worker process, not a UI lifetime, must retain it through game exit.
        if self.gate.game_state().is_ok_and(|s| s != Some(true)) {
            let _ = self.shutdown_transport();
        }
    }
}
fn create(endpoint: &str, acl: &access::Acl, first: bool) -> Result<NamedPipeServer> {
    let mut attrs = acl.attributes();
    Ok(unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(16)
            .in_buffer_size(65536)
            .out_buffer_size(65536)
            .create_with_security_attributes_raw(endpoint, &mut attrs as *mut _ as _)?
    })
}
async fn serve(
    mut pipe: NamedPipeServer,
    assets: Arc<Service>,
    gate: Arc<Gate>,
    stop: Arc<AtomicBool>,
    permits: Arc<Semaphore>,
    controls: Option<Arc<crate::provider_launch::Control>>,
) -> Result<()> {
    let deadline = Instant::now() + REQUEST_TIME;
    let cancelled = Arc::new(AtomicBool::new(false));
    let _guard = RequestGuard(cancelled.clone());
    let mut header = [0u8; 4];
    pipe.read_exact(&mut header).await?;
    let size = u32::from_le_bytes(header) as usize;
    ensure!(
        size > 0 && size <= wire::MAX_FRAME,
        "Invalid provider frame size"
    );
    let mut body = vec![0; size];
    pipe.read_exact(&mut body).await?;
    let pid = access::client_pid(HANDLE(pipe.as_raw_handle()))?;
    let binary = wire::has_magic(&body);
    let (mut finish, mut authorized) = (false, false);
    let response = if binary {
        if !gate.game(pid)? {
            wire::encode_error(1)
        } else {
            let permit = permits.try_acquire_owned().context("Provider is busy")?;
            let a = assets.clone();
            let result = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                a.binary_reply(&body)
            })
            .await?;
            ensure!(
                gate.game(pid)? && !stop.load(Ordering::Acquire),
                "Game exited during provider read"
            );
            result
        }
    } else {
        let request: Value = serde_json::from_slice(&body)?;
        let token = request["sessionToken"].as_str().unwrap_or("");
        if !gate.parent(pid, token)? {
            serde_json::to_vec(
                &json!({"ok":false,"error":"Invalid provider session token or owner"}),
            )?
        } else {
            authorized = true;
            let check = || -> Result<()> {
                ensure!(
                    !stop.load(Ordering::Acquire)
                        && !cancelled.load(Ordering::Acquire)
                        && Instant::now() < deadline
                        && gate.parent_alive()?,
                    "Provider request expired"
                );
                Ok(())
            };
            let result = match request["operation"].as_str() {
                Some("bind-game") => {
                    let id: Identity = serde_json::from_value(request["identity"].clone())?;
                    gate.bind(&id, check)
                        .map(|_| json!({"operation":"bind-game","bound":true}))
                }
                Some("stop") => {
                    ensure!(
                        gate.game_state()? != Some(true),
                        "Keep provider alive until game exit"
                    );
                    if let Some(c) = &controls {
                        c.cancel_before_dispatch()?;
                    }
                    finish = true;
                    Ok(json!({"operation":"stop","stopping":true}))
                }
                Some(op @ ("modded-start" | "modded-status")) => controls
                    .as_ref()
                    .context("Native launch control is unavailable")?
                    .request(op, check),
                Some("outfit-command") => (|| -> Result<Value> {
                    #[derive(serde::Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Request {
                        operation: String,
                        session_token: String,
                        acknowledgement: String,
                        command: crate::outfit_runtime::Command,
                    }
                    let r: Request = serde_json::from_value(request.clone())?;
                    ensure!(
                        r.operation == "outfit-command"
                            && !r.session_token.is_empty()
                            && r.acknowledgement
                                == if r.command.writes() {
                                    "apply-one-outfit-for-current-game"
                                } else {
                                    "read-blacktop-outfits"
                                },
                        "Invalid outfit operation acknowledgement"
                    );
                    controls
                        .as_ref()
                        .context("Native outfit control unavailable")?
                        .outfit_command(r.command, || {
                            check()?;
                            ensure!(
                                gate.game_state()? == Some(true),
                                "The exact launched game is not running"
                            );
                            Ok(())
                        })
                })(),
                Some("camera-command") => (|| -> Result<Value> {
                    #[derive(serde::Deserialize)]
                    #[serde(rename_all = "camelCase", deny_unknown_fields)]
                    struct Request {
                        operation: String,
                        session_token: String,
                        acknowledgement: String,
                        command: crate::feature_runtime::CameraCommand,
                    }
                    let r: Request = serde_json::from_value(request.clone())?;
                    ensure!(
                        r.operation == "camera-command"
                            && !r.session_token.is_empty()
                            && r.acknowledgement == "control-cameras-for-current-game",
                        "Invalid camera operation acknowledgement"
                    );
                    controls
                        .as_ref()
                        .context("Native camera control unavailable")?
                        .camera_command(r.command, || {
                            check()?;
                            ensure!(
                                gate.game_state()? == Some(true),
                                "The exact launched game is not running"
                            );
                            Ok(())
                        })
                })(),
                Some(operation @ ("arena-select" | "ball-select" | "scoreboard-select")) => {
                    (|| -> Result<Value> {
                        #[derive(serde::Deserialize)]
                        #[serde(rename_all = "camelCase", deny_unknown_fields)]
                        struct Request {
                            operation: String,
                            session_token: String,
                            asset_id: String,
                            acknowledgement: String,
                        }
                        let r: Request = serde_json::from_value(request.clone())?;
                        ensure!(
                            r.operation == operation
                                && r.acknowledgement
                                    == match operation {
                                        "arena-select" => "apply-one-arena-for-current-game",
                                        "ball-select" => "apply-one-ball-for-current-game",
                                        "scoreboard-select" =>
                                            "apply-one-scoreboard-for-current-game",
                                        _ => unreachable!(),
                                    }
                                && !r.session_token.is_empty(),
                            "Invalid catalog operation acknowledgement"
                        );
                        let control = controls
                            .as_ref()
                            .context("Native catalog control unavailable")?;
                        let authorize = || {
                            check()?;
                            ensure!(
                                gate.game_state()? == Some(true),
                                "The exact launched game is not running"
                            );
                            Ok(())
                        };
                        match operation {
                            "arena-select" => control.select_arena(&r.asset_id, authorize),
                            "ball-select" => control.select_ball(&r.asset_id, authorize),
                            "scoreboard-select" => {
                                control.select_scoreboard(&r.asset_id, authorize)
                            }
                            _ => unreachable!(),
                        }
                    })()
                }
                _ => {
                    let permit = permits.try_acquire_owned().context("Provider is busy")?;
                    let (a, g, c, s) = (
                        assets.clone(),
                        gate.clone(),
                        cancelled.clone(),
                        stop.clone(),
                    );
                    tokio::task::spawn_blocking(move||{
                        let _permit=permit;
                        let check=||->Result<()>{ensure!(!c.load(Ordering::Acquire)&&!s.load(Ordering::Acquire)&&Instant::now()<deadline&&g.parent_alive()?,"Provider request expired");Ok(())};
                        check()?;
                        match request["operation"].as_str(){
                            Some("status")=>{let(indexed,aliases,cache)=a.counts()?;let game=g.game_state()?;Ok(json!({"indexedAssets":indexed,"indexedAliases":aliases,"cachedFiles":cache,"gameBound":game.is_some(),"gameAlive":game,"security":{"currentUserOnly":true,"sessionToken":true,"tokenOnCommandLine":false}}))},
                            Some("query")=>{let a=a.query(request["logicalPath"].as_str().context("Asset path missing")?)?;Ok(json!({"operation":"query","logicalPath":a.logical,"size":a.size}))},
                            Some("set-alias")=>{
                                let target=request["logicalPath"].as_str().context("Alias path missing")?;
                                let restore=request["restore"]==true;
                                let source=if restore{None}else{Some(request["sourceLogicalPath"].as_str().context("Alias source missing")?)};
                                let a=a.set_alias_checked(target,source,check)?;
                                Ok(json!({"operation":"set-alias","logicalPath":target.replace('\\',"/"),"sourceLogicalPath":source.unwrap_or("original"),"available":a.is_some(),"restored":restore,"size":a.map_or(0,|a|a.size)}))
                            },
                            _=>anyhow::bail!("Unsupported provider control operation"),
                        }
                    }).await?
                }
            };
            match result {
                Ok(response) => serde_json::to_vec(&json!({"ok":true,"response":response}))?,
                Err(_) => serde_json::to_vec(
                    &json!({"ok":false,"error":"Provider control operation failed"}),
                )?,
            }
        }
    };
    if authorized {
        ensure!(gate.parent_alive()?, "Provider owner exited before reply");
    }
    let frame = wire::encode_frame(&response)?;
    tokio::time::timeout(Duration::from_millis(200), async {
        pipe.write_all(&frame).await?;
        pipe.flush().await
    })
    .await??;
    if finish {
        stop.store(true, Ordering::Release);
        assets.cancel();
    }
    Ok(())
}
