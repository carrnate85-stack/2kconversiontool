use super::*;
use std::cell::Cell;

/// Image-backed fake of the read-only game memory interface.
struct Image {
    base: u64,
    bytes: Vec<u8>,
    reads: Cell<usize>,
}
impl Memory for Image {
    fn base(&self) -> u64 {
        self.base
    }
    fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
        self.reads.set(self.reads.get() + 1);
        let start = (at - self.base) as usize;
        Ok(self.bytes[start..start + len].to_vec())
    }
}
fn image(stamp: u32, key: &[u8], pointer_ok: bool) -> Image {
    let base = 0x1_4000_0000u64;
    let mut bytes = vec![0u8; MANAGER_RVA as usize + 0x200];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&0x80u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x88..0x8C].copy_from_slice(&stamp.to_le_bytes());
    bytes[0xD0..0xD4].copy_from_slice(&PE_IMAGE.to_le_bytes());
    let manager = MANAGER_RVA as usize;
    bytes[manager + 0x58..manager + 0x58 + key.len()].copy_from_slice(key);
    let pointer = base + MANAGER_RVA + if pointer_ok { 0x58 } else { 0x60 };
    bytes[manager + 0xD8..manager + 0xE0].copy_from_slice(&pointer.to_le_bytes());
    bytes[manager + 0xE0..manager + 0xE8].copy_from_slice(&(key.len() as u64).to_le_bytes());
    Image {
        base,
        bytes,
        reads: Cell::new(0),
    }
}

#[test]
fn key_capture_requires_the_v2_image_and_an_initialized_inline_key() {
    let key: Vec<u8> = (1..=32).collect();
    assert_eq!(capture_key(&image(PE_STAMP, &key, true)).unwrap(), key);
    assert!(capture_key(&image(0x6AA2_013D, &key, true)).is_err());
    assert!(capture_key(&image(PE_STAMP, &key, false)).is_err());
    assert!(capture_key(&image(PE_STAMP, &[0; 32], true)).is_err());
    assert!(capture_key(&image(PE_STAMP, &[], true)).is_err());
}

#[test]
fn variants_parse_with_wpf_era_default_and_reject_unknown_names() {
    assert_eq!(Variant::parse("").unwrap(), Variant::WpfEra);
    for v in [Variant::WpfEra, Variant::National, Variant::Clubs] {
        assert_eq!(Variant::parse(v.name()).unwrap(), v);
        assert_eq!(v.expected_sha256().len(), 64);
    }
    assert!(Variant::parse("all-teams").is_err());
    assert!(!Variant::Clubs.exposes_categories() && Variant::WpfEra.exposes_categories());
}

#[test]
fn key_storage_is_local_and_bounded() {
    let root = std::env::temp_dir().join(format!("rust-wnba-key-{}", crate::backend::unique_id()));
    assert!(load_key(&root).is_none());
    save_key(&root, &[7; 32]).unwrap();
    assert_eq!(load_key(&root).unwrap(), vec![7; 32]);
    assert!(root.join("Data").join(FOLDER).join(KEY_FILE).is_file());
    fs::write(root.join("Data").join(FOLDER).join(KEY_FILE), [1; 3]).unwrap();
    assert!(load_key(&root).is_none());
    fs::remove_dir_all(root).unwrap();
}

fn decoded_dir() -> Option<PathBuf> {
    std::env::var_os("C27_WNBA_ROSTER_DECODED").map(PathBuf::from)
}

#[test]
#[ignore = "explicit installed archive and executable audit; read-only and requires C27_GAME_EXE"]
fn updated_installed_game_keeps_pinned_wnba_inputs_and_salt() {
    let exe = PathBuf::from(std::env::var_os("C27_GAME_EXE").expect("C27_GAME_EXE"));
    assert_eq!(exe.file_name().unwrap(), "NBA2K27.exe");
    assert_eq!(read_salt(&exe).unwrap().len(), SALT_LEN);
    let game = exe.parent().unwrap();
    let [nba, wnba] = read_indexed(game, [PATH, WNBA_PATH], &|| Ok(())).unwrap();
    assert_eq!(sha256_hex(&nba), STOCK_NBA_SHA256);
    assert_eq!(sha256_hex(&wnba), STOCK_WNBA_SHA256);
}

/// Rust merge reproduces tools/roster_merge_wnba.py byte for byte.
/// C27_WNBA_ROSTER_DECODED = folder with learn2k_roster.decoded.iff and
/// learn2k_wnba_roster.decoded.iff; C27_GAME_EXE = installed NBA2K27.exe.
#[test]
#[ignore]
fn decoded_stock_merge_matches_offline_hashes_for_every_variant() {
    let dir = decoded_dir().expect("C27_WNBA_ROSTER_DECODED");
    let exe = PathBuf::from(std::env::var_os("C27_GAME_EXE").expect("C27_GAME_EXE"));
    let salt = read_salt(&exe).unwrap();
    let base = fs::read(dir.join("learn2k_roster.decoded.iff")).unwrap();
    let src = fs::read(dir.join("learn2k_wnba_roster.decoded.iff")).unwrap();
    let parsed = Roster::parse(&base).unwrap();
    assert!(parsed.bad_crcs(&salt).is_empty());
    assert_eq!(parsed.iff_bytes(&salt).unwrap(), base, "stock round trip");
    for v in [Variant::WpfEra, Variant::National, Variant::Clubs] {
        let (bytes, s) = merge(
            Roster::parse(&base).unwrap(),
            Roster::parse(&src).unwrap(),
            v,
            &salt,
        )
        .unwrap();
        assert_eq!(sha256_hex(&bytes), v.expected_sha256(), "{}", v.name());
        assert_eq!((s.nba, s.wnba), (30, 15));
        if v == Variant::WpfEra {
            assert_eq!((s.teams, s.inserted_at, s.teams_added), (403, 161, 69));
        }
    }
}

/// End-to-end from the installed archives with a locally captured key.
/// C27_WNBA_ROSTER_KEY = key file; C27_GAME_EXE = installed NBA2K27.exe.
#[test]
#[ignore]
fn installed_game_build_decrypts_merges_and_caches() {
    let key =
        fs::read(std::env::var_os("C27_WNBA_ROSTER_KEY").expect("C27_WNBA_ROSTER_KEY")).unwrap();
    let exe = PathBuf::from(std::env::var_os("C27_GAME_EXE").expect("C27_GAME_EXE"));
    let game = exe.parent().unwrap();
    let root =
        std::env::temp_dir().join(format!("rust-wnba-build-{}", crate::backend::unique_id()));
    fs::create_dir_all(root.join("Data")).unwrap();
    let prepared = build(&root, game, Variant::WpfEra, &key, || Ok(())).unwrap();
    assert_eq!(prepared.summary.teams, 403);
    assert_eq!(load_key(&root).unwrap(), key);
    let again = cached(&root, game, Variant::WpfEra).unwrap().unwrap();
    assert_eq!(again.bytes, prepared.bytes);
    assert!(build(&root, game, Variant::WpfEra, &[0x55; 32], || Ok(())).is_err());
    fs::remove_dir_all(root).unwrap();
}
