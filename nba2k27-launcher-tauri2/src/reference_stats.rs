//! Native, bounded Basketball-Reference NBA totals import. No HTML enters the UI,
//! no authentication/cookies, and no game handles or filesystem writes.
use anyhow::{Context, Result, ensure};
use regex::Regex;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
const MAX_BYTES: usize = 4 * 1024 * 1024;
const HOST: &str = "www.basketball-reference.com";
const FIXTURE_URL: &str = "https://www.basketball-reference.com/players/m/maxeyty01.html";
const KEYS: &[(&str, &[&str])] = &[
    ("ftm", &["ft", "ftm"]),
    ("fta", &["fta"]),
    ("fgm", &["fg", "fgm"]),
    ("fga", &["fga"]),
    ("threeMade", &["fg3", "fg3m"]),
    ("threeAttempted", &["fg3a"]),
    ("oreb", &["orb"]),
    ("dreb", &["drb"]),
    ("steals", &["stl"]),
    ("blocks", &["blk"]),
    ("assists", &["ast"]),
    ("turnovers", &["tov"]),
    ("points", &["pts"]),
    ("gp", &["g", "games"]),
    ("gs", &["gs", "games_started"]),
    ("minutes", &["mp", "minutes"]),
    ("fouls", &["pf"]),
    ("doubleDoubles", &[]),
    ("tripleDoubles", &[]),
    ("plusMinus", &[]),
];
fn expressions() -> &'static [Regex; 8] {
    static R: OnceLock<[Regex; 8]> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"(?is)<table\b([^>]*)>(.*?)</table>",
            r"(?is)<h1\b[^>]*>(.*?)</h1>",
            r"(?is)<tr\b[^>]*>(.*?)</tr>",
            r"(?is)<(?:td|th)\b([^>]*)>(.*?)</(?:td|th)>",
            r#"(?i)(?:^|\s)([a-z][a-z0-9_-]*)\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s>]+))"#,
            r"<[^>]*>",
            r"(?i)&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp|ndash|mdash);",
            r"^/players/[a-z]/[a-z0-9]+\.html$",
        ]
        .map(|r| Regex::new(r).expect("Constant regex"))
    })
}
pub fn player_url(value: &str) -> Result<String> {
    ensure!(
        value.encode_utf16().count() <= 500 && !value.chars().any(|c| c.is_control()),
        "Paste a Basketball-Reference NBA player URL."
    );
    let mut url =
        tauri::Url::parse(value.trim()).context("Paste a Basketball-Reference NBA player URL.")?;
    ensure!(
        url.scheme() == "https"
            && matches!(url.host_str(), Some("basketball-reference.com" | HOST))
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && expressions()[7].is_match(url.path()),
        "Use an HTTPS Basketball-Reference NBA player page URL."
    );
    url.set_host(Some(HOST))?;
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.to_string())
}
fn attribute(tag: &str, name: &str) -> String {
    expressions()[4]
        .captures_iter(tag)
        .find(|c| c[1].eq_ignore_ascii_case(name))
        .and_then(|c| c.get(2).or_else(|| c.get(3)).or_else(|| c.get(4)))
        .map_or(String::new(), |c| c.as_str().to_owned())
}
fn text(html: &str) -> String {
    let stripped = expressions()[5].replace_all(html, "");
    let decoded = expressions()[6].replace_all(&stripped, |c: &regex::Captures| {
        let e = c[1].to_ascii_lowercase();
        if let Some(hex) = e.strip_prefix("#x") {
            u32::from_str_radix(hex, 16)
                .ok()
                .and_then(char::from_u32)
                .filter(|&c| c != '\0')
                .map_or(String::new(), |c| c.to_string())
        } else if let Some(n) = e.strip_prefix('#') {
            n.parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .filter(|&c| c != '\0')
                .map_or(String::new(), |c| c.to_string())
        } else {
            match e.as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                "nbsp" => " ",
                "ndash" => "–",
                "mdash" => "—",
                _ => "",
            }
            .to_owned()
        }
    });
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn team(code: &str) -> &str {
    match code {
        "ATL" => "Hawks",
        "BOS" => "Celtics",
        "BRK" | "NJN" => "Nets",
        "CHA" | "CHO" | "CHH" => "Hornets",
        "CHI" => "Bulls",
        "CLE" => "Cavaliers",
        "DAL" => "Mavericks",
        "DEN" => "Nuggets",
        "DET" => "Pistons",
        "GSW" => "Warriors",
        "HOU" => "Rockets",
        "IND" => "Pacers",
        "LAC" => "Clippers",
        "LAL" => "Lakers",
        "MEM" | "VAN" => "Grizzlies",
        "MIA" => "Heat",
        "MIL" => "Bucks",
        "MIN" => "Timberwolves",
        "NOP" | "NOH" | "NOK" => "Pelicans",
        "NYK" => "Knicks",
        "OKC" => "Thunder",
        "ORL" => "Magic",
        "PHI" => "76ers",
        "PHO" | "PHX" => "Suns",
        "POR" => "Trail Blazers",
        "SAC" => "Kings",
        "SAS" => "Spurs",
        "TOR" => "Raptors",
        "UTA" => "Jazz",
        "WAS" | "WSB" => "Wizards",
        _ => code,
    }
}
pub fn parse(html: &str, url: &str) -> Result<Value> {
    ensure!(
        html.len() <= MAX_BYTES,
        "Basketball-Reference page is too large."
    );
    let url = player_url(url)?;
    // Lazy tables in comments remain inert text; only reviewed numeric records leave here.
    let tables: Vec<_> = expressions()[0]
        .captures_iter(html)
        .filter(|c| matches!(attribute(&c[1], "id").as_str(), "totals" | "totals_stats"))
        .take(2)
        .collect();
    ensure!(
        tables.len() == 1,
        "No unique NBA regular-season totals table was found. The page may be blocked or its format changed."
    );
    let name = expressions()[1]
        .captures(html)
        .map(|c| text(&c[1]))
        .context("Player name missing")?;
    ensure!(
        !name.is_empty()
            && name.encode_utf16().count() <= 120
            && !name.chars().any(|c| c.is_control()),
        "Player name could not be verified."
    );
    let mut years: BTreeMap<u32, Vec<Value>> = BTreeMap::new();
    let mut warnings = Vec::new();
    for (i, c) in expressions()[2].captures_iter(&tables[0][2]).enumerate() {
        ensure!(i < 512, "Too many totals rows.");
        let mut cells = BTreeMap::new();
        for (j, cell) in expressions()[3].captures_iter(&c[1]).enumerate() {
            ensure!(j < 128, "Too many totals columns.");
            let key = attribute(&cell[1], "data-stat");
            if !key.is_empty() {
                ensure!(
                    cells.insert(key, text(&cell[2])).is_none(),
                    "Duplicate totals column."
                );
            }
        }
        let get = |keys: &[&str]| keys.iter().find_map(|k| cells.get(*k)).map(String::as_str);
        let Some(label) = get(&["season", "year_id"]) else {
            continue;
        };
        let parts = label.replace('–', "-");
        let b = parts.as_bytes();
        if b.len() != 7
            || b[4] != b'-'
            || !b[..4].iter().chain(b[5..].iter()).all(u8::is_ascii_digit)
        {
            continue;
        }
        let year = parts[..4].parse::<u32>()? + 1;
        ensure!(
            (1900..=2155).contains(&year) && parts[5..].parse::<u32>()? == year % 100,
            "Invalid season year."
        );
        if get(&["lg_id", "comp_name_abbr", "league_id"]) != Some("NBA") {
            warnings.push(format!("{label}: non-NBA row skipped."));
            continue;
        }
        let abbreviation =
            get(&["team_id", "team_name_abbr"]).context("A season team is missing.")?;
        ensure!(
            !abbreviation.is_empty() && abbreviation.len() <= 100,
            "A season team is missing."
        );
        let mut totals = serde_json::Map::new();
        let mut missing = Vec::new();
        for &(key, aliases) in KEYS {
            let raw = aliases
                .iter()
                .filter_map(|k| cells.get(*k))
                .find(|v| !v.is_empty());
            let n = if let Some(raw) = raw {
                let normalized = raw.replace(',', "");
                ensure!(
                    !normalized.is_empty() && normalized.bytes().all(|b| b.is_ascii_digit()),
                    "{label}: unsupported {key} total. Nothing was imported."
                );
                let n = normalized
                    .parse::<u32>()
                    .with_context(|| format!("Unsupported {key} total"))?;
                ensure!(
                    n <= crate::player_dna::schema()["seasonLimits"][key]
                        .as_u64()
                        .unwrap() as u32,
                    "{label}: unsupported {key} total. Nothing was imported."
                );
                n
            } else {
                missing.push(key);
                0
            };
            totals.insert(key.into(), json!(n));
        }
        ensure!(
            !["gp", "minutes", "points", "fgm", "fga", "ftm", "fta"]
                .iter()
                .any(|k| missing.contains(k)),
            "{label}: required total is missing."
        );
        if totals["gp"] == 0 {
            continue;
        }
        let bytes = abbreviation.as_bytes();
        let aggregate = abbreviation == "TOT"
            || (bytes.len() == 3 && (b'2'..=b'9').contains(&bytes[0]) && &bytes[1..] == b"TM");
        let row = json!({"year":year,"label":label,"team":team(abbreviation),"abbreviation":abbreviation,"aggregate":aggregate,"totals":totals,"missingFields":missing});
        years.entry(year).or_default().push(row);
    }
    let mut seasons = Vec::new();
    for (year, rows) in years.into_iter().rev() {
        let aggregates: Vec<_> = rows.iter().filter(|r| r["aggregate"] == true).collect();
        ensure!(
            aggregates.len() == 1 || (aggregates.is_empty() && rows.len() == 1),
            "{year}: no unique aggregate season totals. Nothing was imported."
        );
        let row = aggregates.first().copied().unwrap_or(&rows[0]);
        crate::player_dna::seasons(
            &json!([{"year":year,"team":row["team"],"totals":row["totals"]}]),
        )?;
        seasons.push(row.clone());
    }
    ensure!(
        !seasons.is_empty() && seasons.len() <= 40,
        "No supported NBA regular-season history was found."
    );
    let mut seen = HashSet::new();
    warnings.retain(|s| seen.insert(s.clone()));
    Ok(
        json!({"name":name,"url":url,"seasons":seasons,"warnings":warnings,"source":"Basketball-Reference","awardsSupported":false}),
    )
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub encoding: String,
    pub location: String,
    pub body: Vec<u8>,
}
pub trait Transport {
    fn get(&self, url: &str, deadline: Instant) -> Result<Response>;
}
pub struct WinHttp;
impl Transport for WinHttp {
    fn get(&self, url: &str, deadline: Instant) -> Result<Response> {
        use std::{ffi::c_void, ptr};
        use windows::{
            Win32::Networking::WinHttp::*,
            core::{PCWSTR, w},
        };
        struct Handle(*mut c_void);
        impl Handle {
            fn new(p: *mut c_void) -> Result<Self> {
                ensure!(
                    !p.is_null(),
                    "Network handle unavailable: {}",
                    std::io::Error::last_os_error()
                );
                Ok(Self(p))
            }
        }
        impl Drop for Handle {
            fn drop(&mut self) {
                unsafe {
                    let _ = WinHttpCloseHandle(self.0);
                }
            }
        }
        fn header(h: *mut c_void, key: u32, optional: bool) -> Result<String> {
            let mut buf = [0u16; 4096];
            let mut size = (buf.len() * 2) as u32;
            let mut index = 0;
            let result = unsafe {
                WinHttpQueryHeaders(
                    h,
                    key,
                    PCWSTR::null(),
                    Some(buf.as_mut_ptr().cast()),
                    &mut size,
                    &mut index,
                )
            };
            if let Err(error) = result {
                if optional && error.code().0 as u32 == 0x80072F76 {
                    return Ok(String::new());
                }
                return Err(error.into());
            }
            ensure!(
                size as usize <= buf.len() * 2 && size % 2 == 0,
                "Oversized or invalid response header"
            );
            Ok(String::from_utf16(&buf[..size as usize / 2])?
                .trim_end_matches('\0')
                .to_owned())
        }
        fn timeouts(h: *mut c_void, deadline: Instant) -> Result<()> {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis();
            ensure!(
                remaining > 0,
                "Basketball-Reference request timed out. No stats were imported."
            );
            let ms = remaining.min(4000) as i32;
            unsafe {
                WinHttpSetTimeouts(h, ms.min(3000), ms, ms, ms)?;
            }
            Ok(())
        }
        let canonical = player_url(url)?;
        let parsed = tauri::Url::parse(&canonical)?;
        let object: Vec<u16> = parsed.path().encode_utf16().chain(Some(0)).collect();
        unsafe {
            let session = Handle::new(WinHttpOpen(
                w!("2k-Launcher-Rust/0.11.0 (season stats import)"),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            ))?;
            timeouts(session.0, deadline)?;
            let connection = Handle::new(WinHttpConnect(
                session.0,
                w!("www.basketball-reference.com"),
                443,
                0,
            ))?;
            let request = Handle::new(WinHttpOpenRequest(
                connection.0,
                w!("GET"),
                PCWSTR(object.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                ptr::null(),
                WINHTTP_FLAG_SECURE | WINHTTP_FLAG_REFRESH,
            ))?;
            let disabled = WINHTTP_DISABLE_REDIRECTS
                | WINHTTP_DISABLE_COOKIES
                | WINHTTP_DISABLE_AUTHENTICATION;
            WinHttpSetOption(
                Some(request.0),
                WINHTTP_OPTION_DISABLE_FEATURE,
                Some(&disabled.to_ne_bytes()),
            )?;
            timeouts(request.0, deadline)?;
            let headers: Vec<u16> = "Accept: text/html\r\nAccept-Encoding: identity\r\n"
                .encode_utf16()
                .collect();
            WinHttpSendRequest(request.0, Some(&headers), None, 0, 0, 0).context(
                "Basketball-Reference connection failed or timed out. No stats were imported.",
            )?;
            timeouts(request.0, deadline)?;
            WinHttpReceiveResponse(request.0, ptr::null_mut()).context(
                "Basketball-Reference response failed or timed out. No stats were imported.",
            )?;
            let status = header(request.0, WINHTTP_QUERY_STATUS_CODE, false)?.parse::<u16>()?;
            let mut result = Response {
                status,
                content_type: String::new(),
                encoding: String::new(),
                location: String::new(),
                body: Vec::new(),
            };
            if [301, 302, 303, 307, 308].contains(&status) {
                result.location = header(request.0, WINHTTP_QUERY_LOCATION, false)?;
                ensure!(!result.location.is_empty(), "Player redirect is missing.");
                return Ok(result);
            }
            if status != 200 {
                return Ok(result);
            }
            result.content_type = header(request.0, WINHTTP_QUERY_CONTENT_TYPE, false)?;
            ensure!(
                result
                    .content_type
                    .split(';')
                    .next()
                    .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/html")),
                "The player URL did not return an HTML page."
            );
            result.encoding = header(request.0, WINHTTP_QUERY_CONTENT_ENCODING, true)?;
            ensure!(
                result.encoding.is_empty() || result.encoding.eq_ignore_ascii_case("identity"),
                "Unsupported compressed player page."
            );
            let length = header(request.0, WINHTTP_QUERY_CONTENT_LENGTH, true)?;
            if !length.is_empty() {
                ensure!(
                    length.parse::<u64>().context("Invalid response length")? <= MAX_BYTES as u64,
                    "Basketball-Reference page is too large."
                );
            }
            let mut chunk = [0u8; 16384];
            loop {
                timeouts(request.0, deadline)?;
                let mut read = 0;
                WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    chunk.len() as u32,
                    &mut read,
                )
                .context(
                    "Basketball-Reference download failed or timed out. No stats were imported.",
                )?;
                ensure!(read as usize <= chunk.len(), "Invalid download size");
                if read == 0 {
                    break;
                }
                ensure!(
                    result.body.len() + read as usize <= MAX_BYTES,
                    "Basketball-Reference page is too large."
                );
                result.body.extend_from_slice(&chunk[..read as usize]);
            }
            Ok(result)
        }
    }
}
fn redirected(url: &str, location: &str) -> Result<String> {
    player_url(tauri::Url::parse(url)?.join(location)?.as_str())
}
pub fn download(transport: &impl Transport, url: &str) -> Result<Value> {
    let mut url = player_url(url)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    for redirects in 0..=2 {
        ensure!(
            Instant::now() < deadline,
            "Basketball-Reference request timed out. No stats were imported."
        );
        let r = transport.get(&url, deadline)?;
        ensure!(
            Instant::now() < deadline,
            "Basketball-Reference request timed out. No stats were imported."
        );
        if [301, 302, 303, 307, 308].contains(&r.status) {
            ensure!(redirects < 2, "Too many player-page redirects.");
            url = redirected(&url, &r.location)?;
            continue;
        }
        ensure!(
            r.status == 200,
            "Basketball-Reference {} (HTTP {}). Try again later. No stats were imported.",
            if [403, 429].contains(&r.status) {
                "blocked this request"
            } else {
                "returned an error"
            },
            r.status
        );
        ensure!(
            r.content_type
                .split(';')
                .next()
                .is_some_and(|s| s.trim().eq_ignore_ascii_case("text/html")),
            "The player URL did not return an HTML page."
        );
        ensure!(
            r.encoding.is_empty() || r.encoding.eq_ignore_ascii_case("identity"),
            "Unsupported compressed player page."
        );
        ensure!(
            r.body.len() <= MAX_BYTES,
            "Basketball-Reference page is too large."
        );
        return parse(
            std::str::from_utf8(&r.body).context("Player page was not UTF-8")?,
            &url,
        );
    }
    unreachable!()
}
#[derive(Default)]
struct State {
    busy: bool,
    last: Option<Instant>,
    cache: VecDeque<Entry>,
}
struct Entry {
    url: String,
    token: String,
    at: Instant,
    data: Value,
}
#[derive(Default)]
pub struct Service {
    state: Mutex<State>,
}
struct Busy<'a>(&'a Service);
impl Drop for Busy<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = self.0.state.lock() {
            s.busy = false;
        }
    }
}
impl Service {
    fn remember(&self, url: String, data: Value) -> Result<Value> {
        let token = crate::backend::unique_id();
        let mut s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Stats cache unavailable"))?;
        s.cache.retain(|e| e.url != url);
        s.cache.push_back(Entry {
            url,
            token: token.clone(),
            at: Instant::now(),
            data: data.clone(),
        });
        while s.cache.len() > 4 {
            s.cache.pop_front();
        }
        let mut result = data;
        result["token"] = json!(token);
        Ok(result)
    }
    pub fn load(&self, value: &str, transport: &impl Transport) -> Result<Value> {
        let url = player_url(value)?;
        {
            let mut s = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("Stats cache unavailable"))?;
            if let Some(e) = s
                .cache
                .iter()
                .find(|e| e.url == url && e.at.elapsed() < Duration::from_secs(600))
            {
                let mut d = e.data.clone();
                d["token"] = json!(e.token);
                return Ok(d);
            }
            ensure!(!s.busy, "A player-page request is already running.");
            ensure!(
                s.last
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(5)),
                "Please wait a few seconds before requesting another player page."
            );
            s.busy = true;
            s.last = Some(Instant::now());
        }
        let _busy = Busy(self);
        let data = download(transport, &url)?;
        self.remember(url, data)
    }
    pub fn snapshot(&self, token: &str) -> Result<Value> {
        let s = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("Stats cache unavailable"))?;
        ensure!(token.len() == 48, "Invalid stats review token");
        Ok(s.cache
            .iter()
            .find(|e| e.token == token && e.at.elapsed() < Duration::from_secs(600))
            .context("Stats review expired; load the player URL again.")?
            .data
            .clone())
    }
    pub fn fixture(&self) -> Result<Value> {
        self.remember(
            FIXTURE_URL.into(),
            parse(
                include_str!("../assets/reference-fixture.html"),
                FIXTURE_URL,
            )?,
        )
    }
}

