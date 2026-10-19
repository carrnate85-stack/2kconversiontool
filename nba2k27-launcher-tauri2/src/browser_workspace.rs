//! Original WPF browser pages, served by Rust. Normal editor sessions can use
//! a narrow read-only roster service. Sample sessions have no such capability.
use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

const HEADER_LIMIT: usize = 16 * 1024;
const BODY_LIMIT: usize = 2 * 1024 * 1024;
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data: blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";
const UNPORTED: &str = "This live roster/team operation is not ported in this Rust QA build. No game read or write was attempted by this request. Use the current WPF launcher for live editing.";
const PREVIEW_JS: &str = r#"(() => {
  const kind=new URLSearchParams(location.search).get('preview');
  if(!['player','team'].includes(kind))return;
  let attempts=0;const timer=setInterval(()=>{
    if(++attempts>100){clearInterval(timer);return;}
    if(document.getElementById('appStartup')?.classList.contains('hidden')&&typeof previewPlayerEditorLayout==='function'){
      clearInterval(timer);
      if(kind==='player')previewPlayerEditorLayout();
      else {
        openTeamEditor({source:'layout-preview',name:'76ers',cityName:'Philadelphia',cityShort:'PHI',stateShort:'PA',teamId:0,recordIndex:0,rosterTypeId:0});
        document.getElementById('teamEditBoundary').textContent='Layout preview Ã‚Â· Sample data only. Nothing is read from or saved to the game.';
      }
    }
  },50);
})();"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    path: String,
    mime: String,
    bytes: Vec<u8>,
    sha256: String,
}
type Assets = BTreeMap<String, Asset>;
type CatalogService = Arc<dyn Fn(&Value) -> Result<Value> + Send + Sync>;
type LauncherService = Arc<dyn Fn() -> Result<()> + Send + Sync>;
type RosterService = Arc<dyn Fn(&str, &str) -> Result<Value> + Send + Sync>;
type PlayerSaveService = Arc<dyn Fn(&str, &Value) -> Result<Value> + Send + Sync>;
type PlayerDnaService = Arc<dyn Fn(&str, &Value) -> Result<Value> + Send + Sync>;
type TeamDnaService = Arc<dyn Fn(&str, &Value) -> Result<Value> + Send + Sync>;
type OnlineRatingsService = Arc<dyn Fn(&str, &Value) -> Result<Value> + Send + Sync>;
type ShutdownService = Arc<dyn Fn() + Send + Sync>;
type PresetService = Arc<dyn Fn(&Value) -> Result<Value> + Send + Sync>;
/// (method, tab, argument) for WPF editor services: teamStaff, saveTeamStaff,
/// playerSeasons and basketballReferenceStats.
type EditorService = Arc<dyn Fn(&str, &str, &Value) -> Result<Value> + Send + Sync>;
/// WPF animation:* IPC (player-animation-preview.js), owner = X-Animation-Owner.
type AnimationService = Arc<crate::animation_preview::Service>;
#[derive(Clone, Default)]
struct Services {
    animation: Option<AnimationService>,
    catalog: Option<CatalogService>,
    launcher: Option<LauncherService>,
    roster: Option<RosterService>,
    editor: Option<EditorService>,
    save_player: Option<PlayerSaveService>,
    save_team: Option<PlayerSaveService>,
    save_uniform: Option<PlayerSaveService>,
    player_dna: Option<PlayerDnaService>,
    team_dna: Option<TeamDnaService>,
    presets: Option<PresetService>,
    online_ratings: Option<OnlineRatingsService>,
    online_shutdown: Option<ShutdownService>,
}
fn assets() -> Result<Assets> {
    let decoder =
        flate2::read::GzDecoder::new(&include_bytes!("../assets/wpf-browser.json.gz")[..]);
    let mut bytes = Vec::new();
    decoder.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 32 * 1024 * 1024,
        "Browser asset bundle too large"
    );
    let rows: Vec<Asset> = serde_json::from_slice(&bytes)?;
    ensure!(
        !rows.is_empty() && rows.len() <= 256,
        "Invalid browser asset count"
    );
    let mut result = Assets::new();
    for row in rows {
        ensure!(
            static_path(&row.path) && row.bytes.len() <= 4 * 1024 * 1024,
            "Invalid embedded browser asset"
        );
        ensure!(
            matches!(
                row.mime.as_str(),
                "text/html; charset=utf-8"
                    | "text/css; charset=utf-8"
                    | "text/javascript; charset=utf-8"
                    | "image/png"
            ),
            "Invalid embedded asset MIME"
        );
        ensure!(
            format!("{:x}", Sha256::digest(&row.bytes)) == row.sha256,
            "Embedded browser asset hash mismatch"
        );
        ensure!(
            result.insert(row.path.clone(), row).is_none(),
            "Duplicate browser asset"
        );
    }
    ensure!(
        result.contains_key("/renderer/index.html")
            && result.contains_key("/renderer/roster-browser-bridge.js"),
        "Missing original browser workspace"
    );
    Ok(result)
}
fn static_path(path: &str) -> bool {
    if !path.starts_with('/')
        || path.contains('%')
        || path.contains('\\')
        || path.contains('?')
        || path.contains('#')
    {
        return false;
    }
    let pieces: Vec<_> = path[1..].split('/').collect();
    if pieces.iter().any(|p| {
        p.is_empty()
            || p.starts_with('.')
            || *p == "."
            || *p == ".."
            || !p
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
    }) {
        return false;
    }
    match pieces.as_slice() {
        ["assets", "app-icon.png"] => true,
        ["renderer", name] => {
            name.ends_with(".html") || name.ends_with(".css") || name.ends_with(".js")
        }
        ["shared", name]
        | ["renderer", "animation-preview", name]
        | ["renderer", "animation-preview", "vendor", name] => name.ends_with(".js"),
        _ => false,
    }
}
fn token() -> Result<String> {
    let mut bytes = [0u8; 32];
    unsafe {
        BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()?;
    }
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn token_equal(actual: &str, expected: &str) -> bool {
    if actual.len() != 64
        || expected.len() != 64
        || !actual
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return false;
    }
    actual
        .bytes()
        .zip(expected.bytes())
        .fold(0u8, |d, (a, b)| d | (a ^ b))
        == 0
}

