//! Isolated live ratings sessions. The remote page has no launcher IPC, host
//! objects, supplied scripts, file paths, or game capabilities.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::Url;
#[path = "online_ratings_native.rs"]
mod native;
pub(crate) const MAX_TEXT: usize = 4_000_000;
const LIFETIME: Duration = Duration::from_secs(120);
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Address {
    pub url: String,
    pub host: String,
}
impl Address {
    pub(crate) fn parse(input: &str) -> Result<Self> {
        ensure!(
            input.len() <= 2048 && !input.chars().any(char::is_control),
            "Ratings URL is too long or contains controls"
        );
        let u = Url::parse(input).context("Paste a 2KDB or 2KAuction player link")?;
        ensure!(
            u.scheme() == "https"
                && u.port().is_none()
                && u.username().is_empty()
                && u.password().is_none()
                && u.fragment().is_none(),
            "Use HTTPS without credentials, ports or fragments"
        );
        let host = u.host_str().context("Ratings host missing")?;
        let authority = input
            .split_once("://")
            .map(|(_, s)| s.split('/').next().unwrap_or(""))
            .unwrap_or("");
        ensure!(
            !authority.contains(':') && !authority.contains('@'),
            "Explicit URL ports/credentials are not accepted"
        );
        if host == "2kdb.net" {
            ensure!(
                regex::Regex::new(r"^/player/[1-9][0-9]{0,11}$")?.is_match(u.path())
                    && u.query().is_none(),
                "Use a 2KDB /player/number URL without query parameters"
            );
        } else {
            ensure!(host == "2kauction.com", "Unsupported ratings host");
            let re = regex::Regex::new(
                r"^/mynba/(?:(2k[0-9]{2})/)?players/[a-z0-9]+(?:-[a-z0-9]+)*/?$",
            )?;
            let caps = re
                .captures(u.path())
                .context("Use a 2KAuction MyNBA player URL")?;
            let mut seen = std::collections::BTreeSet::new();
            for (k, v) in u.query_pairs() {
                ensure!(
                    seen.insert(k.to_string()),
                    "Ambiguous game/roster selection"
                );
                let valid = match k.as_ref() {
                    "game" => regex::Regex::new(r"^2k[0-9]{2}$")?.is_match(&v),
                    "roster" => regex::Regex::new(r"^[a-z0-9_-]{1,80}$")?.is_match(&v),
                    "version" => regex::Regex::new(r"^[0-9]{1,10}$")?.is_match(&v),
                    _ => false,
                };
                ensure!(valid, "Unsupported game/roster selection");
                if k == "game" {
                    if let Some(path) = caps.get(1) {
                        ensure!(path.as_str() == v, "Conflicting game selections");
                    }
                }
            }
        }
        Ok(Self {
            url: u.to_string(),
            host: host.to_owned(),
        })
    }
    pub(crate) fn resource(&self, input: &str) -> bool {
        if input.len() > 8192 || input.chars().any(char::is_control) {
            return false;
        }
        let Ok(u) = Url::parse(input) else {
            return false;
        };
        if matches!(u.scheme(), "data" | "blob") {
            return true;
        }
        u.scheme() == "https"
            && u.port().is_none()
            && u.username().is_empty()
            && u.password().is_none()
            && u.host_str().is_some_and(|h| {
                h == self.host
                    || h == "challenges.cloudflare.com"
                    || (self.host == "2kdb.net" && h == "img.2kdb.net")
            })
    }
    pub(crate) fn navigation(&self, input: &str) -> bool {
        if input.len() > 4096 {
            return false;
        }
        let Ok(u) = Url::parse(input) else {
            return false;
        };
        u.scheme() == "https"
            && u.port().is_none()
            && u.username().is_empty()
            && u.password().is_none()
            && u.host_str() == Some(self.host.as_str())
    }
    pub(crate) fn is_source(&self, input: &str) -> bool {
        Self::parse(input).is_ok_and(|a| a == *self)
    }
}
fn destination(v: &Value) -> Result<String> {
    let object = v.as_object().context("Destination identity missing")?;
    ensure!(object.len() == 4, "Unexpected destination identity fields");
    ensure!(
        v["recordIndex"].as_u64().is_some_and(|n| n <= 8191)
            && v["cyberfaceId"].as_u64().is_some_and(|n| n <= 65535),
        "Invalid destination player identity"
    );
    for key in ["firstName", "lastName"] {
        ensure!(
            v[key].as_str().is_some_and(
                |s| s.encode_utf16().count() <= 100 && !s.chars().any(char::is_control)
            ),
            "Invalid destination player name"
        );
    }
    Ok(serde_json::to_string(v)?)
}
struct Active {
    id: String,
    tab: String,
    destination: String,
    address: Address,
    started: Instant,
    samples: AtomicUsize,
    worker: native::Worker,
}
#[derive(Default)]
pub(crate) struct Service {
    active: Mutex<Option<Arc<Active>>>,
    operation: Mutex<()>,
    stop: Arc<AtomicBool>,
}
impl Service {
    pub(crate) fn new() -> Self {
        Self::default()
    }
    pub(crate) fn cancel(&self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut active) = self.active.lock() {
            if let Some(a) = active.take() {
                a.worker.cancel();
            }
        }
    }
    /// owner is obtained natively from the main launcher window, never RPC input.
    /// tab is the already-authenticated roster-page identity from browser_workspace.
    pub(crate) fn request(
        &self,
        root: &Path,
        owner: usize,
        tab: &str,
        input: &Value,
    ) -> Result<Value> {
        ensure!(
            !self.stop.load(Ordering::Acquire),
            "Ratings capture is closing"
        );
        ensure!(
            tab.len() == 36
                && tab
                    .bytes()
                    .enumerate()
                    .all(|(i, c)| if [8, 13, 18, 23].contains(&i) {
                        c == b'-'
                    } else {
                        c.is_ascii_hexdigit()
                    }),
            "Open the roster editor from the launcher"
        );
        let dest = destination(&input["destination"])?;
        let action = input["action"]
            .as_str()
            .context("Ratings capture action missing")?;
        let _op =
            if action == "cancel" {
                None
            } else {
                Some(self.operation.try_lock().map_err(|_| {
                    anyhow::anyhow!("A ratings capture operation is already running")
                })?)
            };
        if action == "start" {
            ensure!(
                input
                    .as_object()
                    .is_some_and(|o| o.len() == 3 && o.contains_key("url")),
                "Ratings start accepts only URL and destination"
            );
            let address = Address::parse(input["url"].as_str().context("Player URL missing")?)?;
            let mut active = self
                .active
                .lock()
                .map_err(|_| anyhow::anyhow!("Ratings ownership unavailable"))?;
            if let Some(previous) = active.as_ref() {
                if previous.started.elapsed() >= LIFETIME {
                    previous.worker.cancel();
                }
            }
            if active.as_ref().is_some_and(|a| a.worker.ended()) {
                active.take();
            }
            ensure!(active.is_none(), "A ratings import is already open");
            ensure!(
                native::valid_owner(owner),
                "Capture owner is not this launcher window"
            );
            let id = crate::backend::unique_id();
            let directory = profile_directory(root, &id)?;
            let worker =
                native::Worker::spawn(owner, address.clone(), directory, self.stop.clone())?;
            let a = Arc::new(Active {
                id: id.clone(),
                tab: tab.to_owned(),
                destination: dest,
                address: address.clone(),
                started: Instant::now(),
                samples: AtomicUsize::new(0),
                worker,
            });
            *active = Some(a);
            return Ok(
                json!({"id":id,"url":address.url,"expiresInMs":120000,"isolation":"native-webview2-no-launcher-bridge"}),
            );
        }
        ensure!(
            matches!(action, "sample" | "cancel")
                && input
                    .as_object()
                    .is_some_and(|o| o.len() == 3 && o.contains_key("id")),
            "Unsupported ratings capture action/options"
        );
        let a = self
            .active
            .lock()
            .map_err(|_| anyhow::anyhow!("Ratings ownership unavailable"))?
            .as_ref()
            .context("Ratings capture expired or closed")?
            .clone();
        ensure!(
            input["id"].as_str() == Some(a.id.as_str()) && a.tab == tab && a.destination == dest,
            "Ratings capture belongs to another editor visit"
        );
        if action == "cancel" {
            let closed = a.worker.close();
            if closed {
                let mut current = self
                    .active
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Ratings ownership unavailable"))?;
                if current
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &a))
                {
                    current.take();
                }
            }
            return Ok(json!({"canceled":true,"closed":closed}));
        }
        ensure!(
            a.started.elapsed() < LIFETIME && !a.worker.ended(),
            "Ratings capture closed or timed out. No player values were changed"
        );
        ensure!(
            a.samples.fetch_add(1, Ordering::AcqRel) < 80,
            "Ratings capture sample limit reached"
        );
        let sample = a.worker.sample()?;
        ensure!(
            !self.stop.load(Ordering::Acquire) && a.started.elapsed() < LIFETIME,
            "Ratings capture was canceled or expired"
        );
        ensure!(
            self.active
                .lock()
                .map_err(|_| anyhow::anyhow!("Ratings ownership unavailable"))?
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &a)),
            "Ratings capture was replaced"
        );
        Ok(json!({"id":a.id,"url":a.address.url,"sample":sample}))
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(super) struct Profile {
    pub path: PathBuf,
    _pins: Vec<crate::loader_files::Directories>,
}
fn profile_directory(root: &Path, id: &str) -> Result<Profile> {
    ensure!(
        id.len() <= 128 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "Invalid native capture identity"
    );
    let mut path = root.to_owned();
    let mut pins = vec![crate::loader_files::Directories::pin(&path)?];
    for part in ["Cache", "Online Ratings"] {
        path.push(part);
        match std::fs::create_dir(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
        pins.push(crate::loader_files::Directories::pin(&path)?);
    }
    path.push(id);
    std::fs::create_dir(&path)?;
    pins.push(crate::loader_files::Directories::pin(&path)?);
    Ok(Profile { path, _pins: pins })
}
#[cfg(test)]
#[path = "online_ratings_tests.rs"]
mod tests;
