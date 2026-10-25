#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod accessory_catalog;
mod assets;
mod backend;
mod blender_recovery;
mod catalog_cache;
mod catalog_native;
mod host;
mod prerequisites;
mod preview_jobs;
mod preview_packet;
mod runtime;
mod worker_diagnostics;

use anyhow::{Context, Result};
use serde_json::{Value, json};

fn resolve_payload(value: &Value, results: &Value) -> Result<Value> {
    match value {
        Value::Object(object) if object.len() == 1 && object.contains_key("$ref") => {
            let pointer = object["$ref"]
                .as_str()
                .context("Invalid script result reference")?;
            results
                .pointer(pointer)
                .cloned()
                .with_context(|| format!("Missing script result {pointer}"))
        }
        Value::Object(object) => object
            .iter()
            .map(|(key, value)| Ok((key.clone(), resolve_payload(value, results)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(Value::Object),
        Value::Array(array) => array
            .iter()
            .map(|value| resolve_payload(value, results))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        _ => Ok(value.clone()),
    }
}

fn start() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("--decode-scene") {
        use std::io::{Read, Write};
        let mut input = Vec::new();
        std::io::stdin()
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut input)?;
        anyhow::ensure!(input.len() <= 16 * 1024 * 1024, "Oversized fitting scene");
        std::io::stdout()
            .write_all(&nba2k_assets::codec::document(&input, &Default::default())?)?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--decode-geometry") {
        use std::io::{Read, Write};
        anyhow::ensure!(
            args.len() == 3,
            "Geometry decoding requires game root and expected size"
        );
        let expected: usize = args[2].parse().context("Invalid geometry size")?;
        anyhow::ensure!(
            expected > 0 && expected <= 128 * 1024 * 1024,
            "Unsupported geometry size"
        );
        let root = std::path::Path::new(&args[1]);
        anyhow::ensure!(
            root.join("manifest").is_file(),
            "Selected game manifest is missing"
        );
        let mut input = Vec::new();
        std::io::stdin()
            .take(128 * 1024 * 1024 + 1)
            .read_to_end(&mut input)?;
        anyhow::ensure!(
            input.len() <= 128 * 1024 * 1024,
            "Oversized packed geometry"
        );
        let decoded = if nba2k_assets::codec::vcz(&input) {
            nba2k_assets::codec::Oodle::load(root)?.buffer(&input, expected)?
        } else if input.starts_with(b"\x1f\x8b\x08") {
            let mut raw = Vec::new();
            flate2::read::GzDecoder::new(input.as_slice())
                .take(expected as u64 + 1)
                .read_to_end(&mut raw)?;
            raw
        } else {
            input
        };
        anyhow::ensure!(
            decoded.len() == expected,
            "Decoded geometry size does not match its descriptor"
        );
        std::io::stdout().write_all(&decoded)?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--build-dds") {
        use std::io::{Read, Write};
        let mut input = Vec::new();
        std::io::stdin()
            .take(128 * 1024 * 1024 + 33)
            .read_to_end(&mut input)?;
        anyhow::ensure!(
            input.len() > 32 && input.len() <= 128 * 1024 * 1024 + 32,
            "Invalid DDS builder input size"
        );
        anyhow::ensure!(input.starts_with(b"TLD"), "Invalid DDS builder TLD header");
        let size = (input.len() - 32) as u32;
        anyhow::ensure!(
            nba2k_assets::codec::u32_at(&input, 24)? == size,
            "Texture payload size differs from its header"
        );
        input[28..32].copy_from_slice(&size.to_le_bytes());
        std::io::stdout().write_all(&nba2k_assets::codec::dds(&input)?)?;
        return Ok(());
    }
    let backend = backend::Backend::new()?;
    if args.first().map(String::as_str) == Some("--serve-rpc") {
        use std::io::{BufRead, Write};
        let output = std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout()));
        let mut tasks = Vec::new();
        for line in std::io::stdin().lock().lines() {
            let line = line?;
            anyhow::ensure!(line.len() < 64 * 1024 * 1024, "Oversized test RPC");
            let request: Value = serde_json::from_str(&line)?;
            let backend = backend.clone();
            let output = output.clone();
            tasks.push(std::thread::spawn(move || {
                let id = request["id"].clone();
                let command = request["command"].as_str().unwrap_or_default();
                let result = if command == "runtime-info" {
                    Ok(json!({"host":"Rust/Tauri 2","smoke":false,"data":backend.data,"session":backend.session}))
                } else {
                    backend.invoke(command, request["payload"].clone(), |event| {
                        let mut writer = output.lock().unwrap();
                        let _ = writeln!(writer, "{}", json!({"kind":"event","value":event}));
                        let _ = writer.flush();
                    })
                };
                let row = match result {
                    Ok(value) => json!({"id":id,"value":value}),
                    Err(error) => json!({"id":id,"error":format!("{error:#}")}),
                };
                let mut writer = output.lock().unwrap();
                let _ = writeln!(writer, "{row}");
                let _ = writer.flush();
            }));
        }
        backend.cancel();
        for task in tasks {
            let _ = task.join();
        }
        backend.shutdown();
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--script") {
        let bytes = std::fs::read(args.get(1).context("--script needs a JSON file")?)?;
        anyhow::ensure!(bytes.len() <= 1024 * 1024, "Oversized script");
        let steps: Vec<Value> = serde_json::from_slice(&bytes)?;
        anyhow::ensure!(steps.len() <= 100, "Too many script steps");
        let mut results = json!({});
        for step in steps {
            let name = step["name"].as_str().context("Script step needs a name")?;
            anyhow::ensure!(results.get(name).is_none(), "Duplicate script result name");
            let command = step["command"]
                .as_str()
                .context("Script step needs a command")?;
            let payload = resolve_payload(&step["payload"], &results)?;
            eprintln!("CMT_RUST_STEP|{name}|{command}");
            results[name] = backend.invoke(command, payload, |event| {
                eprintln!("CMT_RUST_EVENT|{event}")
            })?;
        }
        backend.shutdown();
        println!("{}", json!({"ok":true,"results":results}));
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("--rpc") {
        let command = args.get(1).context("--rpc requires a command")?;
        let payload: Value = if let Some(path) = args.get(2) {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            json!({})
        };
        let result = backend.invoke(command, payload, |event| {
            eprintln!("CMT_RUST_EVENT|{event}")
        });
        backend.shutdown();
        match result {
            Ok(value) => println!("{}", json!({"ok":true,"value":value})),
            Err(error) => {
                println!("{}", json!({"ok":false,"error":error.to_string()}));
                return Err(error);
            }
        }
        return Ok(());
    }
    host::run(backend, &args)
}

fn main() {
    if let Err(error) = start() {
        eprintln!("{error:#}");
        if !std::env::args().any(|arg| {
            matches!(
                arg.as_str(),
                "--rpc"
                    | "--script"
                    | "--serve-rpc"
                    | "--build-dds"
                    | "--decode-geometry"
                    | "--decode-scene"
            )
        }) {
            unsafe {
                let message = windows::core::HSTRING::from(format!("{error:#}"));
                windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    &message,
                    windows::core::w!("Character Mod Tool Rust Prototype"),
                    windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_references_use_structured_results() {
        let results = json!({"source":{"mainPath":"C:\\player.iff"}});
        assert_eq!(
            resolve_payload(&json!({"filePath":{"$ref":"/source/mainPath"}}), &results).unwrap(),
            json!({"filePath":"C:\\player.iff"})
        );
        assert!(resolve_payload(&json!({"$ref":"/missing"}), &results).is_err());
    }
}