struct Request {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}
fn read_request(stream: &mut TcpStream, stop: &AtomicBool) -> Result<Request> {
    stream.set_read_timeout(Some(Duration::from_millis(50)))?;
    stream.set_write_timeout(Some(Duration::from_millis(200)))?;
    let start = Instant::now();
    let mut bytes = Vec::new();
    let (end, method, target, headers, length) = loop {
        ensure!(
            !stop.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(3),
            "Browser request expired"
        );
        if let Some(end) = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
        {
            ensure!(end <= HEADER_LIMIT, "Headers too large");
            let mut storage = [httparse::EMPTY_HEADER; 48];
            let mut parsed = httparse::Request::new(&mut storage);
            ensure!(
                parsed.parse(&bytes[..end])?.is_complete() && parsed.version == Some(1),
                "Unsupported HTTP request"
            );
            let method = parsed.method.context("Missing method")?.to_owned();
            let target = parsed.path.context("Missing request target")?.to_owned();
            ensure!(
                target.starts_with('/')
                    && !target.starts_with("//")
                    && target.len() <= 2048
                    && !target.contains('#'),
                "Invalid request target"
            );
            let mut headers = HashMap::new();
            for h in parsed.headers.iter() {
                let value = std::str::from_utf8(h.value)?.trim().to_owned();
                ensure!(
                    headers.insert(h.name.to_ascii_lowercase(), value).is_none(),
                    "Duplicate header"
                );
            }
            ensure!(
                !headers.contains_key("transfer-encoding") && !headers.contains_key("expect"),
                "Unsupported HTTP framing"
            );
            let length = match headers.get("content-length") {
                Some(v) => {
                    ensure!(
                        !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()),
                        "Invalid content length"
                    );
                    v.parse::<usize>()?
                }
                None => 0,
            };
            ensure!(length <= BODY_LIMIT, "Body too large");
            if method != "POST" {
                ensure!(length == 0, "Unexpected request body");
            }
            break (end, method, target, headers, length);
        }
        ensure!(bytes.len() < HEADER_LIMIT, "Headers too large");
        read_chunk(stream, &mut bytes)?;
    };
    while bytes.len() < end + length {
        ensure!(
            !stop.load(Ordering::Acquire) && start.elapsed() < Duration::from_secs(3),
            "Browser request expired"
        );
        read_chunk(stream, &mut bytes)?;
    }
    ensure!(
        bytes.len() == end + length,
        "Pipelined/trailing data not supported"
    );
    Ok(Request {
        method,
        target,
        headers,
        body: bytes[end..].to_vec(),
    })
}
fn read_chunk(stream: &mut TcpStream, bytes: &mut Vec<u8>) -> Result<()> {
    let mut buffer = [0u8; 4096];
    match stream.read(&mut buffer) {
        Ok(0) => bail!("Disconnected browser request"),
        Ok(n) => bytes.extend_from_slice(&buffer[..n]),
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) => {}
        Err(e) => return Err(e.into()),
    }
    ensure!(
        bytes.len() <= HEADER_LIMIT + BODY_LIMIT,
        "Browser request too large"
    );
    Ok(())
}
struct Response {
    status: u16,
    mime: String,
    body: Vec<u8>,
    head: bool,
}
fn response(status: u16, body: &str) -> Response {
    Response {
        status,
        mime: "text/plain; charset=utf-8".into(),
        body: body.as_bytes().to_vec(),
        head: false,
    }
}
fn reply(stream: &mut TcpStream, r: Response) -> Result<()> {
    let reason = match r.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Service Unavailable",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: {}\r\n\r\n",
        r.status,
        reason,
        r.mime,
        r.body.len(),
        CSP
    );
    if r.body.len() > 256 * 1024 {
        // Animation payloads reach 32 MiB; loopback still needs more than 200 ms.
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
    }
    stream.write_all(header.as_bytes())?;
    if !r.head {
        stream.write_all(&r.body)?;
    }
    Ok(())
}
fn idle() -> Value {
    json!({"status":"idle","message":UNPORTED,"players":[],"teams":[]})
}
fn rpc(body: &[u8], services: &Services, tab: &str) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Call {
        method: String,
        args: Vec<Value>,
    }
    let call: Call = serde_json::from_slice(body)?;
    ensure!(call.args.len() <= 1, "Unsupported argument count");
    Ok(match call.method.as_str() {
        "ping" if call.args.is_empty() => {
            if let Some(animation) = &services.animation {
                animation.heartbeat(tab);
            }
            json!(true)
        }
        "rosterBootstrap" if call.args.is_empty() => {
            json!({"version":crate::backend::VERSION,"playerResearch":if let Some(service)=&services.roster{service("bootstrap",tab)?}else{idle()}})
        }
        "rosterAutoAttach" | "playerResearchStatus" | "scanPlayerResearch"
            if call.args.is_empty() =>
        {
            if let Some(service) = &services.roster {
                service(&call.method, tab)?
            } else {
                idle()
            }
        }
        "player23Catalog" if call.args.len() == 1 => {
            if call.args[0]["action"].as_str() == Some("import") {
                ensure!(
                    crate::roster_native::valid_tab(tab),
                    "Reopen this editor from the launcher; tab identity is invalid"
                );
            }
            services.catalog.as_ref().context(UNPORTED)?(&call.args[0])?
        }
        "onlineRatings" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.online_ratings.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "savePlayer" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.save_player.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "saveTeam" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.save_team.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "saveTeamUniform" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.save_uniform.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "playerDna" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.player_dna.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "teamDna" if call.args.len() == 1 => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            ensure!(
                matches!(call.args[0]["action"].as_str(), Some("import" | "export")),
                "Unsupported Team DNA file action"
            );
            services.team_dna.as_ref().context(UNPORTED)?(tab, &call.args[0])?
        }
        "teamStaff"
        | "cloneArena"
        | "customArenaProfiles"
        | "saveTeamStaff"
        | "playerSeasons"
        | "basketballReferenceStats"
        | "linkTeamUniform"
        | "playerEditorHeaderLabels"
        | "playerEditorArtwork"
        | "playerEditorBadgeIcons"
        | "teamRotations"
        | "previewPlayerAudio"
            if call.args.len() == 1 =>
        {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.editor.as_ref().context(UNPORTED)?(&call.method, tab, &call.args[0])?
        }
        "exportPlayerResearch" if call.args.is_empty() => {
            ensure!(
                crate::roster_native::valid_tab(tab),
                "Reopen this editor from the launcher; tab identity is invalid"
            );
            services.editor.as_ref().context(UNPORTED)?(&call.method, tab, &Value::Null)?
        }
        "editorPresets" if call.args.len() == 1 => {
            services.presets.as_ref().context(UNPORTED)?(&call.args[0])?
        }
        "openLauncher" if call.args.is_empty() => {
            services
                .launcher
                .as_ref()
                .context("No launcher window is attached to this browser fixture")?()?;
            json!(true)
        }
        "warmAnimationPreview" => match (
            &services.animation,
            call.args.first().and_then(Value::as_bool),
        ) {
            (Some(service), Some(enabled)) => service.warm(tab, enabled)?,
            (None, Some(false)) => json!(false),
            (_, None) => bail!("Invalid animation warmup request."),
            (None, Some(true)) => bail!("{UNPORTED}"),
        },
        "openAnimationPreview" if call.args.is_empty() => {
            services.animation.as_ref().context(UNPORTED)?.open(tab)?
        }
        "animationPreviewCatalog" | "animationPreviewProgress" if call.args.len() == 1 => {
            let service = services.animation.as_ref().context(UNPORTED)?;
            let session = call.args[0]
                .as_str()
                .context("Invalid animation preview session")?;
            if call.method == "animationPreviewCatalog" {
                service.catalog(tab, session)?
            } else {
                service.progress(tab, session)?
            }
        }
        "animationPreviewRequest" if call.args.len() == 1 => services
            .animation
            .as_ref()
            .context(UNPORTED)?
            .request(tab, &call.args[0])?,
        "closeAnimationPreview" => match (
            &services.animation,
            call.args.first().and_then(Value::as_str),
        ) {
            (Some(service), Some(session)) => service.close(tab, session)?,
            // Closing a non-existent preview is harmless; no work was started.
            _ => json!(true),
        },
        _ => bail!("{UNPORTED}"),
    })
}
fn handle(
    req: Request,
    origin: &str,
    token: &str,
    assets: &Assets,
    services: &Services,
) -> Response {
    if req.headers.get("host").map(String::as_str) != Some(&origin[7..])
        || req.headers.get("sec-fetch-site").map(String::as_str) == Some("cross-site")
    {
        return response(403, "Local requests only");
    }
    let path = req.target.split('?').next().unwrap_or("");
    if path == "/rpc" {
        if req.target != "/rpc"
            || req.method != "POST"
            || req.headers.get("origin").map(String::as_str) != Some(origin)
            || req.headers.get("content-type").map(String::as_str) != Some("application/json")
            || !token_equal(
                req.headers
                    .get("x-roster-token")
                    .map(String::as_str)
                    .unwrap_or(""),
                token,
            )
        {
            return response(403, "Open this editor from the launcher");
        }
        let (status, data) = match rpc(
            &req.body,
            services,
            req.headers
                .get("x-animation-owner")
                .map(String::as_str)
                .unwrap_or(""),
        ) {
            Ok(v) => (200, json!({"result":v})),
            Err(e) => (400, json!({"error":e.to_string()})),
        };
        return Response {
            status,
            mime: "application/json".into(),
            body: serde_json::to_vec(&data).unwrap_or_default(),
            head: false,
        };
    }
    // roster-browser-bridge.js animationPreviewFetch: backend-issued payload
    // token, owned by this page's X-Animation-Owner, same roster token.
    if let Some(handle) = path.strip_prefix("/animation-payload/") {
        if req.method != "GET"
            || req.target != path
            || handle.len() != 48
            || !handle
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !token_equal(
                req.headers
                    .get("x-roster-token")
                    .map(String::as_str)
                    .unwrap_or(""),
                token,
            )
        {
            return response(403, "Open this editor from the launcher");
        }
        let owner = req
            .headers
            .get("x-animation-owner")
            .map(String::as_str)
            .unwrap_or("");
        return match services
            .animation
            .as_ref()
            .and_then(|s| s.payload(owner, handle))
        {
            Some(bytes) => Response {
                status: 200,
                mime: "application/json".into(),
                body: bytes.to_vec(),
                head: false,
            },
            None => response(404, "Preview data expired. Retry preview."),
        };
    }
    if !matches!(req.method.as_str(), "GET" | "HEAD") {
        return response(405, "Read-only assets");
    }
    if !static_path(path) {
        return response(404, "Unknown browser asset");
    }
    let (mime, mut body) = if path == "/renderer/rust-browser-adapter.js" {
        (
            "text/javascript; charset=utf-8".to_owned(),
            include_bytes!("../frontend/browser-adapter.js").to_vec(),
        )
    } else if path == "/renderer/rust-layout-preview.js" {
        (
            "text/javascript; charset=utf-8".to_owned(),
            PREVIEW_JS.as_bytes().to_vec(),
        )
    } else if let Some(asset) = assets.get(path) {
        (asset.mime.clone(), asset.bytes.clone())
    } else {
        return response(404, "Unknown browser asset");
    };
    if path == "/renderer/index.html" {
        let html = String::from_utf8_lossy(&body);
        body = html
            .replace(
                "<script src=\"theme.js\">",
                "<script src=\"roster-browser-bridge.js\"></script><script src=\"rust-browser-adapter.js\"></script><script src=\"theme.js\">",
            )
            .replace(
                "</body>",
                "<script src=\"rust-layout-preview.js\"></script></body>",
            )
            .into_bytes();
    }
    Response {
        status: 200,
        mime,
        body,
        head: req.method == "HEAD",
    }
}

