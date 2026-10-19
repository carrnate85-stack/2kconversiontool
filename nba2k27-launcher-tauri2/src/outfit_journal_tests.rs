use super::*;
use crate::blacktop_outfits::tests::{Fixture, game};
use std::sync::atomic::{AtomicUsize, Ordering};

const SESSION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
pub(super) fn root() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!("outfit-journal-{}", backend::unique_id()));
    fs::create_dir_all(p.join("Data")).unwrap();
    p
}
pub(super) fn fixture() -> (Fixture, Player, Composed) {
    let f = Fixture::selection();
    let player = outfits::capture(&f, &game(), &[])
        .unwrap()
        .players
        .remove(0);
    let catalog = crate::outfits::catalog().unwrap();
    let composed = catalog
        .presets
        .iter()
        .filter(|p| p.pool == player.pool && p.separable && !p.manual_only)
        .find_map(|p| {
            let o = catalog.original_outfit(&p.id, "Journal fixture").ok()?;
            let c = outfits::compose(&player, &o).ok()?;
            (c.items.len() >= 3 && c.items != player.items).then_some(c)
        })
        .unwrap();
    (f, player, composed)
}
pub(super) fn committed(store: &mut Store, f: &mut Fixture, p: &Player, c: &Composed) -> Page {
    let r = store.reserve(p, c).unwrap();
    assert_eq!(r.intent().kind, Kind::Compose);
    r.witness().pin().unwrap();
    assert!(store.ledger().choices.is_empty());
    let page = f.private(&game(), p, c);
    let ready = store.prepared(r.prepared(page.clone()).unwrap()).unwrap();
    ready.witness().pin().unwrap();
    let submitted = store.attempting(ready).unwrap();
    submitted.witness().pin().unwrap();
    assert_eq!(
        submitted.intent().target(submitted.page()).unwrap().uniform,
        p.source.uniform
    );
    let id = submitted.intent().id.clone();
    assert!(store.ledger().choices.is_empty());
    drop(submitted);
    store.verified(&id).unwrap();
    assert!(store.verified(&id).is_err());
    page
}

#[test]
fn journal_flushes_intent_page_attempt_confirmation_and_once_only_choices() {
    let root = root();
    let (mut f, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    let page = committed(&mut s, &mut f, &p, &c);
    assert_eq!(s.pages(), &[page]);
    let choices = s.ledger().choices.clone();
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].items, c.items);
    let disk = s.shared.as_ref().unwrap().read().unwrap();
    assert_eq!(disk.1, s.state);
    s.consume(&choices).unwrap();
    assert!(s.consume(&choices).is_err());
    assert_eq!(s.ledger().consumed.len(), 1);
    s.clear_pending().unwrap();
    assert!(s.ledger().choices.is_empty());
    assert_eq!(s.ledger().consumed.len(), 1);
    assert_eq!(s.shared.as_ref().unwrap().read().unwrap().1, s.state);
    let checks = AtomicUsize::new(0);
    s.finish(|| {
        checks.fetch_add(1, Ordering::SeqCst);
        Ok(())
    })
    .unwrap();
    assert_eq!(checks.load(Ordering::SeqCst), 3);
    assert!(!pending(&root).unwrap());
}

