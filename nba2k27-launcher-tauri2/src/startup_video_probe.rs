//! Dev-only, isolated smoke fixture for the actual startup route, ACL and hidden
//! media readiness handshake. No game process, audio guardian or fullscreen.
use crate::startup_video_media::{CatalogEntry, VideoLease};
use crate::startup_video_tauri::{ProgressStage, Request, build_intro_window};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager, WebviewWindow};
struct Probe {
    tx: Mutex<mpsc::Sender<Request>>,
    launch_queries: std::sync::atomic::AtomicUsize,
}
#[tauri::command]
fn startup_intro_event(
    window: WebviewWindow,
    state: tauri::State<'_, Probe>,
    request: Request,
) -> std::result::Result<Value, String> {
    if window.label() != "startup-intro"
        || window
            .url()
            .map(|u| u.as_str() != "https://tauri.localhost/index.html?startupIntro=1")
            .unwrap_or(true)
    {
        return Err("Invalid fixture window".into());
    }
    if let Request::LaunchStatus { generation: 1 } = &request {
        let count = state
            .launch_queries
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Synthetic native-stage responses exercise the real narrow IPC route.
        return Ok(if count == 0 {
            json!({"phase":"waiting-for-game","providerPhase":"preparing-jukebox","failed":false})
        } else {
            json!({"phase":"game-running","providerPhase":"loader-active","failed":false})
        });
    }
    state
        .tx
        .lock()
        .map_err(|e| e.to_string())?
        .send(request)
        .map_err(|e| e.to_string())?;
    Ok(json!({"accepted":true}))
}
pub(crate) fn run(root: PathBuf) -> Result<()> {
    std::fs::create_dir_all(root.join("Logs"))?;
    let doc: Value =
        serde_json::from_slice(&std::fs::read(root.join("Data/Rust/startup-videos.json"))?)?;
    let item = doc["items"]
        .as_array()
        .context("Fixture video catalog missing")?
        .iter()
        .find(|v| v["id"] == doc["selected"])
        .context("Fixture selection missing")?;
    let text = |key: &str| -> Result<String> {
        Ok(item[key]
            .as_str()
            .context("Invalid fixture catalog")?
            .into())
    };
    let media = VideoLease::from_trusted_catalog(
        &root,
        CatalogEntry {
            id: text("id")?,
            name: text("name")?,
            relative_path: PathBuf::from(text("relativePath")?),
            sha256: text("sha256")?,
            bytes: item["bytes"].as_u64().context("Invalid fixture bytes")?,
        },
        || false,
    )?;
    let (tx, rx) = mpsc::channel();
    let bootstrap=json!({"generation":1,"source":media.path(),"title":media.name,"volume":0.0,"muted":true,"audioStatus":"Silent isolated startup route check"}).to_string();
    tauri::Builder::default().manage(Probe{tx:Mutex::new(tx),launch_queries:std::sync::atomic::AtomicUsize::new(0)})
        .invoke_handler(tauri::generate_handler![startup_intro_event])
        .setup(move|app|{
            app.asset_protocol_scope().allow_file(media.path())?;
            let handle=app.handle().clone();
            // Production also builds this window on its background owner thread.
            std::thread::spawn(move||{
                let started=Instant::now();let mut events=Vec::new();
                let result=(||->Result<()>{
                    let (window,_pins)=build_intro_window(&handle,&root,&bootstrap)?;
                    window.set_size(tauri::PhysicalSize::new(1280,800))?;
                    events.push(json!({"event":"hidden-window-created","ms":started.elapsed().as_millis()}));
                    let mut prepared=false;let mut frame_seen=false;let mut stages=Vec::new();
                    while started.elapsed()<Duration::from_secs(25) {
                        let Ok(request)=rx.recv_timeout(Duration::from_millis(100)) else {continue};
                        events.push(json!({"event":request,"ms":started.elapsed().as_millis()}));
                        match request {
                            Request::Prepared{generation:1} if !prepared=>{
                                prepared=true;window.show()?;
                                window.emit("startup-intro-control",json!({"generation":1,"sequence":1,"op":"play","volume":null}))?;
                            },
                            // Exiting the last window here can end Tauri before
                            // this worker saves its report. Exit after saving.
                            Request::Progress{generation:1,stage:ProgressStage::Frame,detail}=>{
                                ensure!(prepared,"Frame before prepared");
                                let layout:Value=serde_json::from_str(&detail).context("Missing frame layout telemetry")?;
                                let number=|key:&str,i:usize|->Result<f64>{layout[key][i].as_f64().context("Missing layout dimension")};
                                let width=number("viewport",0)?;let height=number("viewport",1)?;
                                ensure!(width>450.0 && height>450.0,"Fixture must exceed the old 450px video cap");
                                ensure!(number("rect",0)?.abs()<1.0 && number("rect",1)?.abs()<1.0
                                    && (number("rect",2)?-width).abs()<1.0 && (number("rect",3)?-height).abs()<1.0,
                                    "Startup video does not fill its viewport: {detail}");
                                ensure!(layout["fit"]=="contain","Video must preserve its aspect ratio");
                                let mx=number("meter",0)?;let my=number("meter",1)?;
                                ensure!(mx>=0.0 && mx<=40.0 && number("meter",2)?<=200.0 && my>height-100.0,"Progress line is not small and bottom-left: {detail}");
                                ensure!(layout["green"].as_str().is_some_and(|s|s.contains("24, 143, 89") && s.contains("69, 227, 157")),"Progress line lost the launcher's green gradient");
                                frame_seen=true;
                            },
                            Request::Progress{generation:1,stage:ProgressStage::Launch,detail}=>{
                                stages.push(detail);
                            },
                            Request::Finish{reason,..}=>anyhow::bail!("Intro finished before a decoded frame: {reason}"),
                            _=>{}
                        }
                        if frame_seen && stages.iter().any(|s|s=="Preparing playlist audio") && stages.iter().any(|s|s=="Session ready") {return Ok(());}
                    }
                    anyhow::bail!("Actual intro route did not produce a decoded frame within 25 seconds")
                })();
                let pass=result.is_ok();
                let saved=crate::backend::atomic_json(&root.join("Logs/startup-intro-probe.json"),&json!({"pass":pass,"events":events,"error":result.err().map(|e|format!("{e:#}")),"gameAccess":false,"audioOutput":false,"guardianStarted":false,"fullscreen":false}));
                if let Err(error)=&saved { eprintln!("Cannot save intro probe result: {error:#}"); }
                drop(media);handle.exit(if pass && saved.is_ok(){0}else{1});
            });
            Ok(())
        }).build(tauri::generate_context!())?.run(|_,_|{});
    Ok(())
}
