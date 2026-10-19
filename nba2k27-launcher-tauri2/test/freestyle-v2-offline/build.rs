//! Compile the production filesystem primitives verbatim into this small
//! offline harness. Anchors fail closed when production code moves; there are
//! no hand-maintained substitutes for the functions under test.
use std::{env, fs, path::PathBuf};

fn section<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
    let start = source.find(from).expect("Production fixture start anchor");
    let end = start
        + source[start..]
            .find(to)
            .expect("Production fixture end anchor");
    &source[start..end]
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../src");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let load = |name: &str| {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        fs::read_to_string(path).unwrap()
    };
    let loader = load("loader_files.rs");
    let imports = r#"
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{fs::{self, File}, mem::size_of,
    os::windows::{ffi::OsStrExt, fs::{MetadataExt, OpenOptionsExt}, io::AsRawHandle},
    path::{Component, Path, PathBuf}};
use windows::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
"#;
    let code = format!(
        "{imports}\n{}\n{}\n{}",
        section(&loader, "pub fn digest(", "fn hex("),
        section(&loader, "fn plain(", "/// The proven DLL"),
        section(&loader, "pub(crate) fn unlink(file:", "impl Store {")
    );
    fs::write(out.join("actual_loader_primitives.rs"), code).unwrap();
    let profile = load("mod_profile.rs");
    let code = format!(
        r#"
use anyhow::{{Result, ensure}};
use std::{{fs::{{self, File}}, os::windows::{{fs::{{MetadataExt, OpenOptionsExt}},io::AsRawHandle}}, path::Path}};
use windows::Win32::{{Foundation::HANDLE, Storage::FileSystem::*, System::IO::DeviceIoControl}};
const DELETE_ACCESS:u32 = 0x10000;
{}
{}
"#,
        section(&profile, "fn open_dir(", "fn missing("),
        section(
            &profile,
            "fn set_junction(",
            "#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]"
        )
    );
    fs::write(out.join("actual_profile_junction.rs"), code).unwrap();
    let backend = load("backend.rs");
    let code = format!(
        r#"
use anyhow::{{Context, Result}};
use serde::Serialize;
use std::{{fs, os::windows::ffi::OsStrExt, path::Path, time::{{SystemTime,UNIX_EPOCH}}}};
use windows::{{Win32::Storage::FileSystem::{{MOVEFILE_REPLACE_EXISTING,MOVEFILE_WRITE_THROUGH,MoveFileExW}},core::PCWSTR}};
{}
{}
"#,
        section(&backend, "static NONCE:", "pub const VERSION:"),
        section(&backend, "fn wide(", "fn read_json<")
    );
    fs::write(out.join("actual_backend_atomic.rs"), code).unwrap();
    let session = load("native_session.rs");
    fs::write(
        out.join("actual_same_path.rs"),
        format!(
            "use std::path::Path;\n{}",
            section(&session, "pub(crate) fn same_path(", "fn quiet(")
        ),
    )
    .unwrap();
}
