//! Read-only port of WPF's provider-layout resolver
//! (nba2k27-version-proxy\src\provider_resolver.cpp, driven by
//! Reference\src\core\provider-layout-resolver.js).
//!
//! It reads the game executable FILE only: no process attach, no game-memory
//! access, no hooks and no provider registration. The only write is a small
//! cache report under Cache/provider-layouts, and only after every unique
//! structural check passed (WPF `report.ok` rule). Nothing here grants write
//! authority to any feature; native features keep their own build guards.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

pub const SCHEMA: &str = "nba2k27-provider-layout-v1";
const VTABLE_SLOTS: usize = 47;
const MAX_ANCHORS: usize = 1024;

struct Sig {
    bytes: &'static [u8],
    /// `?` marks a wildcard byte (WPF mask), `None` is an exact match.
    mask: Option<&'static [u8]>,
}
const fn exact(bytes: &'static [u8]) -> Sig {
    Sig { bytes, mask: None }
}
// Verbatim from provider_resolver.cpp:19-104.
const MANAGER_GETTER: Sig = exact(&[
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x58, 0x48, 0x8D, 0x6C, 0x24, 0x50, 0x48, 0xC7, 0x45, 0x00, 0xFE,
]);
const MANAGER_REGISTER: Sig = exact(&[
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x98,
    0x00, 0x00,
]);
const MANAGER_FIND: Sig = exact(&[
    0x55, 0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x40, 0x48, 0x8D, 0x6C, 0x24, 0x40, 0x48,
]);
const MANAGER_SELECT: Sig = exact(&[
    0x41, 0x56, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x20, 0x4C, 0x89, 0xCF, 0x4C, 0x89, 0xC6,
]);
const REFERENCE_RELEASE: Sig = Sig {
    bytes: &[
        0x56, 0x57, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x89, 0xCE, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x39,
        0x46,
    ],
    mask: Some(b"xxxxxxxxxx????xx"),
};
const BASE_CONSTRUCTOR: Sig = Sig {
    bytes: &[
        0x56, 0x48, 0x83, 0xEC, 0x20, 0x48, 0x89, 0xCE, 0x48, 0x8D, 0x05, 0x00, 0x00, 0x00, 0x00,
        0x48,
    ],
    mask: Some(b"xxxxxxxxxxx????x"),
};
const SET_DEVICE_ID: Sig = exact(&[
    0x48, 0x89, 0xD0, 0x45, 0x85, 0xC0, 0x74, 0x0C, 0x44, 0x89, 0x41, 0x40, 0xB9, 0xA8, 0x21, 0x45,
    0x50, 0x89, 0x08, 0xC3,
]);
const BASE_DESTRUCTOR: Sig = exact(&[
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x28, 0x48, 0x8D, 0x6C, 0x24, 0x20, 0x48, 0xC7, 0x45, 0x00, 0xFE,
]);
const PATH_BUILDER: Sig = exact(&[
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x38,
    0x45, 0x89, 0xCE, 0x44, 0x89, 0xC5, 0x89, 0xD3, 0x8B, 0xBC, 0x24, 0xA8, 0x00, 0x00, 0x00, 0x48,
    0x8B, 0xB4, 0x24, 0xA0, 0x00, 0x00, 0x00, 0x83, 0xFA, 0x01,
]);
const RESOURCE_EXISTS: Sig = exact(&[
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0xF0, 0x00, 0x00,
]);
const BASE_EXISTS: Sig = exact(&[
    0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x60, 0x01, 0x00, 0x00, 0x48, 0x89, 0xD6, 0x48, 0x85, 0xC9,
]);
const BASE_EXISTS_OPEN_CALL: Sig = exact(&[
    0xFF, 0x90, 0x08, 0x01, 0x00, 0x00, 0x81, 0x7C, 0x24, 0x2C, 0xA8, 0x21, 0x45, 0x50,
]);
const OPEN_RESULT_CONSTRUCTOR: Sig = exact(&[
    0x48, 0xC7, 0x01, 0x00, 0x00, 0x00, 0x00, 0xC5, 0xF8, 0x57, 0xC0, 0xC5, 0xFC, 0x11, 0x41, 0x18,
]);
const MANAGER_OPEN: Sig = exact(&[
    0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x53, 0x48, 0x81, 0xEC, 0x68,
]);
const MANAGER_OPEN_DEVICE_CALL: Sig = exact(&[
    0x48, 0x8B, 0x80, 0x18, 0x01, 0x00, 0x00, 0x44, 0x89, 0x6C, 0x24, 0x30, 0x8B,
]);
const FILE_HANDLE_CONSTRUCTOR: Sig = exact(&[
    0x56, 0x48, 0x83, 0xEC, 0x30, 0xC5, 0xF8, 0x29, 0x74, 0x24, 0x20, 0x48, 0x89, 0xCE, 0x48, 0xC7,
]);
const FILE_HANDLE_INITIALIZE: Sig = exact(&[
    0x41, 0x56, 0x56, 0x57, 0x53, 0x48, 0x83, 0xEC, 0x28, 0x4C, 0x89, 0xCE, 0x4C, 0x89, 0xC3, 0x49,
]);
const FILE_HANDLE_READ: Sig = exact(&[
    0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56, 0x57, 0x55, 0x53, 0x48, 0x83, 0xEC, 0x58,
]);
const FILE_HANDLE_DESTRUCTOR: Sig = exact(&[
    0x55, 0x56, 0x48, 0x83, 0xEC, 0x38, 0x48, 0x8D, 0x6C, 0x24, 0x30, 0x48, 0xC7, 0x45, 0x00, 0xFE,
]);

