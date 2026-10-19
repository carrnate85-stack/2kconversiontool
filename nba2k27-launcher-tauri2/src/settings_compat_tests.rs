use super::*;
use serde_json::json;
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self> {
        let root = std::env::current_dir()?
            .join("artifacts")
            .join(format!("settings-compat-{}", crate::backend::unique_id()));
        fs::create_dir_all(root.join("Data"))?;
        Ok(Self(root))
    }
    fn old(&self) -> PathBuf {
        self.0.join("Data/settings.json")
    }
    fn new_path(&self) -> PathBuf {
        self.0.join("Data/Rust/settings.json")
    }
    fn write_old(&self, value: &Value) -> Result<Vec<u8>> {
        let b = serde_json::to_vec_pretty(value)?;
        fs::write(self.old(), &b)?;
        Ok(b)
    }
    fn open(&self) -> Result<(Store, Settings, Report)> {
        Store::open(&self.0, crate::backend::validate_settings)
    }
}
fn same(a: &Settings, b: &Settings) -> bool {
    serde_json::to_value(a).unwrap() == serde_json::to_value(b).unwrap()
}
fn wpf() -> Value {
    json!({"gameRoot":"C:\\Games\\NBA 2K27","modsRoot":"D:\\Mods\\NBA2K27",
    "modsProfiles":[{"name":"Original mods","path":"D:\\Mods\\NBA2K27"}],"provider":"local","startCameras":true,
    "autoOpenBlacktop":false,"unlockWnbaFreestyle":true,"unlockWnbaBlacktop":true,
    "blacktopPresentation":{"drone":true,"dj":true,"mc":true},"blacktopCrowdEnabled":false,
    "startupVideoEnabled":true,"theme":"light","scaler":0.8,"helperPath":"C:\\Untrusted\\helper.exe"})
}
fn defaults_and_one_time() -> Result<()> {
    let f = Fixture::new()?;
    let (mut store, settings, report) = f.open()?;
    assert!(same(&settings, &Settings::default()));
    assert_eq!(report.source, "fresh-defaults");
    assert!(!f.old().exists());
    assert!(store.path().ends_with("Data/Rust/settings.json"));
    let mut changed = settings.clone();
    changed.theme = "light".into();
    store.save(&changed)?;
    drop(store);
    // A corrupt older file is irrelevant once a complete namespaced save exists.
    fs::write(f.old(), b"{now corrupt}")?;
    let (store, loaded, report) = f.open()?;
    assert!(same(&loaded, &changed));
    assert_eq!(report.source, "rust-current");
    drop(store);
    assert_eq!(fs::read(f.old())?, b"{now corrupt}");
    let old = Fixture::new()?;
    let mut prior = Settings::default();
    prior.auto_show_overlay = false;
    prior.theme = "light".into();
    prior.cameras_enabled = true;
    let exact = old.write_old(&serde_json::to_value(&prior)?)?;
    let (store, loaded, report) = old.open()?;
    assert!(same(&loaded, &prior));
    assert_eq!(report.source, "older-rust");
    drop(store);
    assert_eq!(fs::read(old.old())?, exact);
    println!(
        "PASS fresh defaults, exact older-Rust import, one-time namespaced priority, safe save and repeated startup; original byte preservation"
    );
    Ok(())
}
fn conservative_wpf() -> Result<()> {
    let f = Fixture::new()?;
    let exact = f.write_old(&wpf())?;
    let (mut store, s, r) = f.open()?;
    assert_eq!(r.source, "wpf-compatible-preferences");
    assert_eq!(s.game_root, r"C:\Games\NBA 2K27");
    assert_eq!(s.mods_root, r"D:\Mods\NBA2K27");
    assert!(!s.auto_show_overlay);
    assert!(s.auto_hide_overlay);
    assert_eq!(s.theme, "dark");
    assert_eq!(s.scaler, 1.0);
    assert!(
        !s.unlock_wnba
            && !s.unlock_wnba_freestyle
            && !s.cameras_enabled
            && !s.startup_video
            && !s.lock_camera
    );
    // Absent in this WPF file: WPF's default (on) applies.
    assert!(s.use_playlist_in_freestyle);
    // WPF's saved court-presentation choices carry over (crowd hidden = remove).
    assert!(s.remove_drone && s.remove_dj && s.remove_mc && s.remove_crowd);
    assert!(
        !r.preserved_not_imported
            .iter()
            .any(|p| p.key.starts_with("blacktop"))
    );
    assert!(
        r.preserved_not_imported
            .iter()
            .any(|p| p.key == "helperPath")
    );
    assert!(r.preserved_not_imported.iter().any(|p| p.key == "theme"));
    assert_eq!(r.original_sha256, Some(digest(&exact)));
    let mut next = s;
    next.theme = "light".into();
    store.save(&next)?;
    drop(store);
    assert_eq!(fs::read(f.old())?, exact);
    let (store, s, r) = f.open()?;
    assert_eq!(s.theme, "light");
    assert_eq!(r.source, "rust-current");
    drop(store);
    assert_eq!(fs::read(f.old())?, exact);
    println!(
        "PASS explicit WPF signature, safe path/auto-show-only mapping, preserved-options report; native/helper/theme settings never inherited"
    );
    Ok(())
}
fn corrupt_and_unknown() -> Result<()> {
    let mut bad_wpf = wpf();
    bad_wpf["blacktopPresentation"]["drone"] = json!("yes");
    let mut unknown_rust = serde_json::to_value(Settings::default())?;
    unknown_rust["arbitraryWorker"] = json!("bad");
    let mut wrong_rust = serde_json::to_value(Settings::default())?;
    wrong_rust["scaler"] = json!(2.0);
    for value in [
        json!({}),
        json!([]),
        json!({"gameRoot":"C:\\Games","modsRoot":"C:\\Mods"}),
        json!({"autoShowOverlay":true}),
        bad_wpf,
        unknown_rust,
        wrong_rust,
    ] {
        let f = Fixture::new()?;
        let b = f.write_old(&value)?;
        assert!(f.open().is_err());
        assert_eq!(fs::read(f.old())?, b);
        assert!(!f.new_path().exists());
    }
    for bad in [
        b"{".to_vec(),
        b"{\"x\":1,\"x\":2}".to_vec(),
        vec![b' '; MAX + 1],
    ] {
        let f = Fixture::new()?;
        fs::write(f.old(), &bad)?;
        assert!(f.open().is_err());
        assert_eq!(fs::read(f.old())?, bad);
        assert!(!f.new_path().exists());
    }
    for bad in [
        b"{".to_vec(),
        b"{}".to_vec(),
        b"{\"profile\":\"x\",\"autoShowOverlay\":true,\"autoHideOverlay\":true,\"theme\":\"dark\",\"theme\":\"light\"}".to_vec(),
        serde_json::to_vec(&wpf())?,
        vec![b' '; MAX + 1],
    ] {
        let f = Fixture::new()?;
        fs::create_dir(f.0.join("Data/Rust"))?;
        fs::write(f.new_path(), &bad)?;
        assert!(f.open().is_err());
        assert_eq!(fs::read(f.new_path())?, bad);
    }
    for path in [
        "",
        "é:",
        r"\\server\share\mods",
        r"C:\good\..\bad",
        r"C:\good\.\bad",
        r"C:\good\extra:stream",
        r"C:\%APPDATA%\mods",
        r"C:\mods\",
        r"C:\mods\\x",
    ] {
        assert!(local_path(path).is_err(), "accepted {path}");
    }
    assert!(local_path(r"C:\Games\NBA 2K27 中文").is_ok());
    println!(
        "PASS corrupt/unknown/incomplete/oversized legacy and current files hold untouched; ambiguous/untrusted path rejection without Unicode panics"
    );
    Ok(())
}
fn foreign_and_concurrent() -> Result<()> {
    let f = Fixture::new()?;
    let (mut first, s, _) = f.open()?;
    let (mut second, _, _) = f.open()?;
    let mut next = s.clone();
    next.theme = "light".into();
    first.save(&next)?;
    verify_no_writes(&f, &first, true)?;
    verify_no_writes(&f, &second, false)?;
    let exact = fs::read(f.new_path())?;
    assert!(second.save(&s).is_err());
    assert_eq!(fs::read(f.new_path())?, exact);
    drop(second);
    drop(first);
    let (mut store, s, _) = f.open()?;
    let exact = fs::read(f.new_path())?;
    fs::rename(f.new_path(), f.0.join("Data/Rust/foreign-held-original"))?;
    fs::write(f.new_path(), &exact)?;
    verify_no_writes(&f, &store, false)?;
    assert!(store.save(&s).is_err());
    assert_eq!(fs::read(f.new_path())?, exact);
    drop(store);
    let g = Fixture::new()?;
    fs::create_dir(g.0.join("Data/Rust"))?;
    let doc = document::Document::open(&g.0.join("Data/Rust"))?;
    let (_, expected) = doc.load()?;
    let foreign = b"foreign concurrent creation";
    fs::write(g.new_path(), foreign)?;
    assert!(
        doc.commit(&expected, b"{}", &crate::backend::unique_id())
            .is_err()
    );
    assert_eq!(fs::read(g.new_path())?, foreign);
    drop(doc);
    let h = Fixture::new()?;
    fs::create_dir(h.0.join("Data/Rust"))?;
    let doc = document::Document::open(&h.0.join("Data/Rust"))?;
    let (_, expected) = doc.load()?;
    let target = h.new_path();
    assert!(
        doc.test_commit(&expected, b"{}", &crate::backend::unique_id(), |point| {
            if point == 1 {
                fs::write(&target, foreign)?;
            }
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(h.new_path())?, foreign);
    assert!(doc.recover().is_err());
    assert_eq!(fs::read(h.new_path())?, foreign);
    println!(
        "PASS stale store/CAS refusal, same-byte foreign identity refusal and concurrent creation before/during atomic first publication; foreign files retained"
    );
    Ok(())
}
fn interrupted_commits() -> Result<()> {
    for point in 1..=4 {
        let f = Fixture::new()?;
        let (store, mut next, _) = f.open()?;
        let before = fs::read(f.new_path())?;
        next.theme = "light".into();
        let after = serde_json::to_vec_pretty(&next)?;
        assert!(
            store
                .document
                .test_commit(&store.revision, &after, &crate::backend::unique_id(), |p| {
                    ensure!(p != point, "injected settings publication fault {point}");
                    Ok(())
                })
                .is_err()
        );
        verify_no_writes(&f, &store, false)?;
        store.document.recover()?;
        verify_no_writes(&f, &store, point <= 2)?;
        drop(store);
        let (store, s, _) = f.open()?;
        assert_eq!(s.theme, if point <= 2 { "dark" } else { "light" });
        assert_eq!(
            fs::read(f.new_path())?,
            if point <= 2 { before } else { after }
        );
        assert!(!f.0.join("Data/Rust/settings-update.json").exists());
        drop(store);
    }
    let foreign = Fixture::new()?;
    let (store, _, _) = foreign.open()?;
    let before = fs::read(foreign.new_path())?;
    let nonce = crate::backend::unique_id();
    assert!(
        store
            .document
            .test_commit(&store.revision, b"{}", &nonce, |point| {
                ensure!(point != 2, "stop after original backup");
                Ok(())
            })
            .is_err()
    );
    fs::write(foreign.new_path(), b"foreign after interrupted commit")?;
    let backup = foreign
        .0
        .join("Data/Rust")
        .join(format!(".2k-rust-settings-{nonce}.previous"));
    assert!(store.document.recover().is_err());
    assert_eq!(
        fs::read(foreign.new_path())?,
        b"foreign after interrupted commit"
    );
    assert_eq!(fs::read(backup)?, before);
    assert!(foreign.0.join("Data/Rust/settings-update.json").exists());
    drop(store);
    println!(
        "PASS four durable transaction phase faults recover original or exact completed settings; foreign recovery target holds with backup/receipt preserved"
    );
    Ok(())
}
fn tree_snapshot(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>, std::time::SystemTime)>> {
    let mut out = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let mut entries = fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let path = entry.path();
            let m = fs::symlink_metadata(&path)?;
            if m.is_dir() {
                out.push((path.clone(), vec![], m.modified()?));
                pending.push(path);
            } else {
                out.push((path.clone(), fs::read(path)?, m.modified()?));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
fn verify_no_writes(f: &Fixture, store: &Store, valid: bool) -> Result<()> {
    let before = tree_snapshot(&f.0)?;
    assert_eq!(store.verify_current().is_ok(), valid);
    assert_eq!(
        tree_snapshot(&f.0)?,
        before,
        "verify_current wrote files/directories or changed bytes/mtime"
    );
    Ok(())
}
fn launch_verification() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.write_old(&wpf())?;
    let (mut store, mut settings, _) = f.open()?;
    verify_no_writes(&f, &store, true)?;
    settings.theme = "light".into();
    store.save(&settings)?;
    verify_no_writes(&f, &store, true)?;
    let saved = fs::read(f.new_path())?;
    fs::remove_file(f.new_path())?;
    verify_no_writes(&f, &store, false)?;
    fs::write(f.new_path(), &saved)?;
    verify_no_writes(&f, &store, false)?;
    drop(store);
    let (store, _, _) = f.open()?;
    verify_no_writes(&f, &store, true)?;
    fs::write(f.new_path(), b"malformed replacement")?;
    verify_no_writes(&f, &store, false)?;
    assert_eq!(fs::read(f.old())?, original);
    println!(
        "PASS read-only launch verification: current/successful save valid; pending commits, stale revision, deletion, same-byte identity replacement and corrupt replacement held; complete fixture snapshots unchanged"
    );
    Ok(())
}
fn pins_and_links() -> Result<()> {
    let f = Fixture::new()?;
    let original = f.write_old(&wpf())?;
    let (store, _, _) = f.open().context("pin fixture open")?;
    assert!(fs::rename(f.0.join("Data/Rust"), f.0.join("MovedRust")).is_err());
    assert!(fs::rename(f.0.join("Data"), f.0.join("MovedData")).is_err());
    drop(store);
    assert_eq!(fs::read(f.old())?, original);
    let linked = Fixture::new()?;
    let b = linked.write_old(&wpf())?;
    fs::hard_link(linked.old(), linked.0.join("other-link")).context("legacy hardlink fixture")?;
    assert!(linked.open().is_err());
    assert_eq!(fs::read(linked.old())?, b);
    let current = Fixture::new()?;
    let (store, _, _) = current.open()?;
    drop(store);
    fs::hard_link(current.new_path(), current.0.join("same-settings-link"))
        .context("current hardlink fixture")?;
    assert!(current.open().is_err());
    let wrong = Fixture::new()?;
    fs::write(wrong.0.join("Data/Rust"), b"not a directory")?;
    assert!(wrong.open().is_err());
    let junction = Fixture::new()?;
    let target = junction.0.join("outside");
    fs::create_dir(&target)?;
    make_junction(&junction.0.join("Data/Rust"), &target).context("native junction fixture")?;
    assert!(junction.open().is_err());
    assert!(fs::read_dir(&target)?.next().is_none());
    fs::remove_dir(junction.0.join("Data/Rust"))?;
    println!(
        "PASS actual pinned root/Data/Rust directory rename refusal, hardlinked settings rejection, non-directory and native junction rejection"
    );
    Ok(())
}
fn make_junction(path: &Path, target: &Path) -> Result<()> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows::Win32::{Foundation::HANDLE, System::IO::DeviceIoControl};
    fs::create_dir(path)?;
    let file = fs::OpenOptions::new()
        .access_mode(0xc0010000)
        .share_mode(1)
        .custom_flags(0x02200000)
        .open(path)?;
    let text = target.to_string_lossy();
    let substitute = format!(r"\??\{text}").encode_utf16().collect::<Vec<_>>();
    let print = text.encode_utf16().collect::<Vec<_>>();
    let sub_bytes = substitute.len() * 2;
    let print_bytes = print.len() * 2;
    let size = 8 + sub_bytes + 2 + print_bytes + 2;
    let mut b = Vec::new();
    b.extend(0xa0000003u32.to_le_bytes());
    b.extend((size as u16).to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b.extend((sub_bytes as u16).to_le_bytes());
    b.extend(((sub_bytes + 2) as u16).to_le_bytes());
    b.extend((print_bytes as u16).to_le_bytes());
    for c in substitute
        .into_iter()
        .chain(Some(0))
        .chain(print)
        .chain(Some(0))
    {
        b.extend(c.to_le_bytes());
    }
    let mut returned = 0;
    unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            0x000900a4,
            Some(b.as_ptr() as _),
            b.len() as u32,
            None,
            0,
            Some(&mut returned),
            None,
        )?;
    }
    Ok(())
}
fn wpf_presentation_choices() -> Result<()> {
    let mut partial = wpf();
    partial["blacktopPresentation"] = json!({"dj":true});
    partial["blacktopCrowdEnabled"] = json!(true);
    let mut absent = wpf();
    absent
        .as_object_mut()
        .unwrap()
        .remove("blacktopPresentation");
    absent
        .as_object_mut()
        .unwrap()
        .remove("blacktopCrowdEnabled");
    for (value, expected) in [(partial, [false, true, false, false]), (absent, [false; 4])] {
        let f = Fixture::new()?;
        f.write_old(&value)?;
        let (store, s, _) = f.open()?;
        assert_eq!(
            [s.remove_drone, s.remove_dj, s.remove_mc, s.remove_crowd],
            expected
        );
        drop(store);
    }
    println!("PASS WPF drone/DJ/MC and crowd choices map exactly; absent keys keep removals off");
    Ok(())
}
pub(crate) fn run() -> Result<()> {
    defaults_and_one_time()?;
    conservative_wpf()?;
    wpf_presentation_choices()?;
    corrupt_and_unknown()?;
    foreign_and_concurrent()?;
    interrupted_commits()?;
    launch_verification()?;
    pins_and_links()?;
    Ok(())
}
