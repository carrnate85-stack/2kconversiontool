use std::{env, fs, path::PathBuf};
fn section<'a>(s: &'a str, a: &str, b: &str) -> &'a str {
    let start = s.find(a).expect(a);
    let end = start + s[start..].find(b).expect(b);
    &s[start..end]
}
fn main() {
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../../src");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let load = |name: &str| {
        let p = root.join(name);
        println!("cargo:rerun-if-changed={}", p.display());
        fs::read_to_string(p).unwrap()
    };
    let native = load("blacktop_native.rs");
    fs::write(
        out.join("native_types.rs"),
        format!(
            "use anyhow::{{Result,ensure}};use crate::session_safety::{{self,Process,Identity}};\n{}\n#[derive(Clone,Debug,PartialEq,Eq)]\n{}\n{}",
            section(
                &native,
                "pub(crate) trait Memory {",
                "impl Memory for Reader"
            ),
            section(
                &native,
                "pub(crate) struct ContextState {",
                "pub(crate) fn context(m:"
            ), section(&native,"pub fn guard(rows:","pub fn idle()")
        ),
    )
    .unwrap();
    let arena = load("blacktop_arena.rs");
    fs::write(out.join("inspect_types.rs"),format!("use anyhow::Result;use crate::blacktop_native::Memory;\n#[derive(Clone,Copy,Debug)]\n{}",section(&arena,"pub(crate) struct Region {","#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]"))).unwrap();
    let rules = load("game_rules.rs");
    fs::write(
        out.join("rule_types.rs"),
        format!(
            "use crate::blacktop_native::ContextState;use serde_json::{{Value,json}};\n{}",
            section(&rules, "pub struct Field {", "pub fn capture(m:")
        ),
    )
    .unwrap();
    let ball = load("blacktop_ball.rs");
    let ball_policy = format!(
        "{}\n{}",
        section(&ball, "use crate::blacktop_arena::Inspect;", "#[cfg(test)]"),
        section(
            &ball,
            "#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]",
            "fn read(m:"
        )
    )
    .replace(
        "include_bytes!(\"../assets/choices.json.gz\")",
        &format!(
            "include_bytes!(r\"{}\")",
            root.join("../assets/choices.json.gz")
                .canonicalize()
                .unwrap()
                .display()
        ),
    );
    fs::write(out.join("ball_policy.rs"), ball_policy).unwrap();
    let loader = load("loader_files.rs");
    fs::write(out.join("pins.rs"),format!("use anyhow::{{Context,Result,ensure}};use std::{{fs::{{self,File}},mem::size_of,os::windows::{{fs::{{MetadataExt,OpenOptionsExt}},io::AsRawHandle}},path::{{Component,Path,PathBuf}}}};use windows::Win32::{{Foundation::HANDLE,Storage::FileSystem::*}};\n{}\n{}",section(&loader,"fn plain(","/// The proven DLL"),section(&loader,"pub(crate) fn unlink(file:","fn rename_buffer("))).unwrap();
}
