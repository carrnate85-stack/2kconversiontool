//! Developer-only one-shot Version2 ball observation. READ/QUERY/SYNCHRONIZE
//! only; there is no writer API, allocation, protection change or engine call.
//! The immutable owned receipt supplies identity+selected ID, NEVER addresses.
#![allow(dead_code)]
use crate::{
    ball_v2,
    blacktop_arena::{Inspect, Region},
    blacktop_ball as ball,
    blacktop_native::Memory,
    rules_v2,
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    mem::size_of,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::{Debug::ReadProcessMemory, ToolHelp::*},
            Memory::*,
            Threading::*,
        },
    },
    core::PWSTR,
};
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Game {
    pid: u32,
    started: String,
    path: String,
}
struct Expected {
    game: Game,
    selected: String,
    source: u32,
    image: u64,
}
fn expected(root: &Path, pid: u32) -> Result<Expected> {
    let _pins = crate::loader_files::Directories::pin(&root.join("Data"))?;
    let path = root.join("Data/blacktop-ball-session.json");
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x00200000)
        .open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file()
            && before.file_attributes() & 0x400 == 0
            && (1..=16384).contains(&before.len()),
        "Probe requires a bounded ordinary owned ball receipt"
    );
    let mut bytes = Vec::new();
    (&file).take(16385).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 == before.len()
            && after.len() == before.len()
            && after.last_write_time() == before.last_write_time(),
        "Owned ball intent changed while read"
    );
    let v: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        v["schema"] == "nba2k27-native-ball-transaction-v1"
            && v["session"]
                .as_str()
                .is_some_and(|s| s.len() == 48 && s.bytes().all(|b| b.is_ascii_hexdigit())),
        "Probe requires the current owned Rust ball intent"
    );
    let game: Game = serde_json::from_value(v["game"].clone())?;
    ensure!(
        game.pid == pid
            && pid > 0
            && game.started.parse::<u64>().is_ok_and(|v| v > 0)
            && Path::new(&game.path).is_absolute()
            && Path::new(&game.path)
                .file_name()
                .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case("NBA2K27.exe")),
        "Probe receipt does not match exact requested game"
    );
    let selected = v["selected"]
        .as_str()
        .context("Missing selected ball")?
        .to_owned();
    ball::choice(&selected)?;
    ensure!(!ball::current(&selected), "No substituted ball is owned");
    let source = u32::try_from(v["source"].as_u64().context("Missing selected model ID")?)?;
    ensure!(
        source != 0 && source != ball::NORMAL,
        "Invalid selected source model"
    );
    Ok(Expected {
        game,
        selected,
        source,
        image: v["image"].as_u64().context("Missing receipt image")?,
    })
}
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn same(a: &str, b: &str) -> bool {
    a.trim_start_matches(r"\\?\")
        .eq_ignore_ascii_case(b.trim_start_matches(r"\\?\"))
}
fn protected(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("easyanticheat") || n == "eaclaunch.exe" || n == "start_protected_game.exe"
}
fn ctext(words: &[u16]) -> Result<String> {
    let end = words
        .iter()
        .position(|w| *w == 0)
        .context("Unterminated native name")?;
    Ok(String::from_utf16(&words[..end])?)
}
fn no_eac() -> Result<()> {
    let s = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? });
    let mut p = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    unsafe {
        Process32FirstW(s.0, &mut p)?;
    }
    for _ in 0..65536 {
        ensure!(
            !protected(&ctext(&p.szExeFile)?),
            "Protected process present; diagnostic refused"
        );
        match unsafe { Process32NextW(s.0, &mut p) } {
            Ok(()) => {}
            Err(e) if e.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) => {
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
    }
    anyhow::bail!("Process snapshot exceeds bounds")
}
struct Native {
    handle: Handle,
    game: Game,
    base: u64,
    clock: Instant,
}
impl Native {
    fn open(e: &Expected) -> Result<Self> {
        no_eac()?;
        let handle = Handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_VM_READ | PROCESS_SYNCHRONIZE,
                false,
                e.game.pid,
            )?
        });
        let mut m = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let snap = Handle(unsafe {
            CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, e.game.pid)?
        });
        unsafe {
            Module32FirstW(snap.0, &mut m)?;
        }
        ensure!(
            same(&ctext(&m.szExePath)?, &e.game.path),
            "First game module path differs"
        );
        let base = m.modBaseAddr as u64;
        ensure!(
            base == e.image,
            "Current first module differs from the exact owned intent"
        );
        for i in 0..2048 {
            ensure!(
                !protected(&ctext(&m.szModule)?),
                "Protected module present; diagnostic refused"
            );
            match unsafe { Module32NextW(snap.0, &mut m) } {
                Ok(()) => ensure!(i < 2047, "Module snapshot exceeds bounds"),
                Err(err)
                    if err.code() == windows::core::HRESULT::from_win32(ERROR_NO_MORE_FILES.0) =>
                {
                    break;
                }
                Err(err) => return Err(err.into()),
            }
        }
        let n = Self {
            handle,
            game: e.game.clone(),
            base,
            clock: Instant::now(),
        };
        n.checkpoint()?;
        ball_v2::verify_profile(&n)?;
        Ok(n)
    }
}
impl Memory for Native {
    fn base(&self) -> u64 {
        self.base
    }
    fn checkpoint(&self) -> Result<()> {
        ensure!(
            self.clock.elapsed().as_secs() < 10,
            "Read-only ball diagnostic time budget exceeded"
        );
        ensure!(
            unsafe { WaitForSingleObject(self.handle.0, 0) } == WAIT_TIMEOUT,
            "Exact game ended or wait state unknown"
        );
        let (mut c, mut e, mut k, mut u) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        unsafe {
            GetProcessTimes(self.handle.0, &mut c, &mut e, &mut k, &mut u)?;
        }
        ensure!(
            (((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64).to_string()
                == self.game.started,
            "Exact game birth differs"
        );
        let mut text = vec![0u16; 32768];
        let mut len = text.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                self.handle.0,
                PROCESS_NAME_WIN32,
                PWSTR(text.as_mut_ptr()),
                &mut len,
            )?;
        }
        ensure!(
            len > 0
                && (len as usize) < text.len()
                && same(&String::from_utf16(&text[..len as usize])?, &self.game.path),
            "Exact retained game path differs"
        );
        Ok(())
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        ensure!(
            a >= 0x10000
                && (1..=65536).contains(&n)
                && a.checked_add(n as u64).is_some_and(|e| e < 0x7fff00000000),
            "Invalid bounded diagnostic read"
        );
        self.checkpoint()?;
        let r = self.region(a)?;
        ensure!(
            r.committed
                && a >= r.start
                && a.checked_add(n as u64)
                    .zip(r.start.checked_add(r.len))
                    .is_some_and(|(x, y)| x <= y)
                && ((r.kind == MEM_IMAGE.0
                    && r.allocation == self.base
                    && matches!(r.protection, 2 | 4 | 8 | 32))
                    || (r.kind == MEM_PRIVATE.0 && matches!(r.protection, 2 | 4))),
            "Diagnostic read escaped image/private readable span"
        );
        let mut bytes = vec![0; n];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(
                self.handle.0,
                a as _,
                bytes.as_mut_ptr() as _,
                n,
                Some(&mut done),
            )?;
        }
        ensure!(done == n, "Partial diagnostic read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl Inspect for Native {
    fn region(&self, a: u64) -> Result<Region> {
        self.checkpoint()?;
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let count = unsafe {
            VirtualQueryEx(
                self.handle.0,
                Some(a as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            count == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Diagnostic VirtualQueryEx failed"
        );
        Ok(Region {
            start: info.BaseAddress as u64,
            len: info.RegionSize as u64,
            allocation: info.AllocationBase as u64,
            kind: info.Type.0,
            protection: info.Protect.0,
            committed: info.State == MEM_COMMIT,
        })
    }
}
fn d(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(
        b.get(o..o + 4)
            .context("Truncated probe DWORD")?
            .try_into()?,
    ))
}
fn q(b: &[u8], o: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(
        b.get(o..o + 8)
            .context("Truncated probe pointer")?
            .try_into()?,
    ))
}
fn hex(b: &[u8]) -> String {
    b.iter().map(|v| format!("{v:02X}")).collect()
}
fn model(m: &impl Inspect, s: &ball_v2::Snapshot, model: u32) -> Result<Value> {
    let entries = m.read(s.lease.registry, s.lease.registry_count as usize * 8)?;
    let mut matches = entries
        .chunks_exact(8)
        .filter(|e| d(e, 0).ok() == Some(model));
    let entry = matches
        .next()
        .context("Model absent from current registry")?;
    ensure!(
        matches.next().is_none(),
        "Model has duplicate registry rows"
    );
    let index = d(entry, 4)?;
    ensure!(index < 500, "Model index exceeds native bounds");
    let address = s.lease.records + u64::from(index) * ball::STRIDE as u64;
    let row = m.read(address, ball::STRIDE)?;
    ensure!(d(&row, 4)? == model, "Model record ID differs");
    let refs = m.read(
        m.base() + ball_v2::REFERENCES + 0x18 + u64::from(index) * 4,
        4,
    )?;
    let resource = m.read(
        m.base() + ball_v2::RENDERER + 0x28 + u64::from(index) * 48,
        48,
    )?;
    let record_again = m.read(address, ball::STRIDE)?;
    ensure!(
        row == record_again
            && entries == m.read(s.lease.registry, s.lease.registry_count as usize * 8)?,
        "Model/registry changed during observation"
    );
    Ok(
        json!({"model":format!("{model:08X}"),"index":index,"filename":ball::filename(&row)?,"references":i32::from_le_bytes(refs.as_slice().try_into()?),"resource":hex(&resource),"resourceSha256":format!("{:X}",Sha256::digest(&resource)),"slotAddress":format!("{:X}",m.base()+ball_v2::RENDERER+0x28+u64::from(index)*48),"meshPointer":format!("{:X}",q(&resource,16)?),"recordSha256":format!("{:X}",Sha256::digest(&row))}),
    )
}
fn queue(m: &impl Inspect) -> Result<Value> {
    for (rva, bytes) in [
        (0x21E704D, &[0x48, 0x63, 0x41, 0x08, 0x3B, 0x41, 0x0C][..]),
        (0x21E7062, &[0x4C, 0x8B, 0x8E, 0xE8, 0x5D, 0, 0][..]),
        (
            0x21E7069,
            &[
                0x48, 0x8D, 0x04, 0x80, 0x48, 0xC1, 0xE0, 0x05, 0x41, 0x89, 0x54, 0x01, 0x48,
            ][..],
        ),
        (
            0x21E7164,
            &[
                0x48, 0x8B, 0x86, 0xE8, 0x5D, 0, 0, 0x48, 0x89, 0x5C, 0x08, 0x40,
            ][..],
        ),
    ] {
        ensure!(
            m.read(m.base() + rva, bytes.len())? == bytes,
            "V2 queue diagnostic layout differs"
        );
    }
    let manager = m.base() + ball_v2::RENDERER;
    let header = m.read(manager, 0x28)?;
    let count = d(&header, 8)? as usize;
    let capacity = d(&header, 12)? as usize;
    ensure!(
        count <= capacity && capacity <= 8192 && count <= 128,
        "Render packet count is outside bounded diagnostic window"
    );
    let address = q(&m.read(manager + 0x5DE8, 8)?, 0)?;
    let bytes = if count == 0 {
        Vec::new()
    } else {
        ensure!(
            address >= 0x10000 && address % 8 == 0,
            "Render queue pointer invalid"
        );
        m.read(address, count * 0xA0)?
    };
    let packets=bytes.chunks_exact(0xA0).map(|b|Ok(json!({"model":format!("{:08X}",d(b,0x48)?),"resourceSlot":format!("{:X}",q(b,0x40)?)}))).collect::<Result<Vec<_>>>()?;
    ensure!(
        header == m.read(manager, 0x28)?
            && address == q(&m.read(manager + 0x5DE8, 8)?, 0)?
            && (count == 0 || bytes == m.read(address, count * 0xA0)?),
        "Render queue is moving; no coherent packet claim"
    );
    Ok(
        json!({"enabled":header[0x20],"count":count,"capacity":capacity,"packets":packets,"note":"Queue membership is diagnostic evidence, not proof of visual appearance"}),
    )
}
fn attempt(f: impl FnOnce() -> Result<Value>) -> Value {
    match f() {
        Ok(v) => v,
        Err(e) => json!({"unavailable":format!("{e:#}")}),
    }
}
pub(crate) fn inspect(m: &impl Inspect, id: &str, source: u32) -> Result<Value> {
    let s = ball_v2::capture(m)?;
    let wanted = id.strip_prefix("balls/").context("Missing ball path")?;
    let native = ball::canonical(s.native_ball);
    let normal = model(m, &s, ball::NORMAL)?;
    let selected = attempt(|| model(m, &s, source));
    let current = attempt(|| model(m, &s, native));
    let packets = attempt(|| queue(m));
    ensure!(
        rules_v2::context(m)? == s.context,
        "Game context changed during diagnostic"
    );
    Ok(
        json!({"schema":"nba2k27-v2-ball-readonly-probe-v1","readOnly":true,"writes":0,"selected":id,"selectedSourceModel":format!("{source:08X}"),"boundary":rules_v2::boundary(&s.context,m.base()),"context":{"current":format!("{:08X}",s.context.current),"alternate":format!("{:08X}",s.context.alternate),"stage":s.context.stage,"gameMode":s.context.game_mode,"active":s.context.active,"kind":s.context.kind},"nativeGameDataModel":format!("{:08X}",s.native_ball),"canonicalGameDataModel":format!("{native:08X}"),"nativeStillNormal":native==ball::NORMAL,"normalFilenameMatchesSelection":normal["filename"].as_str().is_some_and(|name|name.eq_ignore_ascii_case(wanted)),"normal":normal,"selectedSource":selected,"nativeModelRecord":current,"renderQueue":packets,"filenameConsumptionProven":false,"visualAppearanceProven":false}),
    )
}
/// Calling this function needs the user's fresh read-only live-test approval.
/// Tests NEVER invoke it. No diagnostic has been run automatically.
pub(crate) fn run(portable: &Path, pid: u32) -> Result<Value> {
    let e = expected(portable, pid)?;
    let native = Native::open(&e)?;
    let result = inspect(&native, &e.selected, e.source)?;
    native.checkpoint()?;
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> String {
        ball::choices()
            .iter()
            .find(|c| !ball::current(&c.id))
            .unwrap()
            .id
            .clone()
    }
    #[test]
    fn refs_and_mesh_ready_never_become_a_claim_of_selected_filename_or_visual_consumption() {
        let mut f = ball_v2::tests::fixture();
        let s = ball_v2::capture(&f).unwrap();
        f.patch(f.base() + ball_v2::REFERENCES + 0x18, &1u32.to_le_bytes());
        let mut resource = [0; 48];
        resource[16] = 1;
        f.patch(f.base() + ball_v2::RENDERER + 0x28, &resource);
        let v = inspect(&f, &id(), 0x12345678).unwrap();
        assert_eq!(v["normal"]["references"], 1);
        assert_eq!(v["filenameConsumptionProven"], false);
        assert_eq!(v["visualAppearanceProven"], false);
        assert_eq!(v["nativeStillNormal"], true);
        assert_eq!(v["normalFilenameMatchesSelection"], false);
        f.patch(s.context.owner + 0xDA4, &0x12345678u32.to_le_bytes());
        let v = inspect(&f, &id(), 0x12345678).unwrap();
        assert_eq!(v["nativeStillNormal"], false);
        assert_eq!(v["normal"]["references"], 1);
    }
    #[test]
    fn output_separates_patched_normal_filename_from_native_model_transition() {
        let mut f = ball_v2::tests::fixture();
        let s = ball_v2::capture(&f).unwrap();
        let wanted = id();
        let replacement =
            ball::replacement(&s.record, wanted.strip_prefix("balls/").unwrap()).unwrap();
        f.patch(s.lease.address, &replacement);
        f.patch(s.context.owner + 0xDA4, &0x12345678u32.to_le_bytes());
        let v = inspect(&f, &wanted, 0x12345678).unwrap();
        assert_eq!(v["normalFilenameMatchesSelection"], true);
        assert_eq!(v["nativeStillNormal"], false);
        assert_eq!(v["filenameConsumptionProven"], false);
    }
}
