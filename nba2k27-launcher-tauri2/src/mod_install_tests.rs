use super::*;
use std::io::Write;
use zip::write::SimpleFileOptions;

#[test]
fn retained_recovery_survives_restart_refuses_drift_and_cannot_replay() {
    let f = Fixture::new();
    let destination = f.mods.join("char/sig/png1234.iff");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"original").unwrap();
    let input = f.file("png1234.iff", b"new-content");
    let installer = f.installer();
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &[input],
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    let logical = plan["items"][0]["logicalPath"].as_str().unwrap();
    let result = installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":[logical],"keep":[]}),
        )
        .unwrap();
    let id = result["recoveryId"].as_str().unwrap();
    drop(installer);
    let restarted = f.installer();
    assert_eq!(
        restarted
            .history(&f.mods)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    fs::write(&destination, b"user-edited").unwrap();
    assert!(
        restarted
            .restore(&f.mods, id)
            .unwrap_err()
            .to_string()
            .contains("changed since")
    );
    assert_eq!(fs::read(&destination).unwrap(), b"user-edited");
    fs::write(&destination, b"new-content").unwrap();
    restarted.restore(&f.mods, id).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"original");
    assert!(restarted.restore(&f.mods, id).is_err());
}

#[test]
fn recovery_refuses_wrong_profile_and_corrupt_backup() {
    let f = Fixture::new();
    let destination = f.mods.join("char/sig/png1234.iff");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, b"original").unwrap();
    let input = f.file("png1234.iff", b"new-content");
    let installer = f.installer();
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &[input],
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    let logical = plan["items"][0]["logicalPath"].as_str().unwrap();
    let result = installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":[logical],"keep":[]}),
        )
        .unwrap();
    let id = result["recoveryId"].as_str().unwrap();
    assert!(installer.restore(&f.input, id).is_err());
    let receipt: Value =
        serde_json::from_slice(&fs::read(installer.recovery.join(format!("{id}.json"))).unwrap())
            .unwrap();
    let backup = destination
        .parent()
        .unwrap()
        .join(receipt["items"][0]["backup"].as_str().unwrap());
    fs::write(backup, b"corrupt").unwrap();
    assert!(installer.restore(&f.mods, id).is_err());
    assert_eq!(fs::read(destination).unwrap(), b"new-content");
}

