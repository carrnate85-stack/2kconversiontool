use super::*;

const INSTALLED: &str = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe";

fn temp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("relocation-{name}-{}", crate::backend::unique_id()));
    fs::create_dir_all(&p).unwrap();
    p
}

/// Builds a minimal PE32+ file: one `.text` section holding `code` at
/// `text_va`, plus raw-less sections with the given (va, size, flags).
fn synthetic_pe(stamp: u32, text_va: u32, code: &[u8], extra: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut h = vec![0u8; 0x1000];
    h[0] = b'M';
    h[1] = b'Z';
    let pe = 0x80usize;
    h[0x3C..0x40].copy_from_slice(&(pe as u32).to_le_bytes());
    h[pe..pe + 4].copy_from_slice(b"PE\0\0");
    h[pe + 4..pe + 6].copy_from_slice(&0x8664u16.to_le_bytes());
    let count = 1 + extra.len() as u16;
    h[pe + 6..pe + 8].copy_from_slice(&count.to_le_bytes());
    h[pe + 8..pe + 12].copy_from_slice(&stamp.to_le_bytes());
    h[pe + 20..pe + 22].copy_from_slice(&240u16.to_le_bytes());
    let opt = pe + 24;
    h[opt..opt + 2].copy_from_slice(&0x20Bu16.to_le_bytes());
    h[opt + 24..opt + 32].copy_from_slice(&0x1_4000_0000u64.to_le_bytes());
    h[opt + 56..opt + 60].copy_from_slice(&0x3600_0000u32.to_le_bytes());
    h[opt + 60..opt + 64].copy_from_slice(&0x1000u32.to_le_bytes());
    let mut write = |i: usize, name: &[u8], va: u32, vs: u32, raw: u32, rs: u32, flags: u32| {
        let o = opt + 240 + i * 40;
        h[o..o + name.len()].copy_from_slice(name);
        h[o + 8..o + 12].copy_from_slice(&vs.to_le_bytes());
        h[o + 12..o + 16].copy_from_slice(&va.to_le_bytes());
        h[o + 16..o + 20].copy_from_slice(&rs.to_le_bytes());
        h[o + 20..o + 24].copy_from_slice(&raw.to_le_bytes());
        h[o + 36..o + 40].copy_from_slice(&flags.to_le_bytes());
    };
    write(0, b".text", text_va, code.len() as u32, 0x1000, code.len() as u32, 0x6000_0020);
    for (i, (va, size, flags)) in extra.iter().enumerate() {
        write(i + 1, b".data", *va, *size, 0, 0, *flags);
    }
    let mut file = h;
    file.extend_from_slice(code);
    file
}

