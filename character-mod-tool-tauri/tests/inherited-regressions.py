"""Exercise inherited fixtures against the packaged engine and native DDS builder."""
import importlib.util
import contextlib
import logging
import json
import os
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

project = Path(__file__).resolve().parents[1]
production = project.parent / "character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend"
fixtures = project.parent / "character-mod-tool-webview2/tests"
version = tomllib.loads((project / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
executable = Path(os.environ.get("CMT_TEST_DDS_EXE", project / f"dist/Character-Mod-Tool-Tauri-Preview-{version}.exe"))
sys.path.insert(0, str(production))
sys.path.insert(0, str(project / "engine"))
os.environ["CMT_RUST_EXE"] = str(executable)
os.environ["CMT_25_REFERENCE"] = str(production)
os.environ["CMT_25_TOOL_ROOT"] = str(production)
os.environ["CMT_CONFIG_BACKEND"] = str(production)
os.environ["CMT_CONFIG_TOOL_ROOT"] = str(production)
# The inherited decorator checks this flag before importing its DDS function.
# That function is replaced below; no Node process is launched.
os.environ["CHARACTER_MOD_NODE"] = "native-dds-fixture-gate"
import prototype_services
import nba2k25_support

nba2k25_support.build_dds = prototype_services.build_rust_dds
suite = unittest.TestSuite()
with tempfile.TemporaryDirectory(prefix="CmtInherited-") as temporary, contextlib.ExitStack() as cleanup:
    cleanup.callback(logging.shutdown)
    os.environ["LOCALAPPDATA"] = temporary
    os.environ["APPDATA"] = temporary
    import app_settings
    app_settings._USER_DATA_DIR = Path(temporary)
    app_settings.SETTINGS_PATH = Path(temporary) / "settings.json"
    app_settings.LOG_DIR = Path(temporary) / "logs"
    app_settings.DIAGNOSTICS_DIR = Path(temporary) / "diagnostics"
    for index, name in enumerate(("nba2k25-support.py", "nba2k25-facial-hair.py", "config-rename.py")):
        spec = importlib.util.spec_from_file_location(f"inherited_{index}", fixtures / name)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        suite.addTests(unittest.defaultTestLoader.loadTestsFromModule(module))
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    report = {"engine": "WPF 0.1.31 packaged backend", "host": version,
              "nativeDds": True, "nodeInvoked": False, "tests": result.testsRun,
              "skipped": len(result.skipped), "failures": len(result.failures), "errors": len(result.errors)}
    (project / "artifacts/inherited-regressions-report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps(report))
    raise SystemExit(0 if result.wasSuccessful() else 1)