pub struct Runtime {
    server: Mutex<[Option<Server>; 2]>,
    services: Services,
}
struct Server {
    origin: String,
    token: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Runtime {
    #[cfg(test)]
    pub fn new() -> Self {
        Self {
            server: Mutex::new([None, None]),
            services: Services::default(),
        }
    }
    pub fn with_services(catalog: CatalogService, launcher: LauncherService) -> Self {
        Self {
            server: Mutex::new([None, None]),
            services: Services {
                animation: None,
                catalog: Some(catalog),
                launcher: Some(launcher),
                roster: None,
                editor: None,
                save_player: None,
                save_team: None,
                save_uniform: None,
                player_dna: None,
                team_dna: None,
                presets: None,
                online_ratings: None,
                online_shutdown: None,
            },
        }
    }
    pub fn with_roster(mut self, service: RosterService) -> Self {
        self.services.roster = Some(service);
        self
    }
    pub fn with_player_save(mut self, service: PlayerSaveService) -> Self {
        self.services.save_player = Some(service);
        self
    }
    pub fn with_team_save(mut self, service: PlayerSaveService) -> Self {
        self.services.save_team = Some(service);
        self
    }
    pub fn with_uniform_save(mut self, service: PlayerSaveService) -> Self {
        self.services.save_uniform = Some(service);
        self
    }
    pub fn with_player_dna(mut self, service: PlayerDnaService) -> Self {
        self.services.player_dna = Some(service);
        self
    }
    pub fn with_team_dna(mut self, service: TeamDnaService) -> Self {
        self.services.team_dna = Some(service);
        self
    }
    pub fn with_editor(mut self, service: EditorService) -> Self {
        self.services.editor = Some(service);
        self
    }
    pub fn with_animation(mut self, service: AnimationService) -> Self {
        self.services.animation = Some(service);
        self
    }
    pub fn with_presets(mut self, service: PresetService) -> Self {
        self.services.presets = Some(service);
        self
    }
    pub fn with_online_ratings(
        mut self,
        service: OnlineRatingsService,
        shutdown: ShutdownService,
    ) -> Self {
        self.services.online_ratings = Some(service);
        self.services.online_shutdown = Some(shutdown);
        self
    }
    pub fn url(&self, section: &str, preview: bool) -> Result<String> {
        ensure!(
            matches!(section, "players" | "teams"),
            "Unsupported editor workspace"
        );
        let mut server = self
            .server
            .lock()
            .map_err(|_| anyhow::anyhow!("Browser workspace lock failed"))?;
        let index = usize::from(preview);
        if server[index].is_none() {
            let mut services = self.services.clone();
            // A sample-layout page has a different port and token and no native
            // roster capability. Its original automatic attachment stays inert.
            if preview {
                // The catalog callback can now open a native chooser and persist a
                // selected source. Sample-layout pages receive no file service.
                services.catalog = None;
                services.roster = None;
                services.save_player = None;
                services.save_team = None;
                services.save_uniform = None;
                services.player_dna = None;
                services.team_dna = None;
                services.presets = None;
                services.editor = None;
                services.animation = None;
                services.online_ratings = None;
                services.online_shutdown = None;
            }
            server[index] = Some(Server::start(services)?);
        }
        let s = server[index].as_ref().unwrap();
        Ok(format!(
            "{}/renderer/index.html?workspace=roster&section={}{}#{}",
            s.origin,
            section,
            if preview {
                if section == "players" {
                    "&preview=player"
                } else {
                    "&preview=team"
                }
            } else {
                ""
            },
            s.token
        ))
    }
    pub fn shutdown(&self) {
        if let Some(shutdown) = &self.services.online_shutdown {
            shutdown();
        }
        if let Some(animation) = &self.services.animation {
            animation.shutdown();
        }
        if let Ok(mut s) = self.server.lock() {
            for server in s.iter_mut() {
                if let Some(mut server) = server.take() {
                    server.stop();
                }
            }
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl Server {
    fn start(services: Services) -> Result<Self> {
        let assets = Arc::new(assets()?);
        let token = token()?;
        let listener = loop {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;
            if listener.local_addr()?.port() != 43110 {
                break listener;
            }
        };
        listener.set_nonblocking(true)?;
        let origin = format!("http://{}", listener.local_addr()?);
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_stop, worker_origin, worker_token) =
            (stop.clone(), origin.clone(), token.clone());
        let thread = thread::Builder::new()
            .name("wpf-browser-workspace".into())
            .spawn(move || {
                let mut workers: Vec<JoinHandle<()>> = Vec::new();
                while !worker_stop.load(Ordering::Acquire) {
                    let mut i = 0;
                    while i < workers.len() {
                        if workers[i].is_finished() {
                            let _ = workers.swap_remove(i).join();
                        } else {
                            i += 1;
                        }
                    }
                    match listener.accept() {
                        Ok((mut stream, peer)) => {
                            if !peer.ip().is_loopback() || workers.len() >= 8 {
                                let _ = stream.shutdown(std::net::Shutdown::Both);
                                continue;
                            }
                            let (stop, origin, token, assets, services) = (
                                worker_stop.clone(),
                                worker_origin.clone(),
                                worker_token.clone(),
                                assets.clone(),
                                services.clone(),
                            );
                            let worker = thread::Builder::new()
                                .name("wpf-browser-request".into())
                                .spawn(move || {
                                    let result = read_request(&mut stream, &stop);
                                    if stop.load(Ordering::Acquire) {
                                        return;
                                    }
                                    let r = match result {
                                        Ok(req) => handle(req, &origin, &token, &assets, &services),
                                        Err(_) => {
                                            response(400, "Invalid or expired browser request")
                                        }
                                    };
                                    let _ = reply(&mut stream, r);
                                });
                            if let Ok(worker) = worker {
                                workers.push(worker);
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(_) => break,
                    }
                }
                worker_stop.store(true, Ordering::Release);
                for worker in workers {
                    let _ = worker.join();
                }
            })?;
        Ok(Self {
            origin,
            token,
            stop,
            thread: Some(thread),
        })
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn handle(req: Request, origin: &str, token: &str, assets: &Assets) -> Response {
        super::handle(req, origin, token, assets, &Services::default())
    }
    fn request(method: &str, path: &str) -> Request {
        Request {
            method: method.into(),
            target: path.into(),
            headers: HashMap::from([("host".into(), "127.0.0.1:12345".into())]),
            body: vec![],
        }
    }
    #[test]
    fn catalog_import_requires_exact_request_and_known_tab_before_service() {
        use std::sync::atomic::AtomicUsize;
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let services = Services {
            catalog: Some(Arc::new(move |_| {
                seen.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"loaded":true}))
            })),
            ..Services::default()
        };
        let request = |args: Value| {
            serde_json::to_vec(&json!({"method":"player23Catalog","args":args})).unwrap()
        };
        let body = request(json!([{"action":"import"}]));
        assert!(rpc(&body, &services, "sample-layout").is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let tab = "11111111-1111-1111-1111-111111111111";
        assert!(rpc(&request(json!([])), &services, tab).is_err());
        assert!(rpc(&request(json!([{}, {}])), &services, tab).is_err());
        assert!(rpc(&body, &Services::default(), tab).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(rpc(&body, &services, tab).is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn exact_original_assets_and_narrow_static_routes() {
        let a = assets().unwrap();
        assert_eq!(a.len(), 135);
        assert!(a.contains_key("/renderer/editor-destination-capabilities.js"));
        assert!(a.contains_key("/renderer/team-dna-v2.js"));
        assert!(a.contains_key("/renderer/dna-season-selection.js"));
        assert!(a.contains_key("/renderer/remote-ratings-v2.js"));
        assert!(a.contains_key("/renderer/team-metadata-v2.js"));
        assert!(a.contains_key("/renderer/player-origin-v2.js"));
        assert!(a.contains_key("/renderer/roster-workspace.css"));
        for path in [
            "/renderer/../Cargo.toml",
            "/renderer/%2e%2e/main.js",
            "/renderer\\app.js",
            "/renderer//app.js",
            "/assets/other.png",
            "/renderer/private.json",
            "//renderer/app.js",
        ] {
            assert!(!static_path(path), "{path}");
        }
        let r = handle(
            request("GET", "/renderer/index.html?workspace=roster"),
            "http://127.0.0.1:12345",
            &"a".repeat(64),
            &a,
        );
        assert_eq!(r.status, 200);
        let html = String::from_utf8(r.body).unwrap();
        assert!(html.contains(
            "<script src=\"roster-browser-bridge.js\"></script><script src=\"rust-browser-adapter.js\"></script><script src=\"theme.js\">"
        ));
        assert!(html.contains("roster-workspace.css"));
        assert_eq!(
            handle(
                request("GET", "/renderer/nonexistent.html"),
                "http://127.0.0.1:12345",
                &"a".repeat(64),
                &a
            )
            .status,
            404
        );
    }
    #[test]
    fn authenticated_rpc_is_fail_closed_without_game_dispatch() {
        let a = Assets::new();
        let origin = "http://127.0.0.1:12345";
        let token = "a".repeat(64);
        let mut req = request("POST", "/rpc");
        req.body = br#"{"method":"rosterBootstrap","args":[]}"#.to_vec();
        assert_eq!(handle(req, origin, &token, &a).status, 403);
        let make = |method: &str| {
            let mut req = request("POST", "/rpc");
            req.headers.extend([
                ("origin".into(), origin.into()),
                ("content-type".into(), "application/json".into()),
                ("x-roster-token".into(), token.clone()),
            ]);
            req.body = serde_json::to_vec(&json!({"method":method,"args":[]})).unwrap();
            req
        };
        let r = handle(make("rosterBootstrap"), origin, &token, &a);
        assert_eq!(r.status, 200);
        let v: Value = serde_json::from_slice(&r.body).unwrap();
        assert_eq!(v["result"]["playerResearch"]["status"], "idle");
        for method in [
            "savePlayer",
            "saveTeam",
            "saveTeamStaff",
            "saveTeamUniform",
            "nativeRosterExport",
            "launch",
            "game-rules-write",
            "open-library",
            "unknown",
        ] {
            let r = handle(make(method), origin, &token, &a);
            assert_eq!(r.status, 400);
            assert!(
                String::from_utf8(r.body)
                    .unwrap()
                    .contains("No game read or write")
            );
        }
        let mut req = make("ping");
        req.headers
            .insert("origin".into(), "https://example.com".into());
        assert_eq!(handle(req, origin, &token, &a).status, 403);
        let mut req = make("ping");
        req.headers.insert("host".into(), "localhost:12345".into());
        assert_eq!(handle(req, origin, &token, &a).status, 403);
        let mut req = make("ping");
        req.headers
            .insert("sec-fetch-site".into(), "cross-site".into());
        assert_eq!(handle(req, origin, &token, &a).status, 403);
        let mut req = make("ping");
        req.headers.insert("x-roster-token".into(), "b".repeat(64));
        assert_eq!(handle(req, origin, &token, &a).status, 403);
    }
    #[test]
    fn secure_token_and_owned_listener_shutdown() {
        let first = token().unwrap();
        let second = token().unwrap();
        assert_eq!(first.len(), 64);
        assert_ne!(first, second);
        assert!(token_equal(&first, &first));
        assert!(!token_equal(&first.to_uppercase(), &first));
        let runtime = Runtime::new();
        let url = runtime.url("players", true).unwrap();
        let parsed = tauri::Url::parse(&url).unwrap();
        assert_eq!(parsed.host_str(), Some("127.0.0.1"));
        assert_eq!(parsed.fragment().unwrap().len(), 64);
        let address = format!("127.0.0.1:{}", parsed.port().unwrap());
        let mut stream = TcpStream::connect(&address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        stream
            .write_all(
                format!("GET /renderer/styles.css HTTP/1.1\r\nHost: {address}\r\n\r\n").as_bytes(),
            )
            .unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).unwrap();
        assert!(reply.starts_with("HTTP/1.1 200"));
        assert!(reply.contains("frame-ancestors 'none'"));
        let mut pending = TcpStream::connect(&address).unwrap();
        pending.write_all(b"GET / HTTP/1.1\r\n").unwrap();
        thread::sleep(Duration::from_millis(30));
        let start = Instant::now();
        runtime.shutdown();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(TcpStream::connect(&address).is_err());
        assert!(runtime.url("../../bad", false).is_err());
    }
    #[test]
    fn parser_rejects_ambiguous_framing_and_oversized_bodies() {
        for headers in [
            "Host: x\r\nHost: y\r\n",
            "Transfer-Encoding: chunked\r\n",
            "Content-Length: 2097153\r\n",
            "Content-Length: -1\r\n",
            "Content-Length: 0\r\nExpect: 100-continue\r\n",
        ] {
            let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
            let address = listener.local_addr().unwrap();
            let mut client = TcpStream::connect(address).unwrap();
            client
                .write_all(format!("POST /rpc HTTP/1.1\r\n{headers}\r\n").as_bytes())
                .unwrap();
            let (mut server, _) = listener.accept().unwrap();
            assert!(
                read_request(&mut server, &AtomicBool::new(false)).is_err(),
                "{headers}"
            );
        }
    }
    #[test]
    fn sample_preview_has_no_roster_capability_and_normal_rpc_is_narrow() {
        use std::sync::atomic::AtomicUsize;
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let saves = Arc::new(AtomicUsize::new(0));
        let saved = saves.clone();
        let teams = Arc::new(AtomicUsize::new(0));
        let team = teams.clone();
        let uniforms = Arc::new(AtomicUsize::new(0));
        let uniform = uniforms.clone();
        let copies = Arc::new(AtomicUsize::new(0));
        let copied = copies.clone();
        let presets = Arc::new(AtomicUsize::new(0));
        let preset = presets.clone();
        let runtime=Runtime::with_services(Arc::new(|_|Ok(json!({}))),Arc::new(||Ok(()))).with_roster(Arc::new(move|method,tab|{
            ensure!(crate::roster_native::valid_tab(tab), "Invalid tab");
            observed.fetch_add(1,Ordering::SeqCst);
            Ok(json!({"status":"ready","method":method,"readOnly":true,"writes":0,"players":[],"teams":[]}))
        })).with_player_save(Arc::new(move|tab,input|{
            assert_eq!(tab,"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
            assert_eq!(input,&json!({"synthetic":true}));
            saved.fetch_add(1,Ordering::SeqCst);Ok(json!({"synthetic":true,"writes":0}))
        })).with_team_save(Arc::new(move|tab,input|{
            assert_eq!(tab,"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");assert_eq!(input,&json!({"synthetic":true}));
            team.fetch_add(1,Ordering::SeqCst);Ok(json!({"synthetic":true,"writes":0}))
        })).with_uniform_save(Arc::new(move|tab,input|{
            assert_eq!(tab,"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");assert_eq!(input,&json!({"synthetic":true}));uniform.fetch_add(1,Ordering::SeqCst);Ok(json!({"synthetic":true,"writes":0}))
        })).with_player_dna(Arc::new(move|tab,input|{
            ensure!(crate::roster_native::valid_tab(tab),"Invalid copy tab");
            assert_eq!(input,&json!({"action":"players"}));copied.fetch_add(1,Ordering::SeqCst);Ok(json!([]))
        })).with_presets(Arc::new(move|input|{
            assert_eq!(input,&json!({"kind":"signature","action":"list"}));preset.fetch_add(1,Ordering::SeqCst);Ok(json!([]))
        }));
        let preview = tauri::Url::parse(&runtime.url("players", true).unwrap()).unwrap();
        let actual = tauri::Url::parse(&runtime.url("teams", false).unwrap()).unwrap();
        assert_ne!(preview.port(), actual.port());
        assert_ne!(preview.fragment(), actual.fragment());
        let send_owned = |url: &tauri::Url, method: &str, args: Value, tab: &str| {
            let address = format!("127.0.0.1:{}", url.port().unwrap());
            let origin = format!("http://{address}");
            let body = serde_json::to_string(&json!({"method":method,"args":args})).unwrap();
            let mut stream = TcpStream::connect(&address).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(stream,"POST /rpc HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nX-Roster-Token: {}\r\nX-Animation-Owner: {tab}\r\nContent-Length: {}\r\n\r\n{body}",url.fragment().unwrap(),body.len()).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            let (header, body) = reply.split_once("\r\n\r\n").unwrap();
            (
                header.to_owned(),
                serde_json::from_str::<Value>(body).unwrap(),
            )
        };
        let send = |url: &tauri::Url, method: &str, args: Value| {
            send_owned(url, method, args, "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        };
        let (_, v) = send(&preview, "scanPlayerResearch", json!([]));
        assert_eq!(v["result"]["status"], "idle");
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let (_, v) = send(&actual, "rosterBootstrap", json!([]));
        assert_eq!(v["result"]["playerResearch"]["method"], "bootstrap");
        let (_, v) = send(&actual, "scanPlayerResearch", json!([]));
        assert_eq!(v["result"]["method"], "scanPlayerResearch");
        let before = calls.load(Ordering::SeqCst);
        for (method, args) in [
            ("scanPlayerResearch", json!([{"pointer":"0x12345"}])),
            ("savePlayer", json!([])),
            ("saveTeam", json!([])),
            ("saveTeamUniform", json!([])),
            ("game-rules-write", json!([])),
        ] {
            let (header, _) = send(&actual, method, args);
            assert!(header.starts_with("HTTP/1.1 400"));
        }
        assert_eq!(calls.load(Ordering::SeqCst), before);
        let (header, _) = send(&preview, "savePlayer", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(saves.load(Ordering::SeqCst), 0);
        let (header, _) = send_owned(
            &actual,
            "savePlayer",
            json!([{"synthetic":true}]),
            "invalid",
        );
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(saves.load(Ordering::SeqCst), 0);
        let (header, v) = send(&actual, "savePlayer", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 200"));
        assert_eq!(v["result"]["writes"], 0);
        assert_eq!(saves.load(Ordering::SeqCst), 1);
        let (header, _) = send(&preview, "saveTeam", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(teams.load(Ordering::SeqCst), 0);
        let (header, _) = send_owned(&actual, "saveTeam", json!([{"synthetic":true}]), "invalid");
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(teams.load(Ordering::SeqCst), 0);
        let (header, v) = send(&actual, "saveTeam", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 200"));
        assert_eq!(v["result"]["writes"], 0);
        assert_eq!(teams.load(Ordering::SeqCst), 1);
        let (header, _) = send(&preview, "saveTeamUniform", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(uniforms.load(Ordering::SeqCst), 0);
        let (header, _) = send_owned(
            &actual,
            "saveTeamUniform",
            json!([{"synthetic":true}]),
            "invalid",
        );
        assert!(header.starts_with("HTTP/1.1 400"));
        assert_eq!(uniforms.load(Ordering::SeqCst), 0);
        let (header, v) = send(&actual, "saveTeamUniform", json!([{"synthetic":true}]));
        assert!(header.starts_with("HTTP/1.1 200"));
        assert_eq!(v["result"]["writes"], 0);
        assert_eq!(uniforms.load(Ordering::SeqCst), 1);
        for (method, args, counter) in [
            ("playerDna", json!([{"action":"players"}]), &copies),
            (
                "editorPresets",
                json!([{"kind":"signature","action":"list"}]),
                &presets,
            ),
        ] {
            let (header, _) = send(&preview, method, args.clone());
            assert!(header.starts_with("HTTP/1.1 400"));
            assert_eq!(counter.load(Ordering::SeqCst), 0);
            let (header, v) = send(&actual, method, args);
            assert!(header.starts_with("HTTP/1.1 200"));
            assert_eq!(v["result"], json!([]));
            assert_eq!(counter.load(Ordering::SeqCst), 1);
            let (header, _) = send(&actual, method, json!([]));
            assert!(header.starts_with("HTTP/1.1 400"));
            assert_eq!(counter.load(Ordering::SeqCst), 1);
        }
        runtime.shutdown();
        for url in [&preview, &actual] {
            assert!(TcpStream::connect(format!("127.0.0.1:{}", url.port().unwrap())).is_err());
        }
    }
    #[test]
    fn team_dna_file_route_keeps_auth_tab_and_argument_boundary() {
        use std::sync::atomic::AtomicUsize;
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let services = Services {
            team_dna: Some(Arc::new(move |tab, arg| {
                assert_eq!(tab, "0f8f6a3c-1d2e-4b5a-9c7d-123456789abc");
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(json!({"action":arg["action"],"gameWrites":0}))
            })),
            ..Services::default()
        };
        let owner = "0f8f6a3c-1d2e-4b5a-9c7d-123456789abc";
        assert_eq!(
            rpc(
                br#"{"method":"teamDna","args":[{"action":"export"}]}"#,
                &services,
                owner
            )
            .unwrap()["gameWrites"],
            0
        );
        assert!(rpc(br#"{"method":"teamDna","args":[]}"#, &services, owner).is_err());
        assert!(rpc(br#"{"method":"teamDna","args":[{},{}]}"#, &services, owner).is_err());
        assert!(
            rpc(
                br#"{"method":"teamDna","args":[{"action":"apply"}]}"#,
                &services,
                owner
            )
            .is_err()
        );
        assert!(
            rpc(
                br#"{"method":"teamDna","args":[{"action":"import"}]}"#,
                &services,
                "invalid!"
            )
            .is_err()
        );
        let origin = "http://127.0.0.1:12345";
        let token = "a".repeat(64);
        let mut req = request("POST", "/rpc");
        req.headers.extend([
            ("origin".into(), origin.into()),
            ("content-type".into(), "application/json".into()),
            ("x-roster-token".into(), "b".repeat(64)),
            ("x-animation-owner".into(), owner.into()),
        ]);
        req.body = br#"{"method":"teamDna","args":[{"action":"export"}]}"#.to_vec();
        assert_eq!(
            super::handle(req, origin, &token, &Assets::new(), &services).status,
            403
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn only_authenticated_ping_renews_known_animation_interest() {
        use std::sync::atomic::AtomicU64;
        let ticks = Arc::new(AtomicU64::new(0));
        let base = Instant::now();
        let clock = ticks.clone();
        let animation = crate::animation_preview::Service::test_with_clock(Arc::new(move || {
            base + Duration::from_millis(clock.load(Ordering::SeqCst))
        }));
        let a = "11111111-1111-1111-1111-111111111111";
        let b = "22222222-2222-2222-2222-222222222222";
        let sa = animation.open(a).unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        let sb = animation.open(b).unwrap()["session"]
            .as_str()
            .unwrap()
            .to_owned();
        let services = Services {
            animation: Some(animation.clone()),
            ..Services::default()
        };
        let origin = "http://127.0.0.1:12345";
        let token = "a".repeat(64);
        let ping = |owner: &str, auth: &str| {
            let mut req = request("POST", "/rpc");
            req.headers.extend([
                ("origin".into(), origin.into()),
                ("content-type".into(), "application/json".into()),
                ("x-roster-token".into(), auth.into()),
                ("x-animation-owner".into(), owner.into()),
            ]);
            req.body = br#"{"method":"ping","args":[]}"#.to_vec();
            super::handle(req, origin, &token, &Assets::new(), &services).status
        };
        ticks.store(119_000, Ordering::SeqCst);
        assert_eq!(ping(a, &token), 200);
        assert_eq!(ping(b, &"b".repeat(64)), 403);
        ticks.store(120_000, Ordering::SeqCst);
        assert!(animation.progress(a, &sa).is_ok());
        assert!(animation.progress(b, &sb).is_err());
        ticks.store(240_000, Ordering::SeqCst);
        assert_eq!(ping(a, &token), 200);
        assert!(
            animation.progress(a, &sa).is_err(),
            "expired heartbeat cannot reopen old session"
        );
    }

    #[test]
    fn animation_preview_rpcs_and_owned_payload_route() {
        let a = Assets::new();
        let origin = "http://127.0.0.1:12345";
        let token = "a".repeat(64);
        let owner = "0f8f6a3c-1d2e-4b5a-9c7d-123456789abc";
        let root = std::env::temp_dir().join("c27-browser-animation-test");
        let services = Services {
            animation: Some(crate::animation_preview::Service::new(
                &root,
                Arc::new(|| bail!("Select your NBA 2K27 folder in Settings first.")),
            )),
            ..Services::default()
        };
        let call = |method: &str, args: Value, tab: &str| {
            let mut req = request("POST", "/rpc");
            req.headers.extend([
                ("origin".into(), origin.into()),
                ("content-type".into(), "application/json".into()),
                ("x-roster-token".into(), token.clone()),
                ("x-animation-owner".into(), tab.into()),
            ]);
            req.body = serde_json::to_vec(&json!({"method":method,"args":args})).unwrap();
            let r = super::handle(req, origin, &token, &a, &services);
            (r.status, serde_json::from_slice::<Value>(&r.body).unwrap())
        };
        let (status, v) = call("openAnimationPreview", json!([]), owner);
        assert_eq!(status, 200);
        let session = v["result"]["session"].as_str().unwrap().to_owned();
        assert_eq!(
            call("animationPreviewProgress", json!([session]), owner).1["result"]["session"],
            session.as_str()
        );
        assert_eq!(
            call("animationPreviewProgress", json!([session]), "1111-2222").0,
            400
        );
        // Without a game folder the catalog fails with WPF's message; nothing is prepared.
        let (status, v) = call("animationPreviewCatalog", json!([session]), owner);
        assert_eq!(status, 400);
        assert!(
            v["error"]
                .as_str()
                .unwrap()
                .contains("Select your NBA 2K27 folder")
        );
        assert!(!root.exists());
        assert_eq!(
            call(
                "animationPreviewRequest",
                json!([{"op":"export","args":{"session":session}}]),
                owner
            )
            .0,
            400
        );
        assert_eq!(call("warmAnimationPreview", json!(["yes"]), owner).0, 400);
        let payload = |handle: &str, roster: &str, tab: &str| {
            let mut req = request("GET", &format!("/animation-payload/{handle}"));
            req.headers.extend([
                ("x-roster-token".into(), roster.into()),
                ("x-animation-owner".into(), tab.into()),
            ]);
            super::handle(req, origin, &token, &a, &services).status
        };
        let handle = "b".repeat(48);
        assert_eq!(
            payload(&handle, &"c".repeat(64), owner),
            403,
            "roster token required"
        );
        assert_eq!(payload("../x", &token, owner), 403);
        assert_eq!(payload(&handle, &token, owner), 404, "unknown handle");
        assert_eq!(
            call("closeAnimationPreview", json!([session]), owner).1["result"]["closed"],
            true
        );
        assert_eq!(
            call("closeAnimationPreview", json!(["unknown"]), owner).1["result"]["closed"],
            true
        );
        // The stub behaviour stays for a page with no animation capability.
        let r = super::handle(
            {
                let mut req = request("POST", "/rpc");
                req.headers.extend([
                    ("origin".into(), origin.into()),
                    ("content-type".into(), "application/json".into()),
                    ("x-roster-token".into(), token.clone()),
                ]);
                req.body = br#"{"method":"warmAnimationPreview","args":[false]}"#.to_vec();
                req
            },
            origin,
            &token,
            &a,
            &Services::default(),
        );
        assert_eq!(r.status, 200);
        assert_eq!(
            serde_json::from_slice::<Value>(&r.body).unwrap()["result"],
            json!(false)
        );
    }
    /// Manual end-to-end check: serves the real WPF editor page with only the
    /// animation preview attached (no roster, no game memory) so a browser can
    /// drive WPF's animation_popup.js. Env: C27_LIVE_GAME_DIR, C27_ANIM_LAUNCHER_ROOT,
    /// C27_ANIM_URL_FILE (URL written there); runs until C27_ANIM_URL_FILE.stop exists.
    #[test]
    #[ignore]
    fn live_browser_animation_session() {
        let game = std::path::PathBuf::from(std::env::var("C27_LIVE_GAME_DIR").unwrap());
        let launcher = std::path::PathBuf::from(std::env::var("C27_ANIM_LAUNCHER_ROOT").unwrap());
        let file = std::path::PathBuf::from(std::env::var("C27_ANIM_URL_FILE").unwrap());
        let runtime = Runtime::with_services(Arc::new(|_| Ok(json!({}))), Arc::new(|| Ok(())))
            .with_animation(crate::animation_preview::Service::new(
                &launcher,
                Arc::new(move || Ok(game.clone())),
            ));
        std::fs::write(&file, runtime.url("players", false).unwrap()).unwrap();
        let stop = file.with_extension("stop");
        let started = Instant::now();
        while !stop.exists() && started.elapsed() < Duration::from_secs(1800) {
            thread::sleep(Duration::from_millis(200));
        }
        runtime.shutdown();
    }
}
