use serde_json::{Value, json};
use std::path::Path;

const VC_DLLS: &[&str] = &[
    "MSVCP140.dll",
    "VCOMP140.dll",
    "VCRUNTIME140.dll",
    "VCRUNTIME140_1.dll",
];

pub fn health(engine: &Path) -> Value {
    let windows = std::env::var_os("WINDIR").map(std::path::PathBuf::from);
    inspect(engine, windows.as_deref())
}

fn inspect(engine: &Path, windows: Option<&Path>) -> Value {
    let framework = windows.map(|root| root.join("Microsoft.NET/Framework64/v4.0.30319"));
    let clr_files = ["clr.dll", "mscorlib.dll", "System.IO.Compression.dll"];
    let clr_present = framework
        .as_ref()
        .is_some_and(|root| clr_files.iter().all(|name| root.join(name).is_file()));
    let mut missing = Vec::new();
    let mut located = Vec::new();
    for name in VC_DLLS {
        let local = engine.join("tools").join(name);
        let system = windows.map(|root| root.join("System32").join(name));
        if local.is_file() {
            located.push(local);
        } else if let Some(system) = system.filter(|path| path.is_file()) {
            located.push(system);
        } else {
            missing.push(*name);
        }
    }
    json!({"id":"sharing-prerequisites","label":"Sharing Prerequisites","checks":[
        {"id":"framework-files","label":"Compatibility decoder framework","status":if clr_present {"info"} else {"warning"},"statusLabel":if clr_present {Some("Info")} else {None},
         "message":if clr_present {"CLR 4 framework files found. Runtime activation and exact minimum product version are not established by this file check."} else {"CLR 4 framework files were not found. Compatibility decoders require the Windows .NET Framework; a .NET SDK is not required."},"path":framework},
        {"id":"native-helper-runtime-files","label":"Native texture helper runtime","status":if missing.is_empty() {"info"} else {"warning"},"statusLabel":if missing.is_empty() {Some("Info")} else {None},
         "message":if missing.is_empty() {"VC++ v14 x64 dependency files found. File presence alone does not prove loader compatibility.".to_owned()} else {format!("VC++ v14 x64 dependency files missing: {}. Install Microsoft's supported x64 Visual C++ Redistributable before texture conversion.",missing.join(", "))},"files":located},
        {"id":"clean-pc-verification","label":"Clean-PC validation","status":"info","statusLabel":"Unverified","message":"Not yet verified on a clean Windows installation. WebView2 and game assets remain external prerequisites. Blender 5.1 is included in the optional portable release ZIP."}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("cmt-prerequisites-{}", crate::runtime::stamp()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            assert!(
                self.0
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("cmt-prerequisites-")
            );
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn missing_files_warn_without_claiming_the_sdk_is_required() {
        let fixture = Fixture::new();
        let report = inspect(&fixture.0, Some(&fixture.0));
        assert_eq!(report["checks"][0]["status"], "warning");
        assert_eq!(report["checks"][1]["status"], "warning");
        assert!(
            report["checks"][0]["message"]
                .as_str()
                .unwrap()
                .contains("SDK is not required")
        );
    }
    #[test]
    fn app_local_vc_dependencies_are_detected_without_system_files() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("tools")).unwrap();
        for name in VC_DLLS {
            fs::write(
                fixture.0.join("tools").join(name),
                b"file-presence fixture, never loaded",
            )
            .unwrap();
        }
        let report = inspect(&fixture.0, None);
        assert_eq!(report["checks"][1]["status"], "info");
        assert!(
            report["checks"][1]["message"]
                .as_str()
                .unwrap()
                .contains("does not prove")
        );
        assert_eq!(report["checks"][1]["files"].as_array().unwrap().len(), 4);
    }
    #[test]
    fn system_presence_is_info_not_a_clean_pc_pass() {
        let fixture = Fixture::new();
        let framework = fixture.0.join("Microsoft.NET/Framework64/v4.0.30319");
        fs::create_dir_all(&framework).unwrap();
        fs::create_dir(fixture.0.join("System32")).unwrap();
        for name in ["clr.dll", "mscorlib.dll", "System.IO.Compression.dll"] {
            fs::write(framework.join(name), b"fixture").unwrap();
        }
        for name in VC_DLLS {
            fs::write(fixture.0.join("System32").join(name), b"fixture").unwrap();
        }
        let report = inspect(&fixture.0.join("engine"), Some(&fixture.0));
        assert!(
            report["checks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|row| row["status"] == "info")
        );
        assert!(
            report["checks"][2]["message"]
                .as_str()
                .unwrap()
                .contains("Not yet verified")
        );
    }
}