fn matches(candidate: &[u8], sig: &Sig) -> bool {
    candidate.len() >= sig.bytes.len()
        && sig
            .bytes
            .iter()
            .enumerate()
            .all(|(i, b)| sig.mask.is_some_and(|m| m[i] == b'?') || candidate[i] == *b)
}

#[derive(Clone, Copy)]
struct Section {
    name: [u8; 8],
    va: u32,
    virtual_size: u32,
    raw: u32,
    raw_size: u32,
    flags: u32,
}

/// Executable file mapped read-only, as WPF's `ExecutableFile`
/// (provider_resolver.cpp:106-239). The V2 image has a ~700 MB executable
/// section, so it is mapped rather than copied into memory.
pub(crate) struct Image {
    _file: fs::File,
    mapping: windows::Win32::Foundation::HANDLE,
    view: windows::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
    len: usize,
    headers: u32,
    pub timestamp: u32,
    pub image_size: u32,
    image_base: u64,
    sections: Vec<Section>,
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::System::Memory::UnmapViewOfFile(self.view);
            let _ = windows::Win32::Foundation::CloseHandle(self.mapping);
        }
    }
}
impl Image {
    pub fn open(path: &Path) -> Result<Self> {
        use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
        use windows::Win32::{Foundation::HANDLE, System::Memory::*};
        let parse = "unable to open or parse executable";
        // FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, as WPF.
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .open(path)
            .context(parse)?;
        let len = file.metadata()?.len();
        ensure!((4096..=4 * 1024 * 1024 * 1024).contains(&len), parse);
        let mapping = unsafe {
            CreateFileMappingW(
                HANDLE(file.as_raw_handle()),
                None,
                PAGE_READONLY,
                0,
                0,
                windows::core::PCWSTR::null(),
            )
        }
        .context(parse)?;
        let view = unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, 0) };
        if view.Value.is_null() {
            unsafe {
                let _ = windows::Win32::Foundation::CloseHandle(mapping);
            }
            anyhow::bail!(parse);
        }
        let mut image = Self {
            _file: file,
            mapping,
            view,
            len: len as usize,
            headers: 0,
            timestamp: 0,
            image_size: 0,
            image_base: 0,
            sections: vec![],
        };
        let h: [u8; 4096] = image.bytes()[..4096].try_into()?;
        ensure!(&h[..2] == b"MZ", parse);
        let pe = u32::from_le_bytes(h[0x3C..0x40].try_into()?) as usize;
        ensure!(
            pe >= 0x40 && pe + 24 + 112 <= 4096 && &h[pe..pe + 4] == b"PE\0\0",
            parse
        );
        ensure!(
            u16::from_le_bytes(h[pe + 24..pe + 26].try_into()?) == 0x20B,
            parse
        );
        let count = u16::from_le_bytes(h[pe + 6..pe + 8].try_into()?) as usize;
        let optional = u16::from_le_bytes(h[pe + 20..pe + 22].try_into()?) as usize;
        let start = pe + 24 + optional;
        ensure!(
            (1..=96).contains(&count) && start + count * 40 <= h.len(),
            parse
        );
        let d = |o: usize| u32::from_le_bytes(h[o..o + 4].try_into().unwrap());
        let sections = (0..count)
            .map(|i| {
                let o = start + i * 40;
                Section {
                    name: h[o..o + 8].try_into().unwrap(),
                    virtual_size: d(o + 8),
                    va: d(o + 12),
                    raw_size: d(o + 16),
                    raw: d(o + 20),
                    flags: d(o + 36),
                }
            })
            .collect();
        image.headers = d(pe + 24 + 60);
        image.timestamp = d(pe + 8);
        image.image_size = d(pe + 24 + 56);
        image.image_base = u64::from_le_bytes(h[pe + 24 + 24..pe + 24 + 32].try_into()?);
        image.sections = sections;
        Ok(image)
    }
    fn bytes(&self) -> &[u8] {
        // The view covers the whole file and lives as long as `self`.
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.len) }
    }
    fn at(&self, rva: u32, n: usize) -> Option<&[u8]> {
        let raw = if rva < self.headers {
            rva as usize
        } else {
            let s = self.sections.iter().find(|s| {
                rva >= s.va && rva as u64 + n as u64 <= s.va as u64 + s.raw_size as u64
            })?;
            s.raw as usize + (rva - s.va) as usize
        };
        self.bytes().get(raw..raw.checked_add(n)?)
    }
    /// Executable sections as (name, RVA, raw bytes), for signature relocation.
    pub(crate) fn code_sections(&self) -> Vec<(String, u32, &[u8])> {
        self.sections
            .iter()
            .filter(|s| s.flags & 0x2000_0000 != 0)
            .filter_map(|s| {
                let raw = self
                    .bytes()
                    .get(s.raw as usize..s.raw as usize + s.raw_size as usize)?;
                let name = String::from_utf8_lossy(&s.name)
                    .trim_end_matches('\0')
                    .to_string();
                Some((name, s.va, raw))
            })
            .collect()
    }
    /// Name of the section holding `rva`.
    pub(crate) fn section_name(&self, rva: u32) -> Option<String> {
        self.sections
            .iter()
            .find(|s| {
                rva >= s.va && (rva as u64) < s.va as u64 + s.virtual_size.max(s.raw_size) as u64
            })
            .map(|s| {
                String::from_utf8_lossy(&s.name)
                    .trim_end_matches('\0')
                    .to_string()
            })
    }
    /// Section flags for `rva` (writable 0x80000000, executable 0x20000000).
    pub(crate) fn section_flags(&self, rva: u32) -> Option<u32> {
        self.sections
            .iter()
            .find(|s| {
                rva >= s.va && (rva as u64) < s.va as u64 + s.virtual_size.max(s.raw_size) as u64
            })
            .map(|s| s.flags)
    }
    pub(crate) fn read_rva(&self, rva: u32, n: usize) -> Option<&[u8]> {
        self.at(rva, n)
    }
    fn executable(&self, rva: u32) -> bool {
        self.sections
            .iter()
            .find(|s| {
                rva >= s.va && (rva as u64) < s.va as u64 + s.virtual_size.max(s.raw_size) as u64
            })
            .is_some_and(|s| s.flags & 0x2000_0000 != 0)
    }
    fn scan(&self, sig: &Sig) -> (usize, Vec<u32>) {
        let (mut count, mut hits) = (0usize, Vec::new());
        let first = sig.bytes[0];
        for s in self.sections.iter().filter(|s| s.flags & 0x2000_0000 != 0) {
            let Some(bytes) = self
                .bytes()
                .get(s.raw as usize..s.raw as usize + s.raw_size as usize)
            else {
                continue;
            };
            if bytes.len() < sig.bytes.len() {
                continue;
            }
            let last = bytes.len() - sig.bytes.len();
            let mut i = 0;
            while i <= last {
                match bytes[i..=last].iter().position(|b| *b == first) {
                    None => break,
                    Some(p) => {
                        let o = i + p;
                        if matches(&bytes[o..], sig) {
                            if hits.len() < MAX_ANCHORS {
                                hits.push(s.va + o as u32);
                            }
                            count += 1;
                        }
                        i = o + 1;
                    }
                }
            }
        }
        (count, hits)
    }
    fn matches_at(&self, rva: u32, sig: &Sig) -> bool {
        self.at(rva, sig.bytes.len())
            .is_some_and(|b| matches(b, sig))
    }
    fn call_target(&self, rva: u32) -> Option<u32> {
        let b = self.at(rva, 5)?;
        if b[0] != 0xE8 {
            return None;
        }
        let d = i32::from_le_bytes(b[1..5].try_into().ok()?) as i64;
        u32::try_from(rva as i64 + 5 + d).ok()
    }
}