/// Review cannot replace player info/ratings/awards or fabricate unprovided historic totals.
pub fn stage(document: &Value, source: &Value, catalog: &Value, selected: &Value) -> Result<Value> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Choice {
        year: u32,
        team_id: u32,
    }
    let choices: Vec<Choice> = serde_json::from_value(selected.clone())?;
    ensure!(
        !choices.is_empty() && choices.len() <= 25,
        "Choose 1–25 seasons."
    );
    let mut document = crate::player_dna::validate(document)?;
    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for choice in choices {
        ensure!(
            seen.insert(choice.year),
            "Choose one aggregate row per imported season."
        );
        let row = source["seasons"]
            .as_array()
            .context("Source seasons missing")?
            .iter()
            .find(|r| r["year"] == choice.year)
            .context("Season does not belong to the loaded player.")?;
        let team = catalog["teams"]
            .as_array()
            .context("Teams missing")?
            .iter()
            .find(|t| t["category"] == "current" && t["id"] == choice.team_id)
            .context("Choose a normal destination team for each checked season.")?;
        let old = document["seasonHistory"]
            .as_array()
            .and_then(|rs| rs.iter().find(|r| r["year"] == choice.year));
        let mut totals = row["totals"].clone();
        for k in row["missingFields"]
            .as_array()
            .context("Missing-field list unavailable")?
        {
            let k = k.as_str().context("Invalid missing-field key")?;
            ensure!(
                crate::player_dna::schema()["seasonLimits"].get(k).is_some(),
                "Unknown season total."
            );
            totals[k] = old.map(|r| r["totals"][k].clone()).unwrap_or(json!(0));
        }
        rows.push(json!({"year":choice.year,"team":team["name"],"totals":totals}));
    }
    document["seasonHistory"] = crate::player_dna::merge_seasons(
        &json!(rows),
        document.get("seasonHistory").unwrap_or(&json!([])),
    )?;
    document["version"] = json!(2);
    crate::player_dna::validate(&document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    fn fixture() -> Value {
        parse(
            include_str!("../assets/reference-fixture.html"),
            FIXTURE_URL,
        )
        .unwrap()
    }
    fn document() -> Value {
        json!({"format":"2kplayer","version":1,"game":"NBA2K27","schemaId":crate::player_dna::schema()["schemaId"],"name":"Destination","info":{"yearsPro":7},"fields":{}})
    }
    fn catalog() -> Value {
        json!({"teams":[{"id":0,"name":"76ers","category":"current"},{"id":1,"name":"76ers","category":"historic"}]})
    }
    #[test]
    fn parser_and_urls_match_wpf_oracle() {
        let data: Value =
            serde_json::from_str(include_str!("../assets/reference-regressions.json")).unwrap();
        for (i, c) in data["cases"].as_array().unwrap().iter().enumerate() {
            assert_eq!(
                parse(c["html"].as_str().unwrap(), c["url"].as_str().unwrap()).ok(),
                if c["expected"].is_null() {
                    None
                } else {
                    Some(c["expected"].clone())
                },
                "Parser case {i}"
            );
        }
        for c in data["urlCases"].as_array().unwrap() {
            assert_eq!(
                player_url(c["value"].as_str().unwrap()).ok(),
                c["expected"].as_str().map(str::to_owned)
            );
        }
    }
    #[test]
    fn ambiguous_attributes_and_resource_limits_fail_closed() {
        let html = include_str!("../assets/reference-fixture.html");
        assert!(parse(&html.replace("id=", "data-id="), FIXTURE_URL).is_err());
        assert!(parse(&"a".repeat(MAX_BYTES + 1), FIXTURE_URL).is_err());
        assert!(
            player_url("https://www.basketball-reference.com/players/m/%6daxeyty01.html").is_err()
        );
        assert!(
            player_url("https://www.basketball-reference.com/players/m/maxeyty01.html\n").is_err()
        );
    }
    struct Mock {
        calls: AtomicUsize,
        status: u16,
        location: &'static str,
        html: Option<&'static str>,
        mime: &'static str,
    }
    impl Transport for Mock {
        fn get(&self, _: &str, _: Instant) -> Result<Response> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            Ok(Response {
                status: self.status,
                location: self.location.into(),
                content_type: self.mime.into(),
                encoding: String::new(),
                body: self.html.unwrap_or("").as_bytes().to_vec(),
            })
        }
    }
    fn mock(status: u16) -> Mock {
        Mock {
            calls: AtomicUsize::new(0),
            status,
            location: "",
            html: Some(include_str!("../assets/reference-fixture.html")),
            mime: "text/html; charset=utf-8",
        }
    }
    #[test]
    fn blocked_and_redirected_responses_never_retry_or_escape_host() {
        for status in [403, 429, 500] {
            let t = mock(status);
            assert!(download(&t, FIXTURE_URL).is_err());
            assert_eq!(t.calls.load(Ordering::Relaxed), 1);
        }
        let mut t = mock(302);
        t.location = "https://evil.example/players/m/maxeyty01.html";
        assert!(download(&t, FIXTURE_URL).is_err());
        assert_eq!(t.calls.load(Ordering::Relaxed), 1);
        t.location = "/players/m/maxeyty01.html";
        t.calls.store(0, Ordering::Relaxed);
        assert!(download(&t, FIXTURE_URL).is_err());
        assert_eq!(t.calls.load(Ordering::Relaxed), 3);
        let mut t = mock(200);
        t.mime = "image/png";
        assert!(download(&t, FIXTURE_URL).is_err());
    }
    #[test]
    fn cache_throttle_failure_release_and_expiry() {
        let s = Service::default();
        let t = mock(200);
        let mut data = s.load(FIXTURE_URL, &t).unwrap();
        data["seasons"][0]["totals"]["gp"] = json!(0);
        let second = s.load(FIXTURE_URL, &t).unwrap();
        assert_eq!(second["seasons"][0]["totals"]["gp"], 70);
        assert_eq!(t.calls.load(Ordering::Relaxed), 1);
        assert!(
            s.load(
                "https://www.basketball-reference.com/players/c/curry01.html",
                &t
            )
            .is_err()
        );
        let token = second["token"].as_str().unwrap();
        assert!(s.snapshot(token).is_ok());
        s.state.lock().unwrap().cache[0].at = Instant::now() - Duration::from_secs(601);
        assert!(s.snapshot(token).is_err());
        let bad = Service::default();
        assert!(bad.load(FIXTURE_URL, &mock(403)).is_err());
        assert!(!bad.state.lock().unwrap().busy);
        assert!(bad.state.lock().unwrap().cache.is_empty());
        s.state.lock().unwrap().busy = true;
        s.state.lock().unwrap().last = None;
        assert!(s.load(FIXTURE_URL, &t).is_err());
    }
    #[test]
    fn cache_is_bounded_and_evicted_review_cannot_stage() {
        let s = Service::default();
        let first = s.fixture().unwrap();
        for i in 0..5 {
            s.remember(format!("fixture{i}"), fixture()).unwrap();
        }
        assert_eq!(s.state.lock().unwrap().cache.len(), 4);
        assert!(s.snapshot(first["token"].as_str().unwrap()).is_err());
    }
    #[test]
    fn reviewed_stats_preserve_ratings_awards_and_unknown_totals() {
        let f = fixture();
        let mut d = document();
        d["version"] = json!(2);
        let mut row = json!({"year":2026,"team":"76ers","totals":f["seasons"][0]["totals"]});
        row["totals"]["doubleDoubles"] = json!(10);
        row["totals"]["tripleDoubles"] = json!(2);
        row["totals"]["plusMinus"] = json!(-8);
        row["awards"] = json!(
            crate::player_dna::schema()["awards"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| (a["key"].as_str().unwrap().to_owned(), json!(0)))
                .collect::<serde_json::Map<String, Value>>()
        );
        row["awards"]["mvp"] = json!(1);
        d["seasonHistory"] = json!([row]);
        let r = stage(&d, &f, &catalog(), &json!([{"year":2026,"teamId":0}])).unwrap();
        assert_eq!(r["name"], d["name"]);
        assert_eq!(r["info"], d["info"]);
        assert_eq!(r["fields"], d["fields"]);
        assert_eq!(r["seasonHistory"][0]["awards"]["mvp"], 1);
        assert_eq!(r["seasonHistory"][0]["totals"]["doubleDoubles"], 10);
        assert_eq!(r["seasonHistory"][0]["totals"]["plusMinus"], -8);
        assert!(stage(&d, &f, &catalog(), &json!([{"year":2026,"teamId":1}])).is_err());
        assert!(stage(&d, &f, &catalog(), &json!([{"year":2027,"teamId":0}])).is_err());
        assert!(
            stage(
                &d,
                &f,
                &catalog(),
                &json!([{"year":2026,"teamId":0},{"year":2026,"teamId":0}])
            )
            .is_err()
        );
        assert!(stage(&d, &f, &catalog(), &json!([])).is_err());
    }
}
