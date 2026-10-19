// Included only by cfg(test). No production disk-audit or fixture capability.
fn version_two_fixture(hash: u32) -> Fixture {
    let mut f = fixture(hash);
    let base = f.base();
    let old = observer_profile::LEGACY;
    let new = observer_profile::VERSION_TWO;
    let entries = std::mem::take(&mut f.data);
    for (address, mut bytes) in entries {
        let off = address - base;
        let mut next = off;
        for (from, to, length) in [
            (old.root, new.root, 0x1158),
            (old.lineup, new.lineup, 0x200),
            (old.modes, new.modes, 0x2000),
            (old.owner, new.owner, 8),
            (old.active, new.active, 1),
        ] {
            if off >= from && off < from + length {
                next = to + (off - from);
                break;
            }
        }
        if bytes.len() == 8 {
            let value = u64::from_le_bytes(bytes.as_slice().try_into().unwrap());
            for (from, to) in [
                (old.root_tag, new.root_tag),
                (old.model, new.model),
                (old.description, new.description),
            ] {
                if value == base + from {
                    bytes = (base + to).to_le_bytes().to_vec();
                    break;
                }
            }
        }
        f.put(next, &bytes);
    }
    let header = f.data.get_mut(&base).unwrap();
    header[136..140].copy_from_slice(&0x6AC53765u32.to_le_bytes());
    header[208..212].copy_from_slice(&0x35BDE000u32.to_le_bytes());
    header[152..154].copy_from_slice(&0x20Bu16.to_le_bytes());
    f.data.remove(&(base + 0x3ACB20));
    f.data.remove(&(base + 0x2610815));
    for &(at, prefix, target) in observer_profile::RIP_ANCHORS {
        let mut bytes = prefix.to_vec();
        bytes.extend_from_slice(
            &(target as i64 - at as i64 - prefix.len() as i64 - 4)
                .try_into()
                .map(i32::to_le_bytes)
                .unwrap(),
        );
        f.put(at, &bytes);
    }
    for &(at, bytes) in observer_profile::FIELD_ANCHORS {
        f.put(at, bytes);
    }
    for (at, methods) in [
        (new.root_tag, [0x23120u64, 0x346B350]),
        (new.model, [0x3D8A90, 0x3D8AF0]),
        (new.description, [0x14F0, 0x827040]),
    ] {
        let bytes = methods
            .into_iter()
            .flat_map(|method| (base + method).to_le_bytes())
            .collect::<Vec<_>>();
        f.put(at, &bytes);
    }
    f
}
#[test]
fn version_two_readonly_boundaries_match_legacy_without_reusing_old_addresses() {
    for hash in [
        0x92DF06DE, 0x6509837B, 0x079964C0, 0xB4D37B22, 0x3EEF98B9, 0x7C602DF8, 0x50BAD898,
    ] {
        let f = version_two_fixture(hash);
        let p = observer_profile::validate(&f).unwrap();
        let got = snapshot_with_profile(&f, &p).unwrap();
        let expected = snapshot(&fixture(hash)).unwrap();
        assert_eq!(
            serde_json::to_value(got).unwrap(),
            serde_json::to_value(expected).unwrap()
        );
        assert!(
            validate_build(&f).is_err(),
            "Observer support must not admit legacy writers"
        );
        assert!(
            snapshot(&f).is_err(),
            "Ordinary legacy read/write consumers stay blocked"
        );
    }
}
#[test]
fn version_two_observer_rechecks_every_code_target_and_class_table() {
    for &(at, _, _) in observer_profile::RIP_ANCHORS {
        let mut f = version_two_fixture(0x92DF06DE);
        let key = f.base() + at;
        // Preserve the opcode, corrupt only its displacement.
        *f.data.get_mut(&key).unwrap().last_mut().unwrap() ^= 1;
        assert!(
            observer_profile::validate(&f).is_err(),
            "Accepted changed RIP target {at:X}"
        );
    }
    for at in observer_profile::FIELD_ANCHORS.iter().map(|a| a.0).chain([
        observer_profile::VERSION_TWO.root_tag,
        observer_profile::VERSION_TWO.model,
        observer_profile::VERSION_TWO.description,
    ]) {
        let mut f = version_two_fixture(0x92DF06DE);
        let key = f.base() + at;
        f.data.get_mut(&key).unwrap()[0] ^= 1;
        assert!(
            observer_profile::validate(&f).is_err(),
            "Accepted changed signature/table {at:X}"
        );
    }
    let mut f = version_two_fixture(0x92DF06DE);
    f.torn = true;
    let p = observer_profile::validate(&f).unwrap();
    assert!(snapshot_with_profile(&f, &p).is_err());
    let mut f = version_two_fixture(0x92DF06DE);
    f.d(observer_profile::VERSION_TWO.lineup + 8, 6);
    let p = observer_profile::validate(&f).unwrap();
    assert!(snapshot_with_profile(&f, &p).is_err());
}
#[test]
fn version_two_observer_never_grants_a_writable_process_capability() {
    let id = Identity {
        pid: 0x7fff_fffe,
        started: "1".into(),
        path: "not-a-game".into(),
    };
    // Both exact query/read masks are read-only. This is policy inspection
    // only; accepting the mask does not bypass path/identity/build admission.
    for base in [
        PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ,
        PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
    ] {
        assert!(observer_rights(base));
        for extra in [
            PROCESS_VM_WRITE,
            PROCESS_VM_OPERATION,
            PROCESS_CREATE_THREAD,
            PROCESS_TERMINATE,
            PROCESS_SUSPEND_RESUME,
        ] {
            let rights = base | extra;
            assert!(!observer_rights(rights));
            // Deliberately invalid path/identity: the rights rejection must
            // happen first, before even attempting OpenProcess.
            let error = Reader::open_access_kind(&id, Path::new("not-a-game"), rights, true)
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("remain read-only"), "{error}");
        }
    }
}
#[test]
#[ignore = "Explicit disk-only Version 2 profile audit; requires C27_OBSERVER_DISK_AUDIT and never opens a game process"]
fn installed_version_two_observer_disk_audit() {
    use std::{
        fs,
        io::{Read, Seek, SeekFrom},
        os::windows::fs::OpenOptionsExt,
        sync::Mutex,
    };
    struct Disk {
        file: Mutex<fs::File>,
        base: u64,
        len: u64,
        sections: Vec<(u64, u64, u64)>,
    }
    impl Memory for Disk {
        fn base(&self) -> u64 {
            self.base
        }
        fn read(&self, at: u64, len: usize) -> Result<Vec<u8>> {
            ensure!(len > 0 && len <= 4096, "Invalid disk-audit read size");
            let rva = at.checked_sub(self.base).context("Disk RVA underflow")?;
            let raw = if rva + (len as u64) <= 4096 {
                rva
            } else {
                let matches = self
                    .sections
                    .iter()
                    .filter(|(start, n, _)| {
                        rva >= *start
                            && rva
                                .checked_add(len as u64)
                                .is_some_and(|end| end <= start + n)
                    })
                    .collect::<Vec<_>>();
                ensure!(matches.len() == 1, "Ambiguous/unmapped disk RVA");
                matches[0].2 + rva - matches[0].0
            };
            ensure!(
                raw.checked_add(len as u64)
                    .is_some_and(|end| end <= self.len),
                "Disk read escapes image"
            );
            let mut file = self.file.lock().unwrap();
            file.seek(SeekFrom::Start(raw))?;
            let mut bytes = vec![0; len];
            file.read_exact(&mut bytes)?;
            Ok(bytes)
        }
    }
    let path =
        std::env::var_os("C27_OBSERVER_DISK_AUDIT").expect("Pass the explicit installed EXE path");
    let path = std::path::PathBuf::from(path);
    assert!(
        path.file_name()
            .unwrap()
            .to_string_lossy()
            .eq_ignore_ascii_case("NBA2K27.exe")
    );
    let mut file = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let len = file.metadata().unwrap().len();
    let mut header = [0; 4096];
    file.read_exact(&mut header).unwrap();
    let pe = u32::from_le_bytes(header[60..64].try_into().unwrap()) as usize;
    assert!((64..3800).contains(&pe));
    let optional = u16::from_le_bytes(header[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(header[pe + 6..pe + 8].try_into().unwrap()) as usize;
    assert!((1..=96).contains(&count) && pe + 24 + optional + 40 * count <= 4096);
    let base = u64::from_le_bytes(header[pe + 48..pe + 56].try_into().unwrap());
    let mut sections = Vec::new();
    for i in 0..count {
        let at = pe + 24 + optional + 40 * i;
        let u = |offset| {
            u32::from_le_bytes(header[at + offset..at + offset + 4].try_into().unwrap()) as u64
        };
        let (rva, n, raw) = (u(12), u(16), u(20));
        assert!(raw + n <= len);
        sections.push((rva, n, raw));
    }
    let disk = Disk {
        file: Mutex::new(file),
        base,
        len,
        sections,
    };
    let p = observer_profile::validate(&disk).unwrap();
    assert_eq!(p.root, observer_profile::VERSION_TWO.root);
    assert!(
        validate_build(&disk).is_err(),
        "Version 2 must still fail the legacy write gate"
    );
    use sha2::Digest;
    let mut hash = sha2::Sha256::new();
    let mut file = disk.file.lock().unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buffer = vec![0; 65536];
    loop {
        let n = file.read(&mut buffer).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    let sha = format!("{:X}", hash.finalize());
    assert_eq!(
        sha,
        "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
    );
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("artifacts")
        .join(format!(
            "version2-observer-disk-{}",
            crate::backend::unique_id()
        ));
    fs::create_dir(&root).unwrap();
    let report = serde_json::json!({
        "schema":"nba2k27-version2-observer-disk-proof-v1","sha256":sha,
        "profileCodeTargetsFieldsAndTablesPassed":true,"legacyWriteGateRejected":true,
        "gameProcessAccess":false,"gameFilesChanged":false,"runtimeVerification":false,
        "ripAnchors":observer_profile::RIP_ANCHORS.len(),"fieldAnchors":observer_profile::FIELD_ANCHORS.len(),"primaryClassTables":3
    });
    fs::write(
        root.join("report.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("{}", root.display());
}

#[test]
fn blocked_observer_reports_bounded_identity_metadata_without_paths_or_weakening_guard() {
    let expected = Identity {
        pid: 22,
        started: "123".into(),
        path: r"C:\Unavailable\NBA2K27.exe".into(),
    };
    let rows = (0..7)
        .map(|n| Process {
            name: format!("2k-launcher-{}{}.exe", "x".repeat(120), n),
            pid: 100 + n,
            parent: 10,
            identity: Some(Identity {
                pid: 100 + n,
                started: "777".into(),
                path: r"C:\Private\secret.exe".into(),
            }),
        })
        .collect::<Vec<_>>();
    let err = guard(&rows, &expected).unwrap_err().to_string();
    assert!(
        err.contains("Another launcher is active")
            && err.contains("PID 100, parent 10, identity read")
    );
    assert!(
        err.contains("3 more")
            && !err.contains("PID 104")
            && !err.contains("Private")
            && !err.contains("secret.exe")
    );
    assert!(err.len() < 750);
    let unresolved = Process {
        name: "node.exe".into(),
        pid: 200,
        parent: 20,
        identity: None,
    };
    let err = guard(&[unresolved], &expected).unwrap_err().to_string();
    assert!(err.contains("PID 200, parent 20, identity unavailable"));
}

#[test]
fn live_identity_path_transport_never_reuses_a_failed_or_old_path_proof() -> Result<()> {
    use std::cell::Cell;
    let calls = Cell::new(0);
    let query = |text: &str| {
        fresh_image_path(|bytes, len| {
            assert_eq!(*len, 32768);
            calls.set(calls.get() + 1);
            let value = text.encode_utf16().collect::<Vec<_>>();
            bytes[..value.len()].copy_from_slice(&value);
            *len = value.len() as u32;
            Ok(())
        })
    };
    assert_eq!(
        query(r"C:\Long\Original\NBA2K27.exe")?,
        r"C:\Long\Original\NBA2K27.exe"
    );
    assert_eq!(query("short")?, "short");
    assert!(fresh_image_path(|_, _| anyhow::bail!("fresh kernel query failed")).is_err());
    assert_eq!(query("new")?, "new");
    assert_eq!(calls.get(), 3);
    for len in [0, 32769] {
        assert!(
            fresh_image_path(|_, n| {
                *n = len;
                Ok(())
            })
            .is_err()
        );
    }
    assert!(
        fresh_image_path(|bytes, len| {
            bytes[0] = 0xD800;
            *len = 1;
            Ok(())
        })
        .is_err()
    );
    // Reentrant kernel-adapter fixtures get independent mutable storage.
    let outer = fresh_image_path(|bytes, len| {
        bytes[0] = 'A' as u16;
        *len = 1;
        assert_eq!(query("nested")?, "nested");
        assert_eq!(bytes[0], 'A' as u16);
        Ok(())
    })?;
    assert_eq!(outer, "A");
    let threads = (0..8)
        .map(|n| {
            std::thread::spawn(move || {
                for _ in 0..100 {
                    let expected = format!("thread-{n}");
                    let actual = fresh_image_path(|bytes, len| {
                        let value = expected.encode_utf16().collect::<Vec<_>>();
                        bytes[..value.len()].copy_from_slice(&value);
                        *len = value.len() as u32;
                        Ok(())
                    })
                    .unwrap();
                    assert_eq!(actual, expected);
                }
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
    Ok(())
}
