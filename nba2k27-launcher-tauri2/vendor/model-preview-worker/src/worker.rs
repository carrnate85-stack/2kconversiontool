//! Persistent native decoder process, launched from the same EXE without UI.
//! Asset sessions stay in memory. Crashes/hangs cannot take down the Tauri host.
use crate::backend::{Backend, Emit};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
const MAX_FRAME: usize = 256 * 1024 * 1024;
pub const MAX_REQUEST_ARGS: usize = 1024 * 1024;
// The frame also contains the channel, request ID and JSON envelope.
const MAX_REQUEST_FRAME: usize = MAX_REQUEST_ARGS + 1024;
fn read_frame(reader: &mut impl Read, max: usize) -> Result<Value> {
    let mut size = [0u8; 4];
    reader.read_exact(&mut size)?;
    let size = u32::from_le_bytes(size) as usize;
    ensure!(
        size > 0 && size <= max,
        "Native worker message exceeds limit"
    );
    let mut bytes = vec![0u8; size];
    reader.read_exact(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}
fn frame_bytes(value: &Value, max: usize) -> Result<Vec<u8>> {
    struct BoundedBytes {
        bytes: Vec<u8>,
        max: usize,
    }
    impl Write for BoundedBytes {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max.saturating_sub(self.bytes.len()) {
                return Err(std::io::Error::other(
                    "Native worker response exceeds IPC limit",
                ));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = BoundedBytes {
        bytes: Vec::new(),
        max,
    };
    serde_json::to_writer(&mut output, value)?;
    Ok(output.bytes)
}
fn write_bytes(writer: &mut impl Write, bytes: &[u8]) -> Result<()> {
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    Ok(())
}
fn write_frame(writer: &mut impl Write, value: &Value) -> Result<()> {
    write_bytes(writer, &frame_bytes(value, MAX_FRAME)?)
}
fn write_reply(writer: &mut impl Write, reply: &Value, max: usize) -> Result<()> {
    // Serialization/budget errors are request failures, not transport failures.
    // Never partially write an oversized frame or kill a healthy asset session.
    let bytes = match frame_bytes(reply, max) {
        Ok(bytes) => bytes,
        Err(_) => frame_bytes(
            &json!({"kind":"reply","id":reply["id"],"ok":false,
            "error":"Native worker response exceeds IPC limit; choose a smaller preview or inspection."}),
            MAX_FRAME,
        )?,
    };
    write_bytes(writer, &bytes)
}
pub fn serve() -> Result<()> {
    // Native geometry/material parsing can recurse through deeply nested 2K
    // structures. Windows gives the process main thread a comparatively small
    // stack, so host the persistent decoder loop on an explicitly sized stack.
    std::thread::Builder::new()
        .name("model-preview-decoder".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(serve_inner)
        .context("Could not start the model preview decoder thread")?
        .join()
        .map_err(|_| anyhow::anyhow!("Model preview decoder thread stopped unexpectedly"))?
}
fn serve_inner() -> Result<()> {
    let cancel = Arc::new(AtomicBool::new(false));
    let input_cancel = cancel.clone();
    let (tx, rx) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        while let Ok(command) = read_frame(&mut input, MAX_REQUEST_FRAME) {
            if command["channel"] == "cancel" {
                input_cancel.store(true, Ordering::Relaxed);
            } else if tx.send(command).is_err() {
                break;
            }
        }
        input_cancel.store(true, Ordering::Relaxed);
    });
    let output = Arc::new(Mutex::new(std::io::stdout()));
    let events = output.clone();
    let emit: Emit = Arc::new(move |event| {
        let _ = write_frame(&mut *events.lock().unwrap(), &event);
    });
    let mut backend = Backend::new(emit, cancel);
    for command in rx {
        let channel = command["channel"]
            .as_str()
            .context("Worker channel missing")?;
        let args = command["args"]
            .as_array()
            .context("Worker arguments missing")?;
        let diagnostic = std::env::args().any(|a| a == "--worker-diagnostics");
        let result = if diagnostic && channel == "test:oversized-reply" {
            Ok(json!("x".repeat(2048)))
        } else if diagnostic && channel.starts_with("test:") {
            if channel == "test:partial-hang" {
                let folder = backend
                    .output
                    .as_ref()
                    .context("Diagnostic output missing")?
                    .join(format!("Rust-export-diagnostic-{}", std::process::id()));
                std::fs::create_dir(&folder)?;
                let target = folder.join("complete.bin");
                crate::backend::write_new_atomic(&target, b"complete source fixture")?;
                (backend.emit)(
                    json!({"kind":"export-job","folder":folder,"ids":[1,2,3],"mode":"original"}),
                );
                (backend.emit)(
                    json!({"kind":"export-item","value":{"id":1,"name":"complete.bin","target":target}}),
                );
            }
            std::thread::sleep(Duration::from_secs(3));
            Ok(json!(true))
        } else {
            backend.dispatch(channel, args)
        };
        let reply = match result {
            Ok(value) => json!({"kind":"reply","id":command["id"],"ok":true,"value":value}),
            Err(error) => {
                json!({"kind":"reply","id":command["id"],"ok":false,"error":format!("{error:#}")})
            }
        };
        let reply_limit = if diagnostic && channel == "test:oversized-reply" {
            1024
        } else {
            MAX_FRAME
        };
        write_reply(&mut *output.lock().unwrap(), &reply, reply_limit)?;
    }
    Ok(())
}
struct Process {
    child: Child,
    input: ChildStdin,
    messages: Option<mpsc::Receiver<Result<Value>>>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Process {
    fn drop(&mut self) {
        self.messages.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
pub struct Session {
    process: Option<Process>,
    emit: Emit,
    cancel: Arc<AtomicBool>,
    closing: Arc<AtomicBool>,
    next: u64,
    game: Option<Value>,
    output: Option<Value>,
    idle: Duration,
    tracked: Option<Value>,
    diagnostic: bool,
}
impl Session {
    pub fn new(emit: Emit, cancel: Arc<AtomicBool>, closing: Arc<AtomicBool>) -> Self {
        Self {
            process: None,
            emit,
            cancel,
            closing,
            next: 0,
            game: None,
            output: None,
            idle: Duration::from_secs(180),
            tracked: None,
            diagnostic: false,
        }
    }
    fn spawn(&mut self) -> Result<()> {
        let mut child = Command::new(std::env::current_exe()?)
            .arg("--asset-worker")
            .args(if self.diagnostic {
                vec!["--worker-diagnostics"]
            } else {
                vec![]
            })
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Could not start the isolated native asset worker")?;
        let input = child.stdin.take().context("Worker input unavailable")?;
        let mut output = child.stdout.take().context("Worker output unavailable")?;
        let (tx, messages) = mpsc::sync_channel(8);
        let reader = std::thread::spawn(move || {
            loop {
                let result = read_frame(&mut output, MAX_FRAME);
                let failed = result.is_err();
                if tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        self.process = Some(Process {
            child,
            input,
            messages: Some(messages),
            reader: Some(reader),
        });
        Ok(())
    }
    fn exchange(&mut self, channel: &str, args: &[Value]) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        let process = self
            .process
            .as_mut()
            .context("Native worker is not running")?;
        write_frame(
            &mut process.input,
            &json!({"id":id,"channel":channel,"args":args}),
        )?;
        let started = Instant::now();
        let mut last = Instant::now();
        let mut cancelled = None;
        loop {
            if (self.cancel.load(Ordering::Relaxed) || self.closing.load(Ordering::Relaxed))
                && cancelled.is_none()
            {
                write_frame(&mut process.input, &json!({"channel":"cancel"}))?;
                cancelled = Some(Instant::now());
            }
            ensure!(
                cancelled.is_none_or(|t: Instant| t.elapsed() < Duration::from_secs(2)),
                "Cancelled native worker stopped; reopen any preview/inspection session"
            );
            ensure!(
                last.elapsed() < self.idle && started.elapsed() < Duration::from_secs(4 * 60 * 60),
                "Native worker stopped after its watchdog timeout; app and published exports remain available"
            );
            match process
                .messages
                .as_ref()
                .context("Worker reader unavailable")?
                .recv_timeout(Duration::from_millis(100))
            {
                Ok(result) => {
                    let message = result.context(
                        "Native worker exited unexpectedly; reopen any preview/inspection session",
                    )?;
                    last = Instant::now();
                    if message["kind"] == "event" {
                        (self.emit)(message);
                        continue;
                    }
                    if message["kind"] == "export-job" {
                        self.tracked = Some(
                            json!({"folder":message["folder"],"ids":message["ids"],"mode":message["mode"],"results":[],"failures":[]}),
                        );
                        continue;
                    }
                    if message["kind"] == "export-item" || message["kind"] == "export-failure" {
                        if let Some(job) = self.tracked.as_mut() {
                            job[if message["kind"] == "export-item" {
                                "results"
                            } else {
                                "failures"
                            }]
                            .as_array_mut()
                            .unwrap()
                            .push(message["value"].clone());
                        }
                        continue;
                    }
                    ensure!(
                        message["kind"] == "reply" && message["id"] == id,
                        "Unexpected native worker reply"
                    );
                    return Ok(message);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(e) => return Err(e.into()),
            }
        }
    }
    pub fn dispatch(&mut self, channel: &str, args: &[Value]) -> Result<Value> {
        if matches!(channel, "mod:compare" | "mod:export") {
            bail!("Reference-tool commands are not supported by the native exporter");
        }
        self.native_dispatch(channel, args)
    }
    fn native_dispatch(&mut self, channel: &str, args: &[Value]) -> Result<Value> {
        ensure!(
            !self.closing.load(Ordering::Relaxed),
            "Exporter is shutting down"
        );
        self.cancel.store(false, Ordering::Relaxed);
        if self.process.is_none() {
            self.spawn()?;
            // Restart catalog/settings only. Never replay a failed export or Save.
            for (command, setting) in [
                ("game:load", self.game.clone()),
                ("export:set-root", self.output.clone()),
            ] {
                if let Some(setting) = setting {
                    let reply = self.exchange(command, &[setting]);
                    if !reply.as_ref().is_ok_and(|r| r["ok"] == true) {
                        self.process.take();
                        bail!(
                            "Could not restore native worker catalog/settings; re-index the game"
                        );
                    }
                }
            }
        }
        self.tracked = None;
        let mut reply = match self.exchange(channel, args) {
            Ok(reply) => reply,
            Err(e) => {
                self.process.take();
                if let Some(mut job) = self.tracked.take() {
                    let pending: Vec<_> = job["ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|id| {
                            !job["results"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .chain(job["failures"].as_array().unwrap())
                                .any(|r| &r["id"] == *id)
                        })
                        .cloned()
                        .collect();
                    if let Some(id) = pending.first() {
                        job["failures"]
                            .as_array_mut()
                            .unwrap()
                            .push(json!({"id":id,"error":format!("{e:#}")}));
                    }
                    job["pending"] = json!(pending.into_iter().skip(1).collect::<Vec<_>>());
                    job["cancelled"] = json!(self.cancel.load(Ordering::Relaxed));
                    job["notice"] = json!(
                        "Native worker interrupted. Successfully published files remain unchanged; retry failed/unfinished resources. Reopen preview/inspection sessions after recovery."
                    );
                    return Ok(job);
                }
                return Err(e);
            }
        };
        ensure!(
            reply["ok"] == true,
            "{}",
            reply["error"]
                .as_str()
                .unwrap_or("Native worker request failed")
        );
        if channel == "game:load" {
            self.game = args.first().cloned();
        }
        if channel == "export:set-root" {
            self.output = args.first().cloned();
        }
        Ok(reply["value"].take())
    }
    pub fn stop(&mut self) {
        self.process.take();
    }
}
pub fn verify(root: &str, path: &str) -> Result<()> {
    let path = std::path::PathBuf::from(path);
    let parent = path.parent().context("Report parent required")?;
    std::fs::create_dir_all(parent)?;
    let cancel = Arc::new(AtomicBool::new(false));
    let mut session = Session::new(
        Arc::new(|_| {}),
        cancel.clone(),
        Arc::new(AtomicBool::new(false)),
    );
    session.diagnostic = true;
    session.dispatch("app:bootstrap", &[])?;
    session.dispatch("game:load", &[json!(root)])?;
    session.dispatch("export:set-root", &[json!(parent.canonicalize()?)])?;
    let original_pid = session.process.as_ref().unwrap().child.id();
    let oversized = session.dispatch("test:oversized-reply", &[]).unwrap_err();
    ensure!(
        format!("{oversized:#}").contains("IPC limit"),
        "Missing oversized reply error"
    );
    session.dispatch("cache:status", &[])?;
    ensure!(
        session.process.as_ref().unwrap().child.id() == original_pid,
        "Oversized reply restarted a healthy worker"
    );
    session.idle = Duration::from_millis(500);
    let start = Instant::now();
    let interrupted = session.dispatch("test:partial-hang", &[])?;
    ensure!(
        interrupted["results"].as_array().unwrap().len() == 1
            && interrupted["failures"][0]["id"] == 2
            && interrupted["pending"] == json!([3]),
        "Interrupted job lost results/retry IDs"
    );
    ensure!(
        std::fs::read(interrupted["results"][0]["target"].as_str().unwrap())?
            == b"complete source fixture",
        "Worker recovery changed saved output"
    );
    let watchdog_ms = start.elapsed().as_secs_f64() * 1000.;
    session.idle = Duration::from_secs(5);
    let restored = session.dispatch(
        "catalog:search",
        &[json!({"query":"ball_gameplay_official_country_flag_china","limit":10})],
    )?;
    ensure!(
        restored["total"].as_u64().unwrap_or(0) > 0,
        "Catalog not restored after watchdog"
    );
    // Exercise real export/publication through the same process protocol after
    // restart, not only synthetic fault messages or renderer responses.
    let resource = session.dispatch(
        "catalog:resolve-resource",
        &[json!("balls/ball_gameplay_official_country_flag_china.iff")],
    )?;
    let exported = session.dispatch(
        "export:start",
        &[json!({"ids":[resource["id"]],"mode":"mod-ready"})],
    )?;
    ensure!(
        exported["results"].as_array().is_some_and(|r| r.len() == 1)
            && exported["failures"].as_array().is_some_and(Vec::is_empty)
            && exported["results"][0]["validation"]["status"] == "ready",
        "Real worker export failed after restart"
    );
    let gallery = session.dispatch("preview:textures", &[resource["id"].clone()])?;
    let first = &gallery["items"][0];
    let mut save = session.dispatch(
        "preview:save-texture",
        &[resource["id"].clone(), first["name"].clone()],
    )?;
    let texture_target = parent.join("worker-source-texture.dds");
    save["target"] = json!(texture_target);
    session.dispatch("preview:publish-texture", &[save])?;
    use base64::Engine;
    ensure!(
        std::fs::read(&texture_target)?
            == base64::engine::general_purpose::STANDARD
                .decode(first["data"].as_str().context("Gallery bytes missing")?)?,
        "Worker texture Save changed source bytes"
    );
    session.process.as_mut().unwrap().child.kill()?;
    ensure!(
        session.dispatch("cache:status", &[]).is_err(),
        "Killed worker error not surfaced"
    );
    session.dispatch("catalog:search", &[json!({"query":"png04090","limit":10})])?;
    let signal = cancel.clone();
    let cancellation = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        signal.store(true, Ordering::Relaxed);
    });
    let start = Instant::now();
    ensure!(
        session.dispatch("test:hang", &[]).is_err(),
        "Stuck worker did not cancel"
    );
    cancellation.join().unwrap();
    let cancel_ms = start.elapsed().as_secs_f64() * 1000.;
    ensure!(
        cancel_ms < 2800.,
        "Stuck-worker cancellation was not bounded"
    );
    let again = session.dispatch("cache:status", &[])?;
    ensure!(again["files"] == 0, "Worker restored an asset disk cache");
    session.stop();
    std::fs::write(
        path,
        serde_json::to_vec_pretty(
            &json!({"pass":true,"watchdogMs":watchdog_ms,"cancelMs":cancel_ms,"oversizedReplyPreservesWorker":true,"restartRestoresCatalog":true,"partialResultsRetained":true,"completedBytesUnchanged":true,"realExportAfterRecovery":true,"workerTextureSaveByteExact":true,"noAssetDiskCache":true}),
        )?,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_reply_is_a_request_error_and_next_frame_still_works() {
        let mut bytes = Vec::new();
        write_reply(
            &mut bytes,
            &json!({"kind":"reply","id":7,"ok":true,"value":"x".repeat(1024)}),
            256,
        )
        .unwrap();
        write_reply(
            &mut bytes,
            &json!({"kind":"reply","id":8,"ok":true,"value":true}),
            256,
        )
        .unwrap();
        let mut reader = std::io::Cursor::new(bytes);
        let first = read_frame(&mut reader, 1024).unwrap();
        assert_eq!(first["id"], 7);
        assert_eq!(first["ok"], false);
        assert!(first["error"].as_str().unwrap().contains("IPC limit"));
        assert_eq!(read_frame(&mut reader, 1024).unwrap()["id"], 8);
    }
    #[test]
    fn request_envelope_has_headroom_above_host_argument_limit() {
        let args = json!(["x".repeat(MAX_REQUEST_ARGS - 4)]);
        assert_eq!(serde_json::to_vec(&args).unwrap().len(), MAX_REQUEST_ARGS);
        let frame = json!({"id":u64::MAX,"channel":"x".repeat(64),"args":args});
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &frame).unwrap();
        assert_eq!(
            read_frame(&mut std::io::Cursor::new(bytes), MAX_REQUEST_FRAME).unwrap(),
            frame
        );
    }
    #[test]
    fn reference_tool_commands_are_rejected_before_worker_or_game_access() {
        let mut session = Session::new(
            Arc::new(|_| {}),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        );
        for channel in ["mod:compare", "mod:export"] {
            assert!(
                session
                    .dispatch(channel, &[json!({"id":0,"consent":true})])
                    .is_err()
            );
            assert!(session.process.is_none());
        }
    }
    #[test]
    fn framed_messages_round_trip_and_reject_truncation() {
        let value = json!({"path":"char/sig/player.iff","n":100,"ok":true});
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &value).unwrap();
        assert_eq!(
            read_frame(&mut std::io::Cursor::new(&bytes), 1024).unwrap(),
            value
        );
        bytes.pop();
        assert!(read_frame(&mut std::io::Cursor::new(&bytes), 1024).is_err());
        assert!(read_frame(&mut std::io::Cursor::new(u32::MAX.to_le_bytes()), 1024).is_err());
    }
}
