use super::*;
use crate::jukebox_catalog::Track;
use serde_json::Map;

fn track(bank: u16, cover: &str) -> Track {
    Track {
        name: format!("Song {bank}"),
        artist: "Artist".into(),
        cover: cover.into(),
        collection: 1,
        bank,
        unique: bank.saturating_sub(1),
        extra: Map::new(),
    }
}
fn music(tracks: Vec<Track>) -> Music {
    Music {
        tracks,
        extra: Map::new(),
    }
}
/// Synthetic TXTR in the verified layout: magic, string-table size at +8,
/// CRC-32(name) at +0x18, typed u32 fields keyed by CRC-32(field name).
fn txtr_bytes(name: &str, side: u32, hash: &str) -> Vec<u8> {
    let mut d = vec![0u8; 0x20];
    d[..4].copy_from_slice(&TXTR_MAGIC);
    d[0x18..0x1c].copy_from_slice(&crc32fast::hash(name.as_bytes()).to_le_bytes());
    for f in ["Width", "Height"] {
        d.extend_from_slice(&crc32fast::hash(f.as_bytes()).to_le_bytes());
        d.extend_from_slice(&[6, 0, 0, 0x40]);
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(&side.to_le_bytes());
    }
    let table = format!("{name}\0Binary\0{name}.{hash}.tld\0");
    d[8..12].copy_from_slice(&(table.len() as u32).to_le_bytes());
    d.extend_from_slice(table.as_bytes());
    d
}
fn stored_zip(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let crc = crc32fast::hash(data).to_le_bytes();
        let size = (data.len() as u32).to_le_bytes();
        let local = (out.len() as u32).to_le_bytes();
        out.extend_from_slice(b"PK\x03\x04\x0a\0\0\0\0\0\0\0\0\0");
        out.extend_from_slice(&crc);
        out.extend_from_slice(&size);
        out.extend_from_slice(&size);
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(b"PK\x01\x02\x0a\0\x0a\0\0\0\0\0\0\0\0\0");
        central.extend_from_slice(&crc);
        central.extend_from_slice(&size);
        central.extend_from_slice(&size);
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&local);
        central.extend_from_slice(name.as_bytes());
    }
    let offset = (out.len() as u32).to_le_bytes();
    let count = (files.len() as u16).to_le_bytes();
    let size = (central.len() as u32).to_le_bytes();
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    out.extend_from_slice(&count);
    out.extend_from_slice(&count);
    out.extend_from_slice(&size);
    out.extend_from_slice(&offset);
    out.extend_from_slice(&[0, 0]);
    out
}
pub(super) fn container(names: &[(&str, u32)]) -> Vec<u8> {
    stored_zip(
        &names
            .iter()
            .map(|(n, side)| (format!("{n}.TXTR"), txtr_bytes(n, *side, "0123456789abcdef")))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn hashes_match_the_verifier_check_values() {
    assert_eq!(crc64_xz(b"123456789"), 0x995D_C9BB_DF19_39FA);
    assert_eq!(
        format!("alb_cvr_cont_{:08x}.iff", crc32fast::hash(b"jukebox")),
        CONTAINER
    );
}
#[test]
fn planar_layout_puts_endpoints_first_and_round_trips() {
    let interleaved = (0..RAW).map(|i| (i % 251) as u8).collect::<Vec<_>>();
    let planar = planar_split(&interleaved).unwrap();
    assert_eq!(&planar[..4], &interleaved[..4]);
    assert_eq!(&planar[4..8], &interleaved[8..12]);
    assert_eq!(&planar[RAW / 2..RAW / 2 + 4], &interleaved[4..8]);
    assert_eq!(planar_join(&planar), interleaved);
    assert!(planar_split(&interleaved[1..]).is_err());
}
#[test]
fn texture_form_refuses_wpf_uncompressed_tld() {
    // WPF/Rust library form: 0x21 with stored == raw. No stock precedent.
    let mut wpf = vec![0u8; 32800];
    wpf[..24].copy_from_slice(&PREFIX);
    wpf[24..28].copy_from_slice(&32768u32.to_le_bytes());
    wpf[28..32].copy_from_slice(&32768u32.to_le_bytes());
    assert!(validate_texture(&wpf).is_err());
    let mut stock = PREFIX.to_vec();
    stock.extend_from_slice(&32768u32.to_le_bytes());
    stock.extend_from_slice(&3u32.to_le_bytes());
    stock.extend_from_slice(&[0x8c, 0x0c, 0]);
    validate_texture(&stock).unwrap();
    stock[32] = 0;
    assert!(validate_texture(&stock).is_err());
}
#[test]
fn texture_paths_are_exact_content_hash_names() {
    let texture = b"anything".to_vec();
    let logical = texture_logical("001_helicopter", &texture);
    assert!(texture_path(&logical));
    let hash = format!("{:016x}", crc64_xz(&texture));
    assert_eq!(logical, format!("shared/{}/001_helicopter.{hash}.tld", &hash[..2]));
    for bad in [
        "shared/71/001_helicopter.711a66d148d9d178.dds",
        "shared/72/001_helicopter.711a66d148d9d178.tld",
        "shared/71/022_let_'em_know.711a66d148d9d178.tld",
        "shared/71/x/001.711a66d148d9d178.tld",
        "music.iff",
    ] {
        assert!(!texture_path(bad), "{bad}");
    }
    assert!(texture_path("shared/71/001_helicopter.711a66d148d9d178.tld"));
}
#[test]
fn container_retarget_changes_only_hash_and_crcs() {
    let bytes = container(&[("001_helicopter", 256), ("071_ruff_ryders_anthem", 256), ("2k27_playlistart_season2", 1024)]);
    let mut c = Container::parse(bytes.clone()).unwrap();
    assert!(c.slot("001_helicopter"));
    assert!(!c.slot("071_ruff_ryders_anthem"));
    assert!(!c.slot("2k27_playlistart_season2"));
    assert!(!c.slot("missing"));
    c.retarget("001_helicopter", "fedcba9876543210").unwrap();
    assert_eq!(c.hash("001_helicopter").unwrap(), "fedcba9876543210");
    assert!(c.retarget("001_helicopter", "FEDCBA9876543210").is_err());
    let out = c.finish().unwrap();
    assert_eq!(out.len(), bytes.len());
    let changed = out.iter().zip(&bytes).filter(|(a, b)| a != b).count();
    assert!(changed <= 16 + 8, "changed {changed} bytes");
    // CRCs are rewritten: the strict parser accepts it, a raw edit is refused.
    Container::parse(out.clone()).unwrap();
    let mut tampered = bytes.clone();
    let at = tampered.windows(16).position(|w| w == b"0123456789abcdef").unwrap();
    tampered[at] = b'f';
    assert!(Container::parse(tampered).is_err());
}
#[test]
fn txtr_rejects_wrong_identity_or_leaf() {
    let good = txtr_bytes("001_helicopter", 256, "0123456789abcdef");
    txtr(&good).unwrap();
    let mut wrong_crc = good.clone();
    wrong_crc[0x18] ^= 1;
    assert!(txtr(&wrong_crc).is_err());
    let wrong_leaf = txtr_bytes("001_helicopter", 256, "0123456789abcdeg");
    assert!(txtr(&wrong_leaf).is_err());
    let mut wrong_magic = good;
    wrong_magic[0] = 0;
    assert!(txtr(&wrong_magic).is_err());
}
#[test]
fn slot_choice_follows_wpf_own_cover_and_preserved_name_rules() {
    let stock = music(vec![
        track(1, "001_helicopter"),
        track(2, "002_two"),
        track(3, "003_three"),
        track(4, "004_four"),
    ]);
    let names = ["001_helicopter", "002_two", "003_three", "004_four", "051_2ksoundeffects", "071_ruff_ryders_anthem"]
        .map(String::from)
        .to_vec();
    let slot = |n: &str| n != "071_ruff_ryders_anthem";
    // Bank 1 custom with art takes its own bank's cover. Bank 2 custom with
    // art finds 002 still shown by a song without art, so it takes a spare.
    let published = music(vec![
        track(1, "custom_one"),
        track(2, "custom_two"),
        track(3, "002_two"),
    ]);
    let chosen = choose(&names, &stock, &published, &[true, true, false], slot).unwrap();
    assert_eq!(chosen[0].as_deref(), Some("001_helicopter"));
    assert_eq!(chosen[1].as_deref(), Some("003_three"));
    assert_eq!(chosen[2], None);
    // No free slot: the song keeps its cover (reported as skipped).
    let tight = ["001_helicopter".to_string()].to_vec();
    let chosen = choose(&tight, &stock, &published, &[true, true, false], |_| true).unwrap();
    assert_eq!(chosen[0].as_deref(), Some("001_helicopter"));
    assert_eq!(chosen[1], None);
    assert!(choose(&names, &stock, &published, &[true], slot).is_err());
}
/// Opt-in: NBA2K27_ACTUAL_DISK_AUDIT=1 reads the installed Version 2 game
/// (manifest, archive 0I, the pinned Oodle DLL). Nothing is written.
#[test]
#[ignore = "opt-in installed-disk read (NBA2K27_ACTUAL_DISK_AUDIT=1)"]
fn actual_version2_cover_route_plan() {
    let game = std::path::PathBuf::from(
        std::env::var("NBA2K27_GAME_ROOT")
            .unwrap_or_else(|_| r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27".into()),
    );
    assert_eq!(std::env::var("NBA2K27_ACTUAL_DISK_AUDIT").as_deref(), Ok("1"));
    let stock = crate::jukebox_v2_resources::read_stock(&game, || Ok(())).unwrap().music;
    let mut native = vec![0u8; 32800];
    native[..4].copy_from_slice(b"TLD ");
    native[8..12].copy_from_slice(&[5, 33, 71, 1]);
    native[12..16].copy_from_slice(&31u32.to_le_bytes());
    native[16..24].copy_from_slice(&[0, 1, 0, 1, 1, 0, 1, 0]);
    native[24..28].copy_from_slice(&32768u32.to_le_bytes());
    native[28..32].copy_from_slice(&32768u32.to_le_bytes());
    for (i, block) in native[32..].chunks_exact_mut(8).enumerate() {
        block[..2].copy_from_slice(&(0xf800u16 | (i as u16 & 0x7ff)).to_le_bytes());
        block[2..4].copy_from_slice(&0x001fu16.to_le_bytes());
        block[4..].copy_from_slice(&(i as u32).wrapping_mul(2654435761).to_le_bytes());
    }
    let first = stock.tracks.iter().find(|t| t.collection == 1 && t.bank == 1).unwrap();
    let mut published = music(vec![first.clone(), {
        let mut t = first.clone();
        t.bank = 2;
        t.unique = 1;
        t.cover = "custom".into();
        t
    }]);
    let route = plan(&game, &stock, &mut published, &[None, Some(native)], &|| Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(route.textures.len(), 1);
    let (logical, texture) = &route.textures[0];
    assert!(texture_path(logical));
    validate_texture(texture).unwrap();
    let parsed = Container::parse(route.container.clone()).unwrap();
    assert_eq!(parsed.covers.len(), 103);
    let slot = &published.tracks[1].cover;
    assert!(logical.contains(&format!("/{slot}.")));
    assert_eq!(parsed.hash(slot).unwrap(), format!("{:016x}", crc64_xz(texture)));
    // The bank-2 stock cover is the slot; the bank-1 stock song keeps its own.
    assert_eq!(published.tracks[0].cover, first.cover);
    println!("PASS actual V2 cover route: {logical} ({} bytes), container {} bytes", texture.len(), route.container.len());
}