#[test]
fn unchanged_outfit_and_stock_restore_preserve_uniform_without_new_pages() {
    let root = root();
    let (mut f, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    let page = committed(&mut s, &mut f, &p, &c);
    let mut current = outfits::capture(&f, &game(), &[page])
        .unwrap()
        .players
        .remove(0);
    current.source.uniform = 0x900000000;
    let r = s.reserve(&current, &c).unwrap();
    assert_eq!(r.intent().kind, Kind::Existing);
    assert!(r.prepared(s.pages()[0].clone()).is_err());
    let r = s.reserve(&current, &c).unwrap();
    let ready = s.existing(r).unwrap();
    assert!(s.attempting(ready).is_err());
    let reservation = s.reserve(&current, &c).unwrap();
    let ready = s.existing(reservation).unwrap();
    let id = ready.intent().id.clone();
    assert_eq!(ready.intent().target(None).unwrap(), current.source);
    drop(ready);
    s.verified(&id).unwrap();
    assert_eq!(s.state.reservations, 1);
    let original = s.ledger().choices[0].choice_id.clone();
    let ready = s.reserve_restore(&current).unwrap();
    assert_eq!(ready.intent().kind, Kind::Restore);
    let sub = s.attempting(ready).unwrap();
    assert_eq!(
        sub.intent().target(None).unwrap(),
        Source {
            pointer: p.original_pointer,
            count: 2,
            uniform: current.source.uniform,
        }
    );
    assert!(!s.ledger().choices.is_empty());
    let id = sub.intent().id.clone();
    drop(sub);
    s.verified(&id).unwrap();
    assert!(s.ledger().choices.is_empty());
    assert!(!s.ledger().consumed.contains(&original));
    assert_eq!(s.pages().len(), 1);
    s.finish(|| Ok(())).unwrap();
}

#[test]
fn reservations_are_bounded_even_when_no_allocation_is_finished() {
    let root = root();
    let (_, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    for _ in 0..outfits::MAX_PAGES {
        drop(s.reserve(&p, &c).unwrap());
    }
    assert_eq!(s.state.reservations, 256);
    assert!(s.reserve(&p, &c).is_err());
    assert_eq!(s.state.reservations, 256);
    assert!(s.pages().is_empty());
    assert!(s.ledger().choices.is_empty());
    assert_eq!(s.shared.as_ref().unwrap().read().unwrap().1, s.state);
    s.finish(|| Ok(())).unwrap();
}

#[test]
fn foreign_sources_pages_capabilities_and_early_confirmation_are_rejected() {
    let a = root();
    let b = root();
    let (mut f, p, c) = fixture();
    let mut s = Store::create(&a, SESSION, &game()).unwrap();
    let mut other = Store::create(&b, SESSION, &game()).unwrap();
    let mut foreign = p.clone();
    foreign.source.pointer = 0x800000080;
    foreign.source.count = c.items.len() as u32;
    assert!(s.reserve(&foreign, &c).is_err());
    let r = s.reserve(&p, &c).unwrap();
    let id = r.intent().id.clone();
    assert!(s.verified(&id).is_err());
    let page = f.private(&game(), &p, &c);
    let prepared = r.prepared(page.clone()).unwrap();
    assert!(other.prepared(prepared).is_err());
    let r = s.reserve(&p, &c).unwrap();
    let mut bad = page.clone();
    bad.sha256 = "0".repeat(64);
    assert!(s.prepared(r.prepared(bad).unwrap()).is_err());
    let r = s.reserve(&p, &c).unwrap();
    let ready = s.prepared(r.prepared(page).unwrap()).unwrap();
    assert!(other.attempting(ready).is_err());
    assert!(s.ledger().choices.is_empty());
    s.finish(|| Ok(())).unwrap();
    other.finish(|| Ok(())).unwrap();
}

#[test]
fn active_capability_and_open_handle_prevent_foreign_deletion_and_recovery() {
    let root = root();
    let (_, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    let r = s.reserve(&p, &c).unwrap();
    let path = root.join("Data").join(MARKER);
    assert!(fs::OpenOptions::new().write(true).open(&path).is_err());
    assert!(fs::remove_file(&path).is_err());
    assert!(Store::create(&root, SESSION, &game()).is_err());
    assert!(s.finish(|| Ok(())).is_err());
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    r.witness().pin().unwrap();
    drop(r);
    assert!(s.finish(|| anyhow::bail!("game still alive")).is_err());
    assert!(pending(&root).unwrap());
    s.finish(|| Ok(())).unwrap();
}

#[test]
fn interrupted_append_halts_without_minting_or_replaying_a_capability() {
    for n in [0, 3, 14] {
        let root = root();
        let (_, p, c) = fixture();
        let mut s = Store::create(&root, SESSION, &game()).unwrap();
        s.fail_append_after = Some(n);
        assert!(s.reserve(&p, &c).is_err());
        assert!(s.halted().is_some());
        assert!(s.reserve(&p, &c).is_err());
        assert!(s.state.ops.is_empty());
        assert!(s.ledger().choices.is_empty());
        drop(s);
        let result = resolve_ended(&root, SESSION, &game(), || Ok(()));
        if n == 0 {
            result.unwrap();
            assert!(!pending(&root).unwrap());
        } else {
            assert!(result.is_err());
            assert!(pending(&root).unwrap());
        }
    }
}

#[test]
fn ended_recovery_requires_exact_game_session_file_and_data_directory() {
    let root = root();
    let (_, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    drop(s.reserve(&p, &c).unwrap());
    drop(s);
    let mut other = game();
    other.started = "134366400000000001".into();
    assert!(resolve_ended(&root, SESSION, &other, || Ok(())).is_err());
    assert!(resolve_ended(&root, &"a".repeat(48), &game(), || Ok(())).is_err());
    let path = root.join("Data").join(MARKER);
    let original = fs::read(&path).unwrap();
    let moved = root.join("moved.bin");
    fs::rename(&path, &moved).unwrap();
    fs::write(&path, &original).unwrap();
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    fs::remove_file(&path).unwrap();
    fs::rename(&moved, &path).unwrap();
    fs::rename(root.join("Data"), root.join("old-Data")).unwrap();
    fs::create_dir(root.join("Data")).unwrap();
    fs::rename(root.join("old-Data").join(MARKER), &path).unwrap();
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    fs::rename(&path, root.join("old-Data").join(MARKER)).unwrap();
    fs::remove_dir(root.join("Data")).unwrap();
    fs::rename(root.join("old-Data"), root.join("Data")).unwrap();
    assert!(resolve_ended(&root, SESSION, &game(), || anyhow::bail!("alive")).is_err());
    let calls = AtomicUsize::new(0);
    resolve_ended(&root, SESSION, &game(), || {
        ensure!(
            calls.fetch_add(1, Ordering::SeqCst) == 0,
            "game appeared at final gate"
        );
        Ok(())
    })
    .unwrap_err();
    assert!(pending(&root).unwrap());
    resolve_ended(&root, SESSION, &game(), || Ok(())).unwrap();
    assert!(!pending(&root).unwrap());
}

#[test]
fn corrupt_torn_unknown_and_out_of_order_frames_are_held() {
    let root = root();
    let (_, p, c) = fixture();
    let mut s = Store::create(&root, SESSION, &game()).unwrap();
    drop(s.reserve(&p, &c).unwrap());
    let data = fs::read(root.join("Data").join(MARKER)).unwrap();
    assert!(parse(&data).is_ok());
    for cut in [0, 7, 12, data.len() - 1] {
        assert!(parse(&data[..cut]).is_err());
    }
    let mut corrupt = data.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(parse(&corrupt).is_err());
    let h = s.shared.as_ref().unwrap().header.clone();
    let mut initial = [MAGIC.as_slice(), &frame(&Event::Header(h)).unwrap()].concat();
    initial.extend(frame(&Event::Attempt { id: "a".repeat(48) }).unwrap());
    assert!(parse(&initial).is_err());
    let body = br#"{"kind":"unknown","data":{}}"#;
    let invalid = [
        data.as_slice(),
        &(body.len() as u32).to_le_bytes(),
        &crc32fast::hash(body).to_le_bytes(),
        body,
    ]
    .concat();
    assert!(parse(&invalid).is_err());
    drop(s);
    fs::write(root.join("Data").join(MARKER), corrupt).unwrap();
    assert!(resolve_ended(&root, SESSION, &game(), || Ok(())).is_err());
    assert!(pending(&root).unwrap());
}

#[test]
fn invalid_owner_never_creates_a_file_and_reparse_parents_are_rejected() {
    let root = root();
    let mut invalid = game();
    invalid.path = "C:\\Fixture\\other.exe".into();
    assert!(Store::create(&root, SESSION, &invalid).is_err());
    assert!(Store::create(&root, "short", &game()).is_err());
    assert!(!pending(&root).unwrap());
    // A directory junction is created only inside this private fixture root.
    let target = root.join("target");
    fs::create_dir(&target).unwrap();
    fs::remove_dir(root.join("Data")).unwrap();
    crate::mod_profile::fixture_junction(&root.join("Data"), &target).unwrap();
    assert!(Store::create(&root, SESSION, &game()).is_err());
    assert!(!target.join(MARKER).exists());
}