/// The 20 RVAs WPF's provider host consumed (`provider-layout-resolver.js:39-41`).
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    pub manager_getter: u32,
    pub manager_register: u32,
    pub manager_find: u32,
    pub manager_select: u32,
    pub reference_release: u32,
    pub base_constructor: u32,
    pub set_device_id: u32,
    pub base_destructor: u32,
    pub base_vtable: u32,
    pub path_builder: u32,
    pub resource_exists_by_id: u32,
    pub base_exists: u32,
    pub base_exists_open_call: u32,
    pub open_result_constructor: u32,
    pub manager_open: u32,
    pub manager_open_device_call: u32,
    pub file_handle_constructor: u32,
    pub file_handle_initialize: u32,
    pub file_handle_read: u32,
    pub file_handle_destructor: u32,
}
impl Layout {
    fn entries(&self) -> [(&'static str, u32); 20] {
        [
            ("managerGetter", self.manager_getter),
            ("managerRegister", self.manager_register),
            ("managerFind", self.manager_find),
            ("managerSelect", self.manager_select),
            ("referenceRelease", self.reference_release),
            ("baseConstructor", self.base_constructor),
            ("setDeviceId", self.set_device_id),
            ("baseDestructor", self.base_destructor),
            ("baseVtable", self.base_vtable),
            ("pathBuilder", self.path_builder),
            ("resourceExistsById", self.resource_exists_by_id),
            ("baseExists", self.base_exists),
            ("baseExistsOpenCall", self.base_exists_open_call),
            ("openResultConstructor", self.open_result_constructor),
            ("managerOpen", self.manager_open),
            ("managerOpenDeviceCall", self.manager_open_device_call),
            ("fileHandleConstructor", self.file_handle_constructor),
            ("fileHandleInitialize", self.file_handle_initialize),
            ("fileHandleRead", self.file_handle_read),
            ("fileHandleDestructor", self.file_handle_destructor),
        ]
    }
}

#[derive(Default)]
pub(crate) struct Report {
    pub ok: bool,
    pub failure: &'static str,
    pub timestamp: u32,
    pub image_size: u32,
    pub manager_anchor_matches: usize,
    pub manager_cluster_candidates: usize,
    pub base_anchor_matches: usize,
    pub base_cluster_candidates: usize,
    pub path_candidates: usize,
    pub reference_release_candidates: usize,
    pub executable_vtable_targets: usize,
    pub layout: Layout,
    pub vtable: Vec<u32>,
}
fn rva(v: u32) -> String {
    format!("0x{v:08X}")
}
impl Report {
    pub fn to_json(&self) -> Value {
        let layout: serde_json::Map<String, Value> = self
            .layout
            .entries()
            .iter()
            .map(|(k, v)| ((*k).into(), json!(rva(*v))))
            .collect();
        json!({
            "schema": SCHEMA, "ok": self.ok, "failure": self.failure,
            "timestamp": rva(self.timestamp), "imageSize": rva(self.image_size),
            "checks": {
                "managerAnchorMatches": self.manager_anchor_matches,
                "managerClusterCandidates": self.manager_cluster_candidates,
                "baseAnchorMatches": self.base_anchor_matches,
                "baseClusterCandidates": self.base_cluster_candidates,
                "pathCandidates": self.path_candidates,
                "referenceReleaseCandidates": self.reference_release_candidates,
                "executableVtableTargets": self.executable_vtable_targets,
            },
            "layout": layout,
            "vtable": self.vtable.iter().map(|v| rva(*v)).collect::<Vec<_>>(),
        })
    }
}

fn manager_candidate(f: &Image, select: u32) -> Option<Layout> {
    let sub = |a: u32| select.checked_sub(a);
    let c = Layout {
        manager_select: select,
        manager_getter: sub(0x1080)?,
        manager_register: sub(0x07C0)?,
        manager_find: sub(0x02E0)?,
        file_handle_constructor: sub(0x56E0)?,
        file_handle_destructor: sub(0x5230)?,
        file_handle_initialize: sub(0x4DB0)?,
        file_handle_read: sub(0x4AC0)?,
        resource_exists_by_id: select.checked_add(0x2990)?,
        manager_open: select.checked_add(0x3B20)?,
        manager_open_device_call: select.checked_add(0x444E)?,
        ..Default::default()
    };
    let ok = f.matches_at(c.manager_getter, &MANAGER_GETTER)
        && f.matches_at(c.manager_register, &MANAGER_REGISTER)
        && f.matches_at(c.manager_find, &MANAGER_FIND)
        && f.matches_at(c.manager_select, &MANAGER_SELECT)
        && f.matches_at(c.resource_exists_by_id, &RESOURCE_EXISTS)
        && f.matches_at(c.manager_open, &MANAGER_OPEN)
        && f.matches_at(c.manager_open_device_call, &MANAGER_OPEN_DEVICE_CALL)
        && f.matches_at(c.file_handle_constructor, &FILE_HANDLE_CONSTRUCTOR)
        && f.matches_at(c.file_handle_destructor, &FILE_HANDLE_DESTRUCTOR)
        && f.matches_at(c.file_handle_initialize, &FILE_HANDLE_INITIALIZE)
        && f.matches_at(c.file_handle_read, &FILE_HANDLE_READ)
        && f.call_target(select + 0x26) == Some(c.manager_find)
        && f.call_target(select + 0xC3) == Some(c.manager_find);
    ok.then_some(c)
}
fn base_candidate(f: &Image, set: u32, base: Layout) -> Option<Layout> {
    let mut c = base;
    c.set_device_id = set;
    c.base_constructor = set.checked_sub(0x580)?;
    c.base_destructor = set.checked_sub(0x330)?;
    c.base_exists = set.checked_sub(0x3C0)?;
    c.base_exists_open_call = c.base_exists + 0x55;
    c.open_result_constructor = set.checked_add(0xF0)?;
    let ok = f.matches_at(c.base_constructor, &BASE_CONSTRUCTOR)
        && f.matches_at(c.base_destructor, &BASE_DESTRUCTOR)
        && f.matches_at(c.base_exists, &BASE_EXISTS)
        && f.matches_at(c.base_exists_open_call, &BASE_EXISTS_OPEN_CALL)
        && f.matches_at(c.open_result_constructor, &OPEN_RESULT_CONSTRUCTOR)
        && f.call_target(c.base_destructor + 0x20) == Some(base.manager_getter);
    if !ok {
        return None;
    }
    let lea = f.at(c.base_constructor + 8, 7)?;
    if lea[..3] != [0x48, 0x8D, 0x05] {
        return None;
    }
    let d = i32::from_le_bytes(lea[3..7].try_into().ok()?) as i64;
    c.base_vtable = u32::try_from(c.base_constructor as i64 + 0x0F + d).ok()?;
    Some(c)
}
fn vtable(f: &Image, r: &mut Report) -> bool {
    let Some(table) = f.at(r.layout.base_vtable, VTABLE_SLOTS * 8) else {
        return false;
    };
    r.vtable.clear();
    for slot in table.chunks_exact(8) {
        let address = u64::from_le_bytes(slot.try_into().unwrap());
        let Some(v) = address
            .checked_sub(f.image_base)
            .and_then(|v| u32::try_from(v).ok())
        else {
            return false;
        };
        r.vtable.push(v);
        if !f.executable(v) {
            return false;
        }
    }
    r.executable_vtable_targets = r.vtable.len();
    let (l, t) = (&r.layout, &r.vtable);
    t[20] == l.base_constructor + 0x50
        && t[32] == l.base_exists
        && t[35] == l.base_constructor + 0xB0
        && t[42] == l.base_constructor + 0x240
        && t[43] == l.base_constructor + 0x1A0
}

/// `resolve_provider_layout` (provider_resolver.cpp:374-456).
pub(crate) fn resolve(path: &Path) -> Report {
    let mut r = Report {
        failure: "unable to open or parse executable",
        ..Default::default()
    };
    let Ok(f) = Image::open(path) else {
        return r;
    };
    r.timestamp = f.timestamp;
    r.image_size = f.image_size;
    let (count, hits) = f.scan(&MANAGER_SELECT);
    r.manager_anchor_matches = count;
    if count > MAX_ANCHORS {
        r.failure = "too many manager anchors";
        return r;
    }
    for hit in hits {
        if let Some(c) = manager_candidate(&f, hit) {
            r.manager_cluster_candidates += 1;
            r.layout = c;
        }
    }
    if r.manager_cluster_candidates != 1 {
        r.failure = "manager cluster is ambiguous or missing";
        return r;
    }
    let (count, hits) = f.scan(&SET_DEVICE_ID);
    r.base_anchor_matches = count;
    if count > MAX_ANCHORS {
        r.failure = "too many base-device anchors";
        return r;
    }
    let manager = r.layout;
    for hit in hits {
        if let Some(c) = base_candidate(&f, hit, manager) {
            r.base_cluster_candidates += 1;
            r.layout = c;
        }
    }
    if r.base_cluster_candidates != 1 {
        r.failure = "base-device cluster is ambiguous or missing";
        return r;
    }
    let (count, hits) = f.scan(&PATH_BUILDER);
    r.path_candidates = count;
    if count == 1 {
        r.layout.path_builder = hits[0];
    }
    let (count, hits) = f.scan(&REFERENCE_RELEASE);
    r.reference_release_candidates = count;
    if count == 1 {
        r.layout.reference_release = hits[0];
    }
    if r.path_candidates != 1 || r.reference_release_candidates != 1 {
        r.failure = "supporting anchors are ambiguous or missing";
        return r;
    }
    let l = r.layout;
    if f.call_target(l.path_builder + 0xD3) != Some(l.manager_getter)
        || f.call_target(l.path_builder + 0xDE).map(|t| t + 0x10) != Some(l.resource_exists_by_id)
    {
        r.failure = "path-builder relationships did not validate";
        return r;
    }
    if !vtable(&f, &mut r) {
        r.failure = "base-device vtable did not validate";
        return r;
    }
    r.ok = true;
    r.failure = "none";
    r
}

/// WPF `validateResolverReport` success rule (`provider-layout-resolver.js:24-55`):
/// success must carry unique structural validation, or it is rejected.
pub(crate) fn proven(report: &Value) -> Result<bool> {
    ensure!(report["schema"] == SCHEMA, "Unsupported resolver report");
    let ok = report["ok"]
        .as_bool()
        .context("Unsupported resolver report")?;
    if ok {
        let c = &report["checks"];
        ensure!(
            [
                "managerClusterCandidates",
                "baseClusterCandidates",
                "pathCandidates",
                "referenceReleaseCandidates"
            ]
            .iter()
            .all(|k| c[*k] == 1)
                && c["executableVtableTargets"] == VTABLE_SLOTS
                && report["layout"].as_object().is_some_and(|m| m.len() == 20)
                && report["vtable"]
                    .as_array()
                    .is_some_and(|v| v.len() == VTABLE_SLOTS),
            "Provider resolver reported success without unique structural validation."
        );
    }
    Ok(ok)
}

#[derive(Clone)]
struct Cached {
    exe: PathBuf,
    size: u64,
    modified: u128,
    state: Value,
}
static STATE: Mutex<Option<Cached>> = Mutex::new(None);

fn identity(exe: &Path) -> Result<(u64, u128, String)> {
    let meta = fs::metadata(exe)?;
    let modified = meta.modified()?.duration_since(UNIX_EPOCH)?;
    Ok((
        meta.len(),
        modified.as_millis(),
        format!("{}", modified.as_secs()),
    ))
}
fn cache_dir(root: &Path) -> PathBuf {
    root.join("Cache/provider-layouts")
}

/// `refreshProviderLayout` (main.js:764-806): one result per executable
/// identity; `force` rescans (WPF "Rescan executable", useCache:false).
pub(crate) fn status(root: &Path, game_root: &Path, force: bool) -> Result<Value> {
    let exe = game_root.join("NBA2K27.exe");
    if !exe.is_file() {
        return Ok(
            json!({"status":"blocked","message":"Choose the installed NBA 2K27 folder.","report":null}),
        );
    }
    let (size, modified, stamp) = identity(&exe)?;
    if !force {
        if let Some(c) = STATE.lock().ok().and_then(|s| s.clone()) {
            if c.exe == exe && c.size == size && c.modified == modified {
                return Ok(c.state);
            }
        }
        if let Some(report) = read_cache(root, &exe, size, &stamp) {
            let state = view(report, "verified-cache");
            remember(&exe, size, modified, &state);
            return Ok(state);
        }
    }
    let mut report = resolve(&exe).to_json();
    let ok = proven(&report)?;
    report["gameExecutable"] = json!(exe);
    report["fileSize"] = json!(size);
    report["fileModifiedAt"] = json!(stamp);
    report["verifiedAt"] = json!(crate::launcher_health::now_iso());
    report["source"] = json!("automatic-local-scan");
    if ok {
        let _ = write_cache(root, &report);
    }
    let state = view(report, "automatic-local-scan");
    remember(&exe, size, modified, &state);
    Ok(state)
}
fn remember(exe: &Path, size: u64, modified: u128, state: &Value) {
    if let Ok(mut s) = STATE.lock() {
        *s = Some(Cached {
            exe: exe.into(),
            size,
            modified,
            state: state.clone(),
        });
    }
}
fn view(mut report: Value, source: &str) -> Value {
    report["source"] = json!(source);
    let ok = report["ok"] == true;
    let pinned = pinned_comparison(&report);
    json!({
        "status": if ok {"ready"} else {"blocked"},
        "message": if ok {
            "Current build resolved automatically and passed every structural check.".to_string()
        } else {
            format!("Automatic resolver stopped safely: {}", report["failure"].as_str().unwrap_or("unknown"))
        },
        "pinned": pinned,
        "report": report,
    })
}
/// Rust-only addition: compare the resolved build identity to the builds this
/// port's native guards accept. Feature admission stays with those guards.
fn pinned_comparison(report: &Value) -> Value {
    let key = format!(
        "{}:{}",
        report["timestamp"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches("0x"),
        report["imageSize"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches("0x")
    );
    let known = crate::launcher_health::known_build(&key);
    json!({
        "build": key,
        "recognized": known.is_some(),
        "label": known.unwrap_or("Unrecognized game update"),
        "message": match (known.is_some(), report["ok"] == true) {
            (true, _) => "This build matches a profile the launcher's native guards accept.",
            (false, true) => "Game update detected. The file-provider structure is intact, but native features stay off until a profile for this build is verified. Nothing will be written.",
            (false, false) => "Game update detected and the provider structure could not be proven. Nothing will be written.",
        },
    })
}
fn read_cache(root: &Path, exe: &Path, size: u64, stamp: &str) -> Option<Value> {
    let mut best: Option<(std::time::SystemTime, Value)> = None;
    for entry in fs::read_dir(cache_dir(root)).ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let valid = name.len() == 22
            && name.ends_with(".json")
            && name[..17].bytes().enumerate().all(|(i, b)| {
                if i == 8 {
                    b == b'-'
                } else {
                    b.is_ascii_hexdigit() && !b.is_ascii_lowercase()
                }
            });
        let meta = entry.metadata().ok()?;
        if !valid || !meta.is_file() || meta.len() > 256 * 1024 {
            continue;
        }
        let Ok(v) = serde_json::from_slice::<Value>(&fs::read(entry.path()).ok()?) else {
            continue;
        };
        let same = proven(&v).unwrap_or(false)
            && v["gameExecutable"]
                .as_str()
                .is_some_and(|p| p.eq_ignore_ascii_case(&exe.to_string_lossy()))
            && v["fileSize"] == size
            && v["fileModifiedAt"] == stamp;
        let at = meta.modified().ok()?;
        if same && best.as_ref().is_none_or(|(t, _)| at > *t) {
            best = Some((at, v));
        }
    }
    best.map(|(_, v)| v)
}
fn write_cache(root: &Path, report: &Value) -> Result<()> {
    let dir = cache_dir(root);
    fs::create_dir_all(&dir)?;
    let name = format!(
        "{}-{}.json",
        report["timestamp"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches("0x"),
        report["imageSize"]
            .as_str()
            .unwrap_or("")
            .trim_start_matches("0x")
    );
    crate::backend::atomic_json(&dir.join(name), report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn masks_and_success_rule_match_wpf() {
        let mut b = REFERENCE_RELEASE.bytes.to_vec();
        b[10..14].copy_from_slice(&[1, 2, 3, 4]);
        assert!(matches(&b, &REFERENCE_RELEASE));
        b[0] = 0;
        assert!(!matches(&b, &REFERENCE_RELEASE));
        let mut r = Report {
            ok: true,
            failure: "none",
            ..Default::default()
        }
        .to_json();
        assert!(
            proven(&r).is_err(),
            "success without unique checks is rejected"
        );
        r["ok"] = json!(false);
        assert_eq!(proven(&r).unwrap(), false);
    }
    #[test]
    fn missing_executable_fails_closed() {
        let r = resolve(Path::new(r"C:\definitely\missing\NBA2K27.exe"));
        assert!(!r.ok);
        assert_eq!(r.failure, "unable to open or parse executable");
    }
    /// Read-only check against the installed game, when present.
    #[test]
    #[ignore]
    fn installed_game_report() {
        let exe = Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe");
        let r = resolve(exe);
        println!("{}", serde_json::to_string_pretty(&r.to_json()).unwrap());
    }
}