/// Places one feature's catalog anchors with code shifted by `code_delta`
/// and every RIP target shifted by `data_delta`, the way a recompiled game
/// update moves code and globals.
fn shifted(f: &FeatureCatalog, code_delta: i64, data_delta: i64, stamp: u32) -> (Vec<u8>, u32) {
    let lo = f.anchors.iter().map(|a| a.rva).min().unwrap() & !0xFFF;
    let hi = f.anchors.iter().map(|a| a.rva as usize + a.hex.len() / 2).max().unwrap();
    let text_va = (lo as i64 + code_delta) as u32;
    let mut code = vec![0xCCu8; hi - lo as usize + 0x100];
    for a in &f.anchors {
        let mut bytes = unhex(&a.hex).unwrap();
        let new_rva = (a.rva as i64 + code_delta) as u32;
        for fl in &a.fields {
            let target = (fl.target as i64 + if fl.kind == "rip" { data_delta } else { code_delta }) as i64;
            let disp = (target - (new_rva as i64 + fl.end as i64)) as i32;
            bytes[fl.off..fl.off + 4].copy_from_slice(&disp.to_le_bytes());
        }
        let at = (new_rva - text_va) as usize;
        code[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    let mut extra: Vec<(u32, u32, u32)> = vec![];
    for c in &f.constants {
        let page = ((c.rva as i64 + data_delta) as u32) & !0xFFF;
        if page >= text_va && page < text_va + code.len() as u32 + 0x1000 {
            continue;
        }
        if !extra.iter().any(|(va, _, _)| *va == page) {
            extra.push((page, 0x1000, c.flags));
        }
    }
    extra.sort();
    (synthetic_pe(stamp, text_va, &code, &extra), text_va)
}
fn write_exe(dir: &Path, bytes: &[u8]) -> PathBuf {
    let p = dir.join("NBA2K27.exe");
    fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn catalog_covers_every_module_proof_exactly() {
    let c = catalog().unwrap();
    assert_eq!((c.timestamp.as_str(), c.image_size.as_str()), ("6AC53765", "35BDE000"));
    for spec in specs() {
        let f = c.features.iter().find(|f| f.key == spec.key).unwrap_or_else(|| panic!("{} missing", spec.key));
        assert_eq!(f.anchors.len(), spec.anchors.len(), "{}", spec.key);
        for ((label, rva, proof), a) in spec.anchors.iter().zip(&f.anchors) {
            assert_eq!(a.rva, *rva, "{label}");
            assert!(unhex(&a.hex).unwrap().starts_with(proof), "{label}: catalog bytes differ from the module proof");
            assert_eq!(a.reference_matches, 1, "{label}: not unique in the verified build");
            // Re-decoding the catalog bytes reproduces the recorded fields.
            assert_eq!(operand_fields(&unhex(&a.hex).unwrap(), a.rva), a.fields, "{label}");
        }
        let targets: Vec<u32> = f.anchors.iter().flat_map(|a| a.fields.iter().map(|x| x.target)).collect();
        for k in &spec.constants {
            assert!(
                targets.contains(k),
                "{}: pinned global 0x{k:08X} has no relocatable code reference",
                spec.key
            );
        }
    }
}

#[test]
fn operand_masking_finds_rip_and_rel32_fields() {
    // lea rcx,[rip+0x08CFEDE2] ; call rel32 ; mov dword [rip+X], imm32
    let bytes = [
        0x48, 0x8D, 0x0D, 0xE2, 0xED, 0xCF, 0x08, 0xE8, 1, 0, 0, 0, 0xC7, 0x05, 0x10, 0, 0, 0, 7, 0, 0, 0,
    ];
    let f = operand_fields(&bytes, 0xC2DA40);
    assert_eq!(f.len(), 3);
    assert_eq!((f[0].off, f[0].end, f[0].kind.as_str(), f[0].target), (3, 7, "rip", 0xC2DA40 + 7 + 0x08CFEDE2));
    assert_eq!((f[1].off, f[1].kind.as_str()), (8, "rel"));
    assert_eq!((f[2].off, f[2].end), (14, 22), "imm32 follows the displacement");
}

#[test]
fn shifted_update_rederives_mods_and_camera_globals() {
    let c = catalog().unwrap();
    for (key, code_delta, data_delta) in [("mods", 0x2_3450i64, 0x8_1000i64), ("cameras", -0x1_2000, 0x4_0000)] {
        let f = c.features.iter().find(|f| f.key == key).unwrap();
        let dir = temp(key);
        let (pe, _) = shifted(f, code_delta, data_delta, 0x6AD0_0001);
        let exe = write_exe(&dir, &pe);
        let image = Image::open(&exe).unwrap();
        let r = resolve_image_with(&image, &c, Some(&[key]), false, false).pop().unwrap();
        assert_eq!(r.found, f.anchors.len(), "{key}: {:?} {:?}", r.missing, r.ambiguous);
        assert_eq!(r.derived, f.constants.len(), "{key}");
        for a in &f.anchors {
            assert_eq!(r.relocations[&hex32(a.rva)], hex32((a.rva as i64 + code_delta) as u32), "{key} anchor");
        }
        for k in &f.constants {
            assert_eq!(r.relocations[&hex32(k.rva)], hex32((k.rva as i64 + data_delta) as u32), "{key} global");
        }
        // Mods has no blockers: re-derived. Cameras keeps its blocker.
        assert_eq!(r.status, if key == "mods" { "Re-derived" } else { "Needs mapping" }, "{}", r.message);
        if key == "cameras" {
            assert!(r.message.contains("absolute game addresses"), "{}", r.message);
        }
        drop(image);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn changed_or_duplicated_anchors_fail_closed() {
    let c = catalog().unwrap();
    let f = c.features.iter().find(|f| f.key == "mods").unwrap();
    // One changed exact byte (e.g. a struct offset moved): needs mapping.
    let dir = temp("changed");
    let (mut pe, text_va) = shifted(f, 0x1000, 0x1000, 0x6AD0_0002);
    let a = &f.anchors[0];
    let mask: Vec<bool> = a.mask.chars().map(|c| c == '?').collect();
    let exact = mask.iter().position(|m| !m).unwrap();
    let at = 0x1000 + (a.rva + 0x1000 - text_va) as usize + exact;
    pe[at] ^= 0xFF;
    let exe = write_exe(&dir, &pe);
    let image = Image::open(&exe).unwrap();
    let r = resolve_image_with(&image, &c, Some(&["mods"]), false, false).pop().unwrap();
    assert_eq!(r.status, "Needs mapping");
    assert_eq!(r.missing, vec![a.label.clone()]);
    assert!(r.message.contains("needs a new mapping"), "{}", r.message);
    drop(image);
    fs::remove_dir_all(&dir).unwrap();
    // The same anchor twice: ambiguous, never a guess.
    let dir = temp("duplicate");
    let (mut pe, _) = shifted(f, 0x1000, 0x1000, 0x6AD0_0003);
    let bytes = unhex(&a.hex).unwrap();
    let tail = pe.len() - 0x100;
    pe[tail..tail + bytes.len()].copy_from_slice(&bytes);
    let exe = write_exe(&dir, &pe);
    let image = Image::open(&exe).unwrap();
    let r = resolve_image_with(&image, &c, Some(&["mods"]), false, false).pop().unwrap();
    assert_eq!(r.status, "Needs mapping");
    assert_eq!(r.ambiguous, vec![a.label.clone()]);
    drop(image);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn dependencies_and_blocker_only_features_stay_off_on_updates() {
    let c = catalog().unwrap();
    let f = c.features.iter().find(|f| f.key == "mods").unwrap();
    let dir = temp("deps");
    let (pe, _) = shifted(f, 0x1000, 0x1000, 0x6AD0_0004);
    let exe = write_exe(&dir, &pe);
    let image = Image::open(&exe).unwrap();
    let all = resolve_image_with(&image, &c, None, false, false);
    for r in &all {
        if r.key == "mods" {
            assert_eq!(r.status, "Re-derived");
        } else {
            assert_eq!(r.status, "Needs mapping", "{}", r.key);
            assert!(r.message.starts_with("Game updated: "), "{}", r.message);
        }
    }
    drop(image);
    fs::remove_dir_all(dir).unwrap();
}

/// Read-only: the verified installed build must map every feature, and a
/// full signature search (as for an unknown update) must land on today's
/// exact RVAs and globals.
#[test]
#[ignore]
fn installed_exe_relocates_to_pinned_rvas() {
    let image = Image::open(Path::new(INSTALLED)).unwrap();
    let c = catalog().unwrap();
    for r in resolve_image(&image, &c, None) {
        assert_eq!(r.status, "Mapped", "{}: {}", r.key, r.message);
    }
    let started = std::time::Instant::now();
    for r in resolve_image_with(&image, &c, None, false, true) {
        let f = c.features.iter().find(|f| f.key == r.key).unwrap();
        assert_eq!(r.found, f.anchors.len(), "{}: {:?} {:?}", r.key, r.missing, r.ambiguous);
        assert_eq!(r.derived, f.constants.len(), "{}", r.key);
        for (from, to) in &r.relocations {
            assert_eq!(from, to, "{}: {from} relocated to {to}", r.key);
        }
        println!("{}: {} anchors, {} globals, identity mapping", r.key, r.found, r.derived);
    }
    println!("full search {:?}", started.elapsed());
}

/// Regenerates assets/v2-relocation-catalog.json from the verified build.
#[test]
#[ignore]
fn generate_relocation_catalog() {
    let c = generate(Path::new(INSTALLED)).unwrap();
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/v2-relocation-catalog.json");
    fs::write(&out, serde_json::to_string_pretty(&c).unwrap() + "\n").unwrap();
    for f in &c.features {
        let ambiguous = f.anchors.iter().filter(|a| a.reference_matches != 1).count();
        println!("{}: {} anchors ({} not unique), {} globals", f.key, f.anchors.len(), ambiguous, f.constants.len());
    }
}