struct Fixture {
    dir: PathBuf,
    game: PathBuf,
    mods: PathBuf,
    input: PathBuf,
    data: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "mod-install-test-{}-{}",
            std::process::id(),
            crate::backend::unique_id()
        ));
        let (game, input, data) = (dir.join("game"), dir.join("input"), dir.join("portable"));
        let mods = game.join("mods");
        for d in [&game, &mods, &input, &data] {
            fs::create_dir_all(d).unwrap();
        }
        fs::write(
            game.join("manifest"),
            "char/sig/png1234.iff,1,2\r\n\
             arenas/arena_999.iff,1,2\n\
             a/team1/common.iff,1,2\n\
             b/team2/common.iff,1,2\n\
             loc/english.iff,1,2\n\
             ../escape.iff,1,2\n",
        )
        .unwrap();
        Self {
            dir,
            game,
            mods,
            input,
            data,
        }
    }
    fn file(&self, relative: &str, body: &[u8]) -> PathBuf {
        let path = self.input.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, body).unwrap();
        path
    }
    fn zip(&self, name: &str, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = self.input.join(name);
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (entry, body) in entries {
            if entry.ends_with('/') {
                zip.add_directory(*entry, SimpleFileOptions::default())
                    .unwrap();
            } else {
                zip.start_file(*entry, SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(body).unwrap();
            }
        }
        zip.finish().unwrap();
        path
    }
    fn installer(&self) -> Installer {
        Installer::new(&self.data)
    }
    fn plan(&self, paths: &[PathBuf], mode: Mode) -> Result<Value> {
        self.installer().plan(
            &self.game,
            &self.mods,
            paths,
            mode,
            &tar_executable(),
            &Limits::default(),
        )
    }
    fn staging_empty(&self) -> bool {
        fs::read_dir(self.data.join("Data/mod-install"))
            .map(|mut d| d.next().is_none())
            .unwrap_or(true)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
fn names(plan: &Value, field: &str) -> Vec<String> {
    plan[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            v["logicalPath"]
                .as_str()
                .or(v["name"].as_str())
                .unwrap()
                .to_string()
        })
        .collect()
}
fn no_escape(f: &Fixture) {
    for leaked in [
        f.dir.join("evil.iff"),
        f.input.join("evil.iff"),
        f.data.join("evil.iff"),
        f.data.join("Data/evil.iff"),
        f.game.join("escape.iff"),
    ] {
        assert!(!leaked.exists(), "{leaked:?} escaped the staging area");
    }
}

#[test]
fn safe_entry_rejects_traversal_absolute_devices_and_controls() {
    for bad in [
        "../evil.iff",
        "a/../../evil.iff",
        "..\\evil.iff",
        "/evil.iff",
        "\\evil.iff",
        "C:/evil.iff",
        "c:evil.iff",
        "a//b.iff",
        "a/./b.iff",
        "CON",
        "nul.iff",
        "com1.txt",
        "a/LPT9",
        "bad\u{1}.iff",
        "a:b.iff",
        "what?.iff",
        "trailing.",
        "trailing ",
        "",
    ] {
        assert_eq!(safe_entry(bad), None, "{bad:?} must be refused");
    }
    assert_eq!(
        safe_entry("./char/sig/png1.iff").as_deref(),
        Some("char/sig/png1.iff")
    );
    assert_eq!(
        safe_entry("pack\\char\\x.iff/").as_deref(),
        Some("pack/char/x.iff")
    );
    assert_eq!(safe_entry("console.iff").as_deref(), Some("console.iff"));
}

#[test]
fn routing_matches_wpf_rules() {
    let index = ManifestIndex::parse(std::io::Cursor::new(
        "char/sig/png1234.iff,1\na/team1/common.iff,1\nb/team2/common.iff,1\narenas/arena_999.iff,1\nnocomma\n,leading\n",
    ))
    .unwrap();
    assert!(index.roots.contains("char") && index.roots.contains("arenas"));
    assert_eq!(index.leaves.len(), 3);
    assert_eq!(
        loose_player_logical("PNG12345_hair.iff").as_deref(),
        Some("char/sig/PNG12345_hair.iff")
    );
    for bad in [
        "png123.iff",
        "png1234567.iff",
        "png1234x.iff",
        "png1234.bin",
    ] {
        assert_eq!(loose_player_logical(bad), None, "{bad}");
    }
    let roots = &index.roots;
    assert_eq!(
        rooted_package_logical(Some("My Pack/arenas/custom/x.iff"), roots).as_deref(),
        Some("arenas/custom/x.iff")
    );
    assert_eq!(
        rooted_package_logical(Some("My Pack/other/x.iff"), roots),
        None
    );
    let both = index.leaves["common.iff"].clone();
    assert_eq!(
        manifest_match(&both, Path::new(r"C:\x\team2\common.iff")).as_deref(),
        Some("b/team2/common.iff")
    );
    assert_eq!(
        manifest_match(&both, Path::new(r"C:\x\other\common.iff")),
        None
    );
    assert!(executable("Setup.EXE") && executable("loader.asi") && !executable("png1.iff"));
}

#[test]
fn loose_files_route_into_the_active_mods_folder_only() {
    let f = Fixture::new();
    let files = [
        f.file("arena_999.iff", b"arena"),
        f.file("png1234_hair.iff", b"hair"),
        f.file("x/team1/common.iff", b"common"),
    ];
    let plan = f.plan(&files, Mode::Files).unwrap();
    assert_eq!(plan["issues"].as_array().unwrap().len(), 0, "{plan}");
    let mut routed = names(&plan, "items");
    routed.sort();
    assert_eq!(
        routed,
        [
            "a/team1/common.iff",
            "arenas/arena_999.iff",
            "char/sig/png1234_hair.iff"
        ]
    );
    let installer = f.installer();
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    let result = installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":[],"keep":[]}),
        )
        .unwrap();
    assert_eq!(result["installed"], 3);
    assert_eq!(
        fs::read(f.mods.join("arenas/arena_999.iff")).unwrap(),
        b"arena"
    );
    assert_eq!(
        fs::read(f.mods.join("char/sig/png1234_hair.iff")).unwrap(),
        b"hair"
    );
    assert!(!f.game.join("arenas").exists(), "game files were touched");
    let leftovers: Vec<_> = fs::read_dir(f.mods.join("arenas"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".mod-install-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn unknown_ambiguous_and_executable_loose_files_block_the_install() {
    let f = Fixture::new();
    let files = [
        f.file("mystery.iff", b"x"),
        f.file("y/elsewhere/common.iff", b"x"),
        f.file("tool.exe", b"MZ"),
    ];
    let plan = f.plan(&files, Mode::Files).unwrap();
    assert!(plan["planId"].is_null());
    let reasons: Vec<_> = plan["issues"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["reason"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(reasons.len(), 3, "{plan}");
    assert!(reasons.iter().any(|r| r.contains("not listed")));
    assert!(reasons.iter().any(|r| r.contains("ambiguous")));
    assert!(reasons.iter().any(|r| r.contains("never installed")));
    assert!(fs::read_dir(&f.mods).unwrap().next().is_none());
}

#[test]
fn folder_packages_keep_structure_and_ignore_docs_and_programs() {
    let f = Fixture::new();
    f.file("Pack/arenas/custom/court.iff", b"court");
    f.file("Pack/readme.txt", b"docs");
    f.file("Pack/preview.png", b"img");
    f.file("Pack/install.bat", b"del");
    f.file("Pack/empty.iff", b"");
    let plan = f.plan(&[f.input.join("Pack")], Mode::Folder).unwrap();
    assert_eq!(names(&plan, "items"), ["arenas/custom/court.iff"]);
    assert_eq!(plan["ignored"].as_array().unwrap().len(), 3, "{plan}");
    assert_eq!(plan["folders"], 1);
    assert!(
        f.plan(&[f.input.join("Pack"), f.input.clone()], Mode::Folder)
            .is_err()
    );
}

#[test]
fn folder_with_junction_is_refused() {
    let f = Fixture::new();
    f.file("Pack/arenas/arena_999.iff", b"a");
    let link = f.input.join("Pack").join("linked");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&f.game)
        .output()
        .unwrap();
    assert!(status.status.success(), "could not create junction fixture");
    let error = f.plan(&[f.input.join("Pack")], Mode::Folder).unwrap_err();
    assert!(format!("{error:#}").contains("linked"), "{error:#}");
    fs::remove_dir(&link).unwrap();
}

#[test]
fn zip_packages_stage_in_data_and_install() {
    let f = Fixture::new();
    let zip = f.zip(
        "pack.zip",
        &[
            ("Pack/", b""),
            ("Pack/arenas/custom/court.iff", b"court"),
            ("Pack/char/sig/png1234.iff", b"player"),
            ("Pack/readme.txt", b"docs"),
        ],
    );
    let installer = f.installer();
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &[zip],
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    assert_eq!(plan["archives"], 1);
    let mut routed = names(&plan, "items");
    routed.sort();
    assert_eq!(routed, ["arenas/custom/court.iff", "char/sig/png1234.iff"]);
    assert!(!f.staging_empty(), "archive content is staged in Data");
    installer
        .commit(&f.mods, &json!({"planId":plan["planId"]}))
        .unwrap();
    assert_eq!(
        fs::read(f.mods.join("char/sig/png1234.iff")).unwrap(),
        b"player"
    );
    assert!(f.staging_empty(), "staging is removed after commit");
}

#[test]
fn malicious_zips_are_refused_before_anything_is_written() {
    let f = Fixture::new();
    let cases: Vec<(&str, PathBuf, &str)> = vec![
        (
            "traversal",
            f.zip("t.zip", &[("ok.iff", b"1"), ("../evil.iff", b"x")]),
            "unsafe path",
        ),
        (
            "deep-traversal",
            f.zip("d.zip", &[("a/../../../evil.iff", b"x")]),
            "unsafe path",
        ),
        (
            "absolute",
            f.zip("a.zip", &[("/evil.iff", b"x")]),
            "unsafe path",
        ),
        (
            "drive",
            f.zip("c.zip", &[("C:/evil.iff", b"x")]),
            "unsafe path",
        ),
        (
            "backslash",
            f.zip("b.zip", &[("..\\evil.iff", b"x")]),
            "unsafe path",
        ),
        (
            "device",
            f.zip("n.zip", &[("arenas/NUL.iff", b"x")]),
            "unsafe path",
        ),
        (
            "duplicate",
            f.zip("u.zip", &[("A.iff", b"1"), ("a.IFF", b"2")]),
            "duplicate",
        ),
        (
            "file-vs-dir",
            f.zip("v.zip", &[("x", b"1"), ("X/y.iff", b"2")]),
            "duplicate",
        ),
        ("empty", f.zip("e.zip", &[]), "empty"),
    ];
    for (label, archive, expected) in cases {
        let error = f.plan(&[archive], Mode::Files).unwrap_err();
        let text = format!("{error:#}");
        assert!(text.contains(expected), "{label}: {text}");
        no_escape(&f);
        assert!(f.staging_empty(), "{label}: staging left behind");
    }
    // Symbolic-link entry.
    let link = f.input.join("link.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&link).unwrap());
    zip.add_symlink(
        "arenas/link.iff",
        "C:/Windows/win.ini",
        SimpleFileOptions::default(),
    )
    .unwrap();
    zip.finish().unwrap();
    let error = f.plan(&[link], Mode::Files).unwrap_err();
    assert!(format!("{error:#}").contains("links"), "{error:#}");
    // Encrypted flag set in the local and central headers.
    let encrypted = f.zip("enc.zip", &[("arenas/arena_999.iff", b"secret")]);
    let mut bytes = fs::read(&encrypted).unwrap();
    for (signature, flag) in [([0x50, 0x4b, 3, 4], 6), ([0x50, 0x4b, 1, 2], 8)] {
        let at = bytes.windows(4).position(|w| w == signature).unwrap();
        bytes[at + flag] |= 1;
    }
    fs::write(&encrypted, bytes).unwrap();
    let error = f.plan(&[encrypted], Mode::Files).unwrap_err();
    assert!(format!("{error:#}").contains("Password"), "{error:#}");
    // Corrupt data fails the CRC check instead of installing garbage.
    let corrupt = f.input.join("crc.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&corrupt).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("arenas/arena_999.iff", stored).unwrap();
    zip.write_all(b"original-bytes").unwrap();
    zip.finish().unwrap();
    let mut bytes = fs::read(&corrupt).unwrap();
    let at = bytes
        .windows(14)
        .position(|w| w == b"original-bytes")
        .unwrap();
    bytes[at] = b'X';
    fs::write(&corrupt, bytes).unwrap();
    assert!(f.plan(&[corrupt], Mode::Files).is_err());
    assert!(fs::read_dir(&f.mods).unwrap().next().is_none());
    assert!(f.staging_empty());
}

#[test]
fn zip_size_and_count_limits_stop_bombs() {
    let f = Fixture::new();
    let zeros = vec![0u8; 1 << 20];
    let path = f.input.join("bomb.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("arenas/arena_999.iff", deflate).unwrap();
    zip.write_all(&zeros).unwrap();
    zip.finish().unwrap();
    assert!(fs::metadata(&path).unwrap().len() < 64 * 1024);
    let small = Limits {
        entries: 4,
        file_bytes: 64 * 1024,
        total_bytes: 128 * 1024,
    };
    let out = f.dir.join("bomb-out");
    fs::create_dir(&out).unwrap();
    let error = extract_zip(&path, &out, &small).unwrap_err();
    assert!(format!("{error:#}").contains("safety limit"), "{error:#}");
    assert!(fs::read_dir(&out).unwrap().next().is_none());
    let many: Vec<(String, &[u8])> = (0..5).map(|i| (format!("f{i}.iff"), &b"x"[..])).collect();
    let refs: Vec<(&str, &[u8])> = many.iter().map(|(n, b)| (n.as_str(), *b)).collect();
    let crowded = f.zip("many.zip", &refs);
    let error = extract_zip(&crowded, &out, &small).unwrap_err();
    assert!(
        format!("{error:#}").contains("entry safety limit"),
        "{error:#}"
    );
}

#[test]
fn limit_messages_use_wpf_wording() {
    let limits = Limits::default();
    let huge = "-rw-rw-r--  0 0      0  9999999999999 Oct 17  2026 big.iff
";
    assert_eq!(
        format!("{:#}", inspect_verbose_listing(huge, &limits).unwrap_err()),
        "The archive expands beyond the 4 GB safety limit."
    );
    assert_eq!(thousands(4096), "4,096");
    assert_eq!(size_text(4 << 30), "4 GB");
}

#[test]
fn rar_listing_checks_match_wpf() {
    let limits = Limits::default();
    let ok = "-rw-rw-r--  0 0      0          12 Oct 17  2026 Pack/arenas/a.iff\n\
              drwxrwxr-x  0 0      0           0 Oct 17  2026 Pack/\n";
    assert_eq!(inspect_verbose_listing(ok, &limits).unwrap(), (2, 12));
    let link = "lrwxrwxrwx  0 0      0           0 Oct 17  2026 Pack/x -> C:/Windows\n";
    assert!(format!("{:#}", inspect_verbose_listing(link, &limits).unwrap_err()).contains("links"));
    let hard = "hrw-rw-r--  0 0      0           0 Oct 17  2026 Pack/x link to y\n";
    assert!(inspect_verbose_listing(hard, &limits).is_err());
    let huge = "-rw-rw-r--  0 0      0  9999999999999 Oct 17  2026 big.iff\n";
    assert!(
        format!("{:#}", inspect_verbose_listing(huge, &limits).unwrap_err())
            .contains("safety limit")
    );
    assert!(inspect_verbose_listing("garbage line\n", &limits).is_err());
}

#[test]
fn missing_archive_tool_reports_rar_honestly() {
    let f = Fixture::new();
    let rar = f.file("pack.rar", b"Rar!\x1a\x07\x01\x00not-really");
    let error = f
        .installer()
        .plan(
            &f.game,
            &f.mods,
            &[rar],
            Mode::Files,
            &f.dir.join("missing-tar.exe"),
            &Limits::default(),
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("RAR archives need"),
        "{error:#}"
    );
    assert!(f.staging_empty());
}

#[test]
fn windows_tar_path_extracts_safely_when_available() {
    // RAR fixtures cannot be authored offline; tar.exe reads ZIP through the
    // same inspect-then-extract path the RAR install uses.
    let tar = tar_executable();
    if !tar.is_file() {
        eprintln!("System32\\tar.exe missing; skipping");
        return;
    }
    let f = Fixture::new();
    let zip = f.zip("via-tar.zip", &[("Pack/arenas/custom/court.iff", b"court")]);
    let out = f.dir.join("tar-out");
    fs::create_dir(&out).unwrap();
    extract_with_tar(&zip, &out, &tar, &Limits::default()).unwrap();
    assert_eq!(
        fs::read(out.join("Pack/arenas/custom/court.iff")).unwrap(),
        b"court"
    );
    let evil = f.zip("evil-tar.zip", &[("../evil.iff", b"x")]);
    let out = f.dir.join("tar-evil");
    fs::create_dir(&out).unwrap();
    assert!(extract_with_tar(&evil, &out, &tar, &Limits::default()).is_err());
    no_escape(&f);
}

#[test]
fn conflicts_need_a_decision_and_keep_or_replace_per_file() {
    let f = Fixture::new();
    fs::create_dir_all(f.mods.join("arenas")).unwrap();
    fs::create_dir_all(f.mods.join("loc")).unwrap();
    fs::write(f.mods.join("arenas/arena_999.iff"), b"old-arena").unwrap();
    fs::write(f.mods.join("loc/english.iff"), b"old-loc").unwrap();
    let files = [
        f.file("arena_999.iff", b"new-arena"),
        f.file("english.iff", b"new-loc"),
        f.file("png1234.iff", b"player"),
    ];
    let installer = f.installer();
    let review = |installer: &Installer| {
        installer
            .plan(
                &f.game,
                &f.mods,
                &files,
                Mode::Files,
                &tar_executable(),
                &Limits::default(),
            )
            .unwrap()
    };
    let plan = review(&installer);
    assert_eq!(plan["replacements"], 2);
    let existing: Vec<_> = plan["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| !i["existing"].is_null())
        .map(|i| i["logicalPath"].as_str().unwrap())
        .collect();
    assert_eq!(existing.len(), 2);
    // No decision → refused, nothing written, plan consumed.
    let error = installer
        .commit(&f.mods, &json!({"planId":plan["planId"]}))
        .unwrap_err();
    assert!(format!("{error:#}").contains("Replace or Keep"));
    assert!(!f.mods.join("char").exists());
    assert!(
        installer
            .commit(&f.mods, &json!({"planId":plan["planId"]}))
            .is_err()
    );
    // Both replace and keep for one file → refused.
    let plan = review(&installer);
    assert!(installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":["arenas/arena_999.iff","loc/english.iff"],"keep":["ARENAS/arena_999.iff"]})
        )
        .is_err());
    // Replace one, keep the other.
    let plan = review(&installer);
    let result = installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":["ARENAS/ARENA_999.IFF"],"keep":["loc/english.iff"]}),
        )
        .unwrap();
    assert_eq!(
        (
            result["installed"].as_u64(),
            result["replaced"].as_u64(),
            result["kept"].as_u64()
        ),
        (Some(2), Some(1), Some(1))
    );
    assert_eq!(
        fs::read(f.mods.join("arenas/arena_999.iff")).unwrap(),
        b"new-arena"
    );
    assert_eq!(
        fs::read(f.mods.join("loc/english.iff")).unwrap(),
        b"old-loc"
    );
    assert_eq!(
        fs::read(f.mods.join("char/sig/png1234.iff")).unwrap(),
        b"player"
    );
    let backups = fs::read_dir(f.mods.join("arenas"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with(".mod-install-"))
        .count();
    assert_eq!(backups, 1);
    assert_eq!(
        installer
            .history(&f.mods)
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let catalog = crate::backend::scan_mods(&f.mods).unwrap();
    assert!(
        catalog["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| !r["name"].as_str().unwrap().starts_with(".mod-install-"))
    );
}

#[test]
fn changes_after_review_roll_back_and_stale_plans_expire() {
    let f = Fixture::new();
    fs::create_dir_all(f.mods.join("arenas")).unwrap();
    fs::write(f.mods.join("arenas/arena_999.iff"), b"old").unwrap();
    let files = [f.file("arena_999.iff", b"new"), f.file("png1234.iff", b"p")];
    let installer = f.installer();
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    // Destination edited by someone else after the review.
    fs::write(f.mods.join("arenas/arena_999.iff"), b"edited-elsewhere").unwrap();
    let error = installer
        .commit(
            &f.mods,
            &json!({"planId":plan["planId"],"replace":["arenas/arena_999.iff"]}),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("changed"), "{error:#}");
    assert_eq!(
        fs::read(f.mods.join("arenas/arena_999.iff")).unwrap(),
        b"edited-elsewhere"
    );
    assert!(
        !f.mods.join("char/sig/png1234.iff").exists(),
        "partial install was not rolled back"
    );
    // Source edited after review.
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    fs::write(&files[1], b"different-length").unwrap();
    assert!(
        installer
            .commit(
                &f.mods,
                &json!({"planId":plan["planId"],"replace":["arenas/arena_999.iff"]})
            )
            .is_err()
    );
    // Wrong plan id or changed profile.
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    assert!(
        installer
            .commit(
                &f.mods,
                &json!({"planId":"other","replace":["arenas/arena_999.iff"]})
            )
            .is_err()
    );
    let _ = plan;
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    let other = f.dir.join("other-mods");
    fs::create_dir(&other).unwrap();
    let error = installer
        .commit(
            &other,
            &json!({"planId":plan["planId"],"replace":["arenas/arena_999.iff"]}),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("profile changed"));
}

#[test]
fn mods_folder_must_be_plain_and_never_the_game_folder() {
    let f = Fixture::new();
    let files = [f.file("arena_999.iff", b"a")];
    let limits = Limits::default();
    let installer = f.installer();
    let error = installer
        .plan(
            &f.game,
            &f.game,
            &files,
            Mode::Files,
            &tar_executable(),
            &limits,
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("dedicated mods folder"));
    let error = installer
        .plan(
            &f.game,
            &f.dir,
            &files,
            Mode::Files,
            &tar_executable(),
            &limits,
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("dedicated mods folder"));
    assert!(
        installer
            .plan(
                &f.game,
                &f.dir.join("missing"),
                &files,
                Mode::Files,
                &tar_executable(),
                &limits
            )
            .is_err()
    );
    let link = f.dir.join("linked-mods");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&f.mods)
        .output()
        .unwrap();
    assert!(made.status.success());
    let error = installer
        .plan(
            &f.game,
            &link,
            &files,
            Mode::Files,
            &tar_executable(),
            &limits,
        )
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("not a plain local directory")
            || format!("{error:#}").contains("linked")
    );
    fs::remove_dir(&link).unwrap();
    // A linked destination folder inside the mods folder is an issue.
    let arenas = f.mods.join("arenas");
    let made = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&arenas)
        .arg(&f.input)
        .output()
        .unwrap();
    assert!(made.status.success());
    let plan = installer
        .plan(
            &f.game,
            &f.mods,
            &files,
            Mode::Files,
            &tar_executable(),
            &limits,
        )
        .unwrap();
    assert!(plan["planId"].is_null());
    assert!(
        plan["issues"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("link")
    );
    fs::remove_dir(&arenas).unwrap();
}

#[test]
fn missing_manifest_and_cancel_clean_up_staging() {
    let f = Fixture::new();
    let zip = f.zip("pack.zip", &[("arenas/arena_999.iff", b"a")]);
    let installer = f.installer();
    installer
        .plan(
            &f.game,
            &f.mods,
            &[zip.clone()],
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap();
    assert!(!f.staging_empty());
    installer.cancel().unwrap();
    assert!(f.staging_empty());
    fs::remove_file(f.game.join("manifest")).unwrap();
    let error = installer
        .plan(
            &f.game,
            &f.mods,
            &[zip],
            Mode::Files,
            &tar_executable(),
            &Limits::default(),
        )
        .unwrap_err();
    assert!(format!("{error:#}").contains("stock manifest"));
    assert!(f.staging_empty());
    // Leftovers from an interrupted run are purged on the next plan.
    let stale = f.data.join("Data/mod-install/stage-old/archive-0");
    fs::create_dir_all(&stale).unwrap();
    let _ = installer.plan(
        &f.game,
        &f.mods,
        &[f.file("x.iff", b"x")],
        Mode::Files,
        &tar_executable(),
        &Limits::default(),
    );
    assert!(!stale.exists());
}

#[test]
fn zips_with_other_compression_go_to_windows_tar_like_wpf() {
    let f = Fixture::new();
    let path = f.input.join("bzip.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("arenas/arena_999.iff", stored).unwrap();
    zip.write_all(b"payload").unwrap();
    zip.finish().unwrap();
    // Re-label the entry as bzip2 (method 12) in both headers.
    let mut bytes = fs::read(&path).unwrap();
    for (signature, offset) in [([0x50, 0x4b, 3, 4], 8), ([0x50, 0x4b, 1, 2], 10)] {
        let at = bytes.windows(4).position(|w| w == signature).unwrap();
        bytes[at + offset] = 12;
        bytes[at + offset + 1] = 0;
    }
    fs::write(&path, bytes).unwrap();
    let out = f.dir.join("zip-out");
    fs::create_dir(&out).unwrap();
    let direct = extract_zip(&path, &out, &Limits::default()).unwrap_err();
    assert!(direct.downcast_ref::<UnsupportedZipCompression>().is_some());
    assert!(fs::read_dir(&out).unwrap().next().is_none());
    let missing =
        extract_zip_or_tar(&path, &out, &f.dir.join("no-tar.exe"), &Limits::default()).unwrap_err();
    assert!(format!("{missing:#}").contains("Re-pack"), "{missing:#}");
    if tar_executable().is_file() {
        // The bytes are not real bzip2, so tar.exe must refuse them too;
        // what matters is that the archive reached tar.exe's checked path.
        let error =
            extract_zip_or_tar(&path, &out, &tar_executable(), &Limits::default()).unwrap_err();
        assert!(
            format!("{error:#}").contains("Could not read this archive"),
            "{error:#}"
        );
    }
    no_escape(&f);
}

#[test]
fn superscript_and_console_devices_are_rejected() {
    for name in [
        "COM¹.iff",
        "COM².iff",
        "COM³",
        "LPT¹",
        "LPT².txt",
        "LPT³",
        "CONIN$",
        "CONOUT$.iff",
    ] {
        assert!(safe_entry(name).is_none(), "Accepted Windows device {name}");
    }
    assert!(safe_entry("COM12.iff").is_some());
}
#[test]
fn owned_atomic_replace_never_removes_destination_before_replacement() {
    let f = Fixture::new();
    let target = f.mods.join("atomic.iff");
    fs::write(&target, b"old").unwrap();
    let stage = f.mods.join(".mod-install-atomic.tmp");
    let mut owned = create_owned(&stage).unwrap();
    owned.write_all(b"new").unwrap();
    owned.sync_all().unwrap();
    assert!(
        fs::remove_file(&stage).is_err(),
        "Retained stage can be removed by another handle"
    );
    rename_owned(&owned, &target, true).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(!stage.exists());
    crate::loader_files::unlink(&owned).unwrap();
    drop(owned);
    assert!(!target.exists());
}
#[test]
fn new_destination_has_no_replacement_authority() {
    let f = Fixture::new();
    let target = f.mods.join("atomic-foreign.iff");
    fs::write(&target, b"foreign").unwrap();
    let stage = f.mods.join(".mod-install-new.tmp");
    let owned = create_owned(&stage).unwrap();
    assert!(rename_owned(&owned, &target, false).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"foreign");
    crate::loader_files::unlink(&owned).unwrap();
}

#[test]
fn atomic_replace_retains_original_read_handle_and_write_exclusion() {
    let f = Fixture::new();
    let target = f.mods.join("retained-original.iff");
    fs::write(&target, b"original").unwrap();
    let original = fs::OpenOptions::new()
        .read(true)
        .share_mode(5)
        .open(&target)
        .unwrap();
    let stage = f.mods.join(".mod-install-retained.tmp");
    let mut file = create_owned(&stage).unwrap();
    file.write_all(b"replacement").unwrap();
    file.sync_all().unwrap();
    assert!(fs::OpenOptions::new().write(true).open(&target).is_err());
    rename_owned(&file, &target, true).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"replacement");
    let mut bytes = Vec::new();
    (&original).read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
    crate::loader_files::unlink(&file).unwrap();
}
