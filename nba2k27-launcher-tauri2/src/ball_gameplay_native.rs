//! Actual native child capability of ball_gameplay_runtime. No arbitrary writer,
//! remote code/allocation, protection changes, hooks or game engine calls.
use super::*;
use crate::{
    ball_gameplay_v2::Permit,
    blacktop_arena::{Inspect, Region},
    blacktop_native::{self, Memory, Reader},
};
use std::mem::size_of;
use windows::{
    Win32::{
        Foundation::*,
        System::{
            Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory},
            Memory::*,
            Threading::*,
        },
    },
    core::PWSTR,
};
const READ: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(PROCESS_QUERY_INFORMATION.0 | PROCESS_VM_READ.0 | PROCESS_SYNCHRONIZE.0);
const WRITE: PROCESS_ACCESS_RIGHTS =
    PROCESS_ACCESS_RIGHTS(READ.0 | PROCESS_VM_WRITE.0 | PROCESS_VM_OPERATION.0);
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
fn retained(handle: HANDLE, id: &Identity) -> Result<bool> {
    let alive = match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        _ => anyhow::bail!("Gameplay ball retained process wait is unknown"),
    };
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
    }
    ensure!(
        (((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64).to_string() == id.started,
        "Gameplay ball retained game birth differs"
    );
    if !alive {
        return Ok(false);
    }
    let mut text = vec![0u16; 32768];
    let mut n = text.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(text.as_mut_ptr()), &mut n)?;
    }
    ensure!(
        n > 0
            && (n as usize) < text.len()
            && crate::native_session::same_path(
                &String::from_utf16(&text[..n as usize])?,
                Path::new(&id.path)
            ),
        "Gameplay ball retained game executable differs"
    );
    Ok(true)
}
/// Per-read liveness proof. A handle is bound to one process object for its
/// whole lifetime, so its image path cannot change after `retained` proved it
/// (open and every guard); each read still proves alive and the exact birth.
fn alive(handle: HANDLE, id: &Identity) -> Result<bool> {
    let alive = match unsafe { WaitForSingleObject(handle, 0) } {
        WAIT_TIMEOUT => true,
        WAIT_OBJECT_0 => false,
        _ => anyhow::bail!("Gameplay ball retained process wait is unknown"),
    };
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        GetProcessTimes(handle, &mut c, &mut e, &mut k, &mut u)?;
    }
    ensure!(
        (((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64).to_string() == id.started,
        "Gameplay ball retained game birth differs"
    );
    Ok(alive)
}
fn guard_rows(
    rows: &[session_safety::Process],
    b: &Binding,
    parent_required: bool,
    parent: Option<&Identity>,
) -> Result<()> {
    if parent_required {
        ensure!(
            parent == Some(&b.parent),
            "Gameplay ball launch owner exited or changed"
        );
    }
    let others = rows
        .iter()
        .filter(|p| {
            !(session_safety::launcher_process(p) && p.identity.as_ref() == Some(&b.parent))
        })
        .map(|p| session_safety::Process {
            name: p.name.clone(),
            pid: p.pid,
            parent: p.parent,
            identity: p.identity.clone(),
        })
        .collect::<Vec<_>>();
    blacktop_native::guard(&others, &b.game)
}
/// The exact stock manifest file a selected row was proven in. WPF read the
/// manifest once per helper run; this proof is reused only while the same
/// file (persistent id), length and last-write time are observed again.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ManifestKey {
    identity: crate::file_identity::Stable,
    len: u64,
    written: u64,
}
#[derive(Default)]
struct ManifestProofs {
    key: Option<ManifestKey>,
    selected: Vec<String>,
}
impl ManifestProofs {
    fn proven(&self, key: &ManifestKey, selected: &str) -> bool {
        self.key.as_ref() == Some(key) && self.selected.iter().any(|s| s == selected)
    }
    fn remember(&mut self, key: ManifestKey, selected: &str) {
        if self.key.as_ref() != Some(&key) {
            self.key = Some(key);
            self.selected.clear();
        }
        if !self.selected.iter().any(|s| s == selected) && self.selected.len() < 64 {
            self.selected.push(selected.into());
        }
    }
}
pub(super) struct Native {
    reader: Reader,
    query: Handle,
    binding: Binding,
    manifest: std::cell::RefCell<ManifestProofs>,
}
fn manifest_key(file: &File, meta: &fs::Metadata) -> Result<ManifestKey> {
    ensure!(
        meta.is_file()
            && meta.file_attributes() & 0x400 == 0
            && (1..=64 * 1024 * 1024).contains(&meta.len()),
        "Gameplay-ball stock manifest is not a bounded ordinary file"
    );
    Ok(ManifestKey {
        identity: crate::file_identity::Stable::read(file)?,
        len: meta.len(),
        written: meta.last_write_time(),
    })
}
impl Native {
    /// Every forward check still opens the pinned stock manifest; only the
    /// 46 MB re-read is skipped while the exact same file is unchanged.
    pub(super) fn source_available(&self, selected: &str) -> Result<()> {
        let _pins = crate::loader_files::Directories::pin(&self.binding.game_root)?;
        let file = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .custom_flags(0x00200000)
            .open(self.binding.game_root.join("manifest"))?;
        let key = manifest_key(&file, &file.metadata()?)?;
        if self
            .manifest
            .try_borrow()
            .is_ok_and(|p| p.proven(&key, selected))
        {
            return Ok(());
        }
        let mut bytes = Vec::new();
        (&file).take(64 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == key.len && manifest_key(&file, &file.metadata()?)? == key,
            "Gameplay-ball manifest changed during reading"
        );
        crate::ball_gameplay_v2::validate_manifest(&bytes, selected)?;
        if let Ok(mut proofs) = self.manifest.try_borrow_mut() {
            proofs.remember(key, selected);
        }
        Ok(())
    }
    pub(super) fn open(binding: &Binding) -> Result<Self> {
        guard_rows(
            &session_safety::processes()?,
            binding,
            true,
            session_safety::identity(binding.parent.pid).ok().as_ref(),
        )?;
        let reader = Reader::open_observer(&binding.game, &binding.game_root)?;
        let query = Handle(unsafe { OpenProcess(READ, false, binding.game.pid)? });
        ensure!(
            retained(query.0, &binding.game)?,
            "Gameplay ball game ended before native initialization"
        );
        let n = Self {
            reader,
            query,
            binding: binding.clone(),
            manifest: Default::default(),
        };
        n.guard(true)?;
        crate::ball_gameplay_v2::verify_profile(&n)?;
        Ok(n)
    }
    fn guard(&self, parent_required: bool) -> Result<()> {
        self.checkpoint()?;
        guard_rows(
            &session_safety::processes()?,
            &self.binding,
            parent_required,
            session_safety::identity(self.binding.parent.pid)
                .ok()
                .as_ref(),
        )?;
        let fresh = Reader::open_observer(&self.binding.game, &self.binding.game_root)?;
        ensure!(
            fresh.base() == self.base(),
            "Gameplay ball first module identity changed"
        );
        Ok(())
    }
    pub(super) fn ended(&self) -> Result<()> {
        ensure!(
            !retained(self.query.0, &self.binding.game)?,
            "Exact retained ball_gameplay game is still alive"
        );
        Ok(())
    }
    fn writable(&self, p: &Permit<'_>) -> Result<Region> {
        let c = p.change()?;
        let r = self.region(c.address)?;
        ensure!(
            (1..=128).contains(&c.before.len())
                && c.after.len() == c.before.len()
                && c.before != c.after
                && writable_span(r, c.address, c.before.len(), p.allocation()),
            "Gameplay ball publication escaped exact private RW court-record field"
        );
        Ok(r)
    }
}
fn writable_span(r: Region, address: u64, len: usize, allocation: u64) -> bool {
    address >= 0x10000
        && address % 4 == 0
        && (1..=128).contains(&len)
        && r.committed
        && r.kind == MEM_PRIVATE.0
        && r.protection == PAGE_READWRITE.0
        && r.allocation == allocation
        && allocation >= 0x10000
        && allocation % 4096 == 0
        && allocation <= r.start
        && address >= r.start
        && address
            .checked_add(len as u64)
            .zip(r.start.checked_add(r.len))
            .is_some_and(|(a, b)| a <= b)
}
impl Memory for Native {
    fn base(&self) -> u64 {
        self.reader.base()
    }
    fn checkpoint(&self) -> Result<()> {
        // `retained` (image path) ran at open and runs at every guard; the
        // per-read proof is liveness plus exact birth on that same handle.
        ensure!(
            alive(self.query.0, &self.binding.game)?,
            "Gameplay ball retained game ended"
        );
        Ok(())
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        ensure!(
            a >= 0x10000
                && (1..=65536).contains(&n)
                && a.checked_add(n as u64).is_some_and(|e| e < 0x7fff00000000),
            "Invalid bounded ball_gameplay native read"
        );
        self.checkpoint()?;
        let r = self.query_region(a)?;
        ensure!(
            r.committed
                && a >= r.start
                && a.checked_add(n as u64)
                    .zip(r.start.checked_add(r.len))
                    .is_some_and(|(x, y)| x <= y)
                && ((r.kind == MEM_IMAGE.0
                    && r.allocation == self.base()
                    && matches!(r.protection, 2 | 4 | 8 | 32))
                    || (r.kind == MEM_PRIVATE.0 && matches!(r.protection, 2 | 4))),
            "Gameplay ball read escaped bounded image/private data"
        );
        let mut bytes = vec![0; n];
        let mut done = 0;
        unsafe {
            ReadProcessMemory(
                self.query.0,
                a as _,
                bytes.as_mut_ptr() as _,
                n,
                Some(&mut done),
            )?;
        }
        ensure!(done == n, "Partial native ball_gameplay read");
        self.checkpoint()?;
        Ok(bytes)
    }
}
impl Inspect for Native {
    fn region(&self, a: u64) -> Result<Region> {
        self.checkpoint()?;
        self.query_region(a)
    }
}
impl Native {
    /// VirtualQueryEx only; callers have just passed a checkpoint.
    fn query_region(&self, a: u64) -> Result<Region> {
        let mut info = MEMORY_BASIC_INFORMATION::default();
        let n = unsafe {
            VirtualQueryEx(
                self.query.0,
                Some(a as _),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        ensure!(
            n == size_of::<MEMORY_BASIC_INFORMATION>(),
            "Gameplay ball VirtualQueryEx failed"
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
impl Native {
    pub(super) fn publish(
        &mut self,
        p: &Permit<'_>,
        durable: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        let parent_required = !p.undo();
        self.guard(parent_required)?;
        if !p.undo() {
            self.source_available(p.selected())?;
        }
        p.verify_before(self)?;
        let original = self.writable(p)?;
        durable()?;
        // Short-lived write handle is acquired only after complete snapshot+intent.
        let writer = Handle(unsafe { OpenProcess(WRITE, false, self.binding.game.pid)? });
        ensure!(
            retained(writer.0, &self.binding.game)?,
            "Gameplay ball write-handle identity ended"
        );
        self.guard(parent_required)?;
        crate::ball_gameplay_v2::verify_profile(self)?;
        if !p.undo() {
            self.source_available(p.selected())?;
        }
        p.verify_before(self)?;
        ensure!(
            same_region(self.writable(p)?, original),
            "Gameplay ball allocation/protection changed after write-handle acquisition"
        );
        durable()?;
        let c = p.change()?;
        ensure!(
            retained(writer.0, &self.binding.game)?
                && self.read(c.address, c.before.len())? == c.before,
            "Gameplay ball owner changed at publication boundary"
        );
        p.verify_before(self)?;
        durable()?;
        let mut done = 0;
        unsafe {
            WriteProcessMemory(
                writer.0,
                c.address as _,
                c.after.as_ptr() as _,
                c.after.len(),
                Some(&mut done),
            )?;
        }
        ensure!(
            done == c.after.len() && same_region(self.writable(p)?, original),
            "Court ball filename publication/allocation is uncertain"
        );
        p.verify_after(self)?;
        self.guard(parent_required)
    }
}
fn same_region(a: Region, b: Region) -> bool {
    a.start == b.start
        && a.len == b.len
        && a.allocation == b.allocation
        && a.kind == b.kind
        && a.protection == b.protection
        && a.committed == b.committed
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_proof_is_reused_only_for_the_same_unchanged_file_and_choice() {
        let root = std::env::temp_dir().join(format!(
            "ball-manifest-proof-{}",
            crate::backend::unique_id()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("manifest");
        fs::write(&path, b"balls/ball_a.iff,0M,1,2
").unwrap();
        let key = |p: &Path| {
            let f = File::open(p).unwrap();
            manifest_key(&f, &f.metadata().unwrap()).unwrap()
        };
        let first = key(&path);
        let mut proofs = ManifestProofs::default();
        assert!(!proofs.proven(&first, "balls/ball_a.iff"));
        proofs.remember(first.clone(), "balls/ball_a.iff");
        assert!(proofs.proven(&first, "balls/ball_a.iff"));
        assert!(!proofs.proven(&first, "balls/ball_b.iff"));
        // Same bytes in a replaced file, or a changed length/write time: re-read.
        let changed = ManifestKey {
            written: first.written + 1,
            ..first.clone()
        };
        assert!(!proofs.proven(&changed, "balls/ball_a.iff"));
        let longer = ManifestKey {
            len: first.len + 1,
            ..first.clone()
        };
        assert!(!proofs.proven(&longer, "balls/ball_a.iff"));
        fs::remove_file(&path).unwrap();
        fs::write(&path, b"balls/ball_a.iff,0M,1,2
").unwrap();
        let replaced = key(&path);
        assert_ne!(replaced.identity, first.identity);
        assert!(!proofs.proven(&replaced, "balls/ball_a.iff"));
        proofs.remember(replaced.clone(), "balls/ball_b.iff");
        // A new file forgets every proof of the old one.
        assert!(!proofs.proven(&replaced, "balls/ball_a.iff"));
        assert!(proofs.proven(&replaced, "balls/ball_b.iff"));
        fs::remove_dir_all(root).unwrap();
    }
    /// Real cost of the native operations, against this test process only
    /// (never the game): liveness checkpoint, VirtualQueryEx, ReadProcessMemory.
    #[test]
    #[ignore = "Measurement: prints per-operation native costs on this machine"]
    fn measure_native_operation_costs() {
        let pid = std::process::id();
        let query = Handle(unsafe { OpenProcess(READ, false, pid).unwrap() });
        let (mut c, mut e, mut k, mut u) = Default::default();
        unsafe { GetProcessTimes(query.0, &mut c, &mut e, &mut k, &mut u).unwrap() };
        let mut text = vec![0u16; 32768];
        let mut n = text.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(query.0, PROCESS_NAME_WIN32, PWSTR(text.as_mut_ptr()), &mut n)
                .unwrap()
        };
        let id = Identity {
            pid,
            started: ((((c.dwHighDateTime as u64) << 32) | c.dwLowDateTime as u64)).to_string(),
            path: String::from_utf16(&text[..n as usize]).unwrap(),
        };
        let data = vec![7u8; 4096];
        let at = data.as_ptr() as u64;
        let time = |label: &str, rounds: u32, f: &mut dyn FnMut()| {
            let t = Instant::now();
            for _ in 0..rounds {
                f();
            }
            println!("{label}: {:.1} us", t.elapsed().as_secs_f64() * 1e6 / rounds as f64);
        };
        time("checkpoint (retained)", 2000, &mut || assert!(retained(query.0, &id).unwrap()));
        time("checkpoint (alive+birth)", 2000, &mut || assert!(alive(query.0, &id).unwrap()));
        time("VirtualQueryEx", 2000, &mut || {
            let mut info = MEMORY_BASIC_INFORMATION::default();
            unsafe { VirtualQueryEx(query.0, Some(at as _), &mut info, size_of::<MEMORY_BASIC_INFORMATION>()) };
        });
        time("ReadProcessMemory 416", 2000, &mut || {
            let mut b = vec![0u8; 416];
            let mut done = 0;
            unsafe { ReadProcessMemory(query.0, at as _, b.as_mut_ptr() as _, 416, Some(&mut done)).unwrap() };
        });
        time("process snapshot+guard rows", 20, &mut || {
            session_safety::processes().unwrap();
        });
        // Read-only copy of the stock manifest on disk (no process access).
        if let Some(path) = std::env::var_os("C27_BALL_MANIFEST") {
            time("manifest read+validate", 3, &mut || {
                let bytes = fs::read(&path).unwrap();
                crate::ball_gameplay_v2::validate_manifest(
                    &bytes,
                    "balls/ball_gameplay_official_animal_zebra.iff",
                )
                .unwrap();
            });
            let file = File::open(&path).unwrap();
            time("manifest identity check", 200, &mut || {
                manifest_key(&file, &file.metadata().unwrap()).unwrap();
            });
        }
    }
    #[test]
    fn writer_accepts_only_one_private_nonexecutable_owned_dword() {
        let r = Region {
            start: 0x30000000000,
            len: 4096,
            allocation: 0x30000000000,
            kind: MEM_PRIVATE.0,
            protection: PAGE_READWRITE.0,
            committed: true,
        };
        assert!(writable_span(r, r.start + 4092, 4, r.allocation));
        assert!(!writable_span(r, r.start + 4093, 4, r.allocation));
        assert!(!writable_span(r, r.start, 4, r.allocation + 4096));
        // Court-record filename field: whole field inside the region, 1..=128 bytes.
        assert!(writable_span(r, r.start + 4040, 56, r.allocation));
        assert!(!writable_span(r, r.start + 4044, 56, r.allocation));
        assert!(!writable_span(r, r.start, 0, r.allocation));
        assert!(!writable_span(r, r.start, 129, r.allocation));
        for changed in [
            Region {
                protection: PAGE_EXECUTE_READWRITE.0,
                ..r
            },
            Region {
                protection: PAGE_READONLY.0,
                ..r
            },
            Region {
                kind: MEM_IMAGE.0,
                ..r
            },
            Region {
                committed: false,
                ..r
            },
            Region { len: 3, ..r },
            Region {
                protection: PAGE_READWRITE.0 | PAGE_GUARD.0,
                ..r
            },
        ] {
            assert!(!writable_span(changed, r.start, 4, r.allocation));
        }
    }
    #[test]
    fn process_guard_exempts_only_the_exact_retained_launcher() {
        let b = Binding::from_launched(
            Identity {
                pid: 11,
                started: "100".into(),
                path: r"C:\Fixture\NBA2K27.exe".into(),
            },
            Identity {
                pid: 22,
                started: "200".into(),
                path: r"C:\Portable\2k Launcher.exe".into(),
            },
            r"C:\Fixture".into(),
            r"C:\Portable".into(),
            "a".repeat(48),
        )
        .unwrap();
        let game = || session_safety::Process {
            name: "NBA2K27.exe".into(),
            pid: 11,
            parent: 22,
            identity: Some(b.game.clone()),
        };
        assert!(guard_rows(&[game()], &b, true, Some(&b.parent)).is_ok());
        assert!(guard_rows(&[game()], &b, true, None).is_err());
        assert!(guard_rows(&[game()], &b, false, None).is_ok());
        for name in ["node.exe", "EasyAntiCheat_EOS.exe"] {
            assert!(
                guard_rows(
                    &[
                        game(),
                        session_safety::Process {
                            name: name.into(),
                            pid: 99,
                            parent: 0,
                            identity: None
                        }
                    ],
                    &b,
                    false,
                    None
                )
                .is_err()
            );
        }
    }
}
