use super::{cache, no_reparse, preparation_lock};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct SettingsWriteGuard {
    _lock: File,
}

pub fn settings_write_guard(data: &Path) -> Result<SettingsWriteGuard> {
    let path = data.join(".runtime-settings.lock");
    no_reparse(&path)?;
    let started = Instant::now();
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(&path)
        {
            Ok(file) => return Ok(SettingsWriteGuard { _lock: file }),
            Err(error)
                if error.raw_os_error() == Some(32)
                    && started.elapsed() < Duration::from_secs(30) =>
            {
                std::thread::sleep(Duration::from_millis(50))
            }
            Err(error) => {
                return Err(error).context(
                    "Runtime maintenance is protecting settings; try saving again shortly",
                );
            }
        }
    }
}

struct ProtectedSettings {
    _settings: File,
    _write: SettingsWriteGuard,
}

fn protect_settings(data: &Path) -> Result<Option<ProtectedSettings>> {
    let write = settings_write_guard(data)?;
    let path = data.join("settings.json");
    no_reparse(&path)?;
    if !path.is_file() {
        return Ok(None);
    }
    let settings = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(&path)?;
    let metadata = settings.metadata()?;
    ensure!(
        metadata.is_file()
            && metadata.file_attributes() & 0x400 == 0
            && metadata.len() <= 4 * 1024 * 1024,
        "Settings cannot be safely protected for runtime cleanup"
    );
    let document: Value = serde_json::from_reader(&settings)?;
    ensure!(
        document.is_object(),
        "Invalid settings document; runtime caches were preserved"
    );
    Ok(Some(ProtectedSettings {
        _settings: settings,
        _write: write,
    }))
}

pub(super) fn retry_scratch(
    lock: &super::PreparationLock,
    data: &Path,
    progress: &mut impl FnMut(&str),
) {
    match protect_settings(data) {
        Ok(Some(_settings)) => cache::retry_scratch(lock, &data.join("settings.json"), progress),
        Ok(None) => {
            progress("Runtime scratch cleanup deferred until initial settings are available.")
        }
        Err(error) => progress(&format!(
            "Preserved runtime scratch; settings protection unavailable: {error:#}"
        )),
    }
}

struct State {
    status: &'static str,
    started: Instant,
    milliseconds: Option<u128>,
    warnings: bool,
    history: VecDeque<String>,
    pending: VecDeque<String>,
}

#[derive(Clone)]
pub(super) struct Maintenance(Arc<Mutex<State>>);
impl Maintenance {
    fn message(&self, message: &str) {
        let message: String = message.chars().take(4096).collect();
        let mut state = self.0.lock().unwrap();
        state.warnings |= message.starts_with("Preserved");
        if state.history.len() == 16 {
            state.history.pop_front();
        }
        state.history.push_back(message.clone());
        if state.pending.len() == 16 {
            state.pending.pop_front();
        }
        state.pending.push_back(message);
    }
    fn finish(&self, status: &'static str) {
        let mut state = self.0.lock().unwrap();
        state.status = status;
        state.milliseconds = Some(state.started.elapsed().as_millis());
    }
    pub(super) fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().pending.drain(..).collect()
    }
    pub(super) fn report(&self) -> Value {
        let state = self.0.lock().unwrap();
        json!({"status":state.status,"milliseconds":state.milliseconds.unwrap_or_else(|| state.started.elapsed().as_millis()),"warnings":state.warnings,"messages":state.history})
    }
}

pub(super) fn start(data: PathBuf, hash: String, lease: File) -> Maintenance {
    let task = Maintenance(Arc::new(Mutex::new(State {
        status: "waiting",
        started: Instant::now(),
        milliseconds: None,
        warnings: false,
        history: VecDeque::new(),
        pending: VecDeque::new(),
    })));
    let worker = task.clone();
    let result = std::thread::Builder::new()
        .name("cmt-runtime-maintenance".into())
        .spawn(move || {
            // This independent lease keeps the runtime live even if its caller is dropped.
            let result = (|| -> Result<()> {
                let lock = preparation_lock(&data.join("Runtime"))?;
                let _runtime_lease = lease;
                worker.0.lock().unwrap().status = "waiting-for-settings";
                let settings_path = data.join("settings.json");
                let Some(_settings) = protect_settings(&data)? else {
                    worker
                        .message("Runtime cleanup deferred until initial settings are available.");
                    worker.finish("deferred");
                    return Ok(());
                };
                worker.0.lock().unwrap().status = "running";
                cache::retry_scratch(&lock, &settings_path, &mut |message| {
                    worker.message(message)
                });
                let removed = cache::prune(&lock.cache, &hash, &settings_path)?;
                if removed > 0 {
                    worker.message(&format!("Removed {removed} unused runtime cache(s)."));
                }
                worker.finish("complete");
                Ok(())
            })();
            if let Err(error) = result {
                worker.message(&format!(
                    "Preserved runtime caches; maintenance deferred: {error:#}"
                ));
                worker.finish("deferred");
            }
        });
    if let Err(error) = result {
        task.message(&format!(
            "Preserved runtime caches; background maintenance could not start: {error}"
        ));
        task.finish("deferred");
    }
    task
}

#[cfg(test)]
#[path = "../tests/runtime-maintenance.inc"]
mod tests;
