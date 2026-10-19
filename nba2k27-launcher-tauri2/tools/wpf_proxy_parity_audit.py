"""Offline WPF version.dll -> Rust V2 proxy parity audit. Read-only.

Checks (no game process, no build):
  1. Shipped WPF session-loader version.dll == nba2k27-version-proxy dist-scoreboard-text build
     == the legacy DLL embedded by src/loader_files.rs.
  2. native/outfit-match-v2-proxy/version_proxy_v2.cpp differs from WPF src/version_proxy.cpp only
     by the outfit V2 bridge hunks (so every other WPF capability is carried verbatim).
  3. freestyle_build_profile.hpp (compiled into BOTH proxies) has / lacks a Version 2 profile.
  4. The V2 native worker applies the MANAGER_INSTANCE -0x78 adjustment WPF relied on.

Usage: python -I tools/wpf_proxy_parity_audit.py [--root <2kconversiontool>] [--json]
Exit 0 when every check has an expected verdict (MISSING items are reported, not failed).
"""
from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import re
import sys
from pathlib import Path

V2_TIMESTAMP = 0x6AC53765
V2_IMAGE = 0x35BDE000
SHIPPED = Path("nba2k27-launcher-wpf/release/quality-pass-qa-20261010/Runtime/0.1.0-wpf.24/session-loader/version.dll")
SCOREBOARD_BUILD = Path("nba2k27-version-proxy/dist-scoreboard-text/version.dll")
WPF_PROXY = Path("nba2k27-version-proxy/src/version_proxy.cpp")
V2_PROXY = Path("nba2k27-launcher-tauri2/native/outfit-match-v2-proxy/version_proxy_v2.cpp")
FREESTYLE_PROFILE = Path("nba2k27-version-proxy/src/freestyle_build_profile.hpp")
LOADER_FILES = Path("nba2k27-launcher-tauri2/src/loader_files.rs")
V2_NATIVE = Path("nba2k27-launcher-tauri2/native/outfit_match_v2_native.hpp")

# Lines the V2 copy may add or change. Anything else is an unexpected divergence.
ALLOWED_V2_LINE = re.compile(
    r"^\s*$|outfit_match_v2|outfit_v2_(session|portable)=config_value|NBA2K27_SESSION_OUTFITS|The Version2 variant must never|^#endif$|"
    r"^\s*if\(!wcscmp\(leaf,kLogName\)\|\|!wcscmp\(leaf,kArmedName\)\|\|!wcscmp\(leaf,kTraceName\)\) \{$|"
    r"^\s*return !directory\.empty\(\)&&wcscpy_s\(output,capacity,\(directory\+leaf\)\.c_str\(\)\)==0;$|^\s*\}$"
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def unexpected_proxy_changes(wpf_text: str, v2_text: str) -> list[str]:
    """Return +/- diff lines that are NOT part of the known V2 bridge hunks."""
    bad = []
    for line in difflib.unified_diff(wpf_text.splitlines(), v2_text.splitlines(), lineterm="", n=0):
        if line.startswith(("+++", "---", "@@")):
            continue
        if line.startswith("-"):
            bad.append(line)  # V2 must not remove WPF logic
        elif line.startswith("+") and not ALLOWED_V2_LINE.search(line[1:]):
            bad.append(line)
    return bad


def freestyle_profiles(text: str) -> list[tuple[int, int]]:
    return [(int(t, 16), int(s, 16)) for t, s in
            re.findall(r"timestamp==0x([0-9A-Fa-f]{8})\s*&&\s*image_size==0x([0-9A-Fa-f]{8})", text)]


def embedded_legacy_dll(loader_rs: str) -> str | None:
    m = re.search(r'const PROXY: &\[u8\] = include_bytes!\("([^"]+)"\);', loader_rs)
    return m.group(1) if m else None


def has_manager_adjust(native_text: str) -> bool:
    return re.search(r"q\(table\)==base\+kSecondary\?table-0x78:table", native_text) is not None


def audit(root: Path) -> dict:
    out: dict = {"root": str(root), "checks": {}}
    c = out["checks"]
    shipped, built = root / SHIPPED, root / SCOREBOARD_BUILD
    loader_rs = (root / LOADER_FILES).read_text(encoding="utf-8")
    rel = embedded_legacy_dll(loader_rs)
    embedded = (root / LOADER_FILES).parent / rel if rel else None
    hashes = {"shipped": sha256(shipped), "dist_scoreboard_text": sha256(built),
              "rust_embedded": sha256(embedded.resolve()) if embedded and embedded.exists() else None}
    c["legacy_dll_identity"] = {"verdict": "PASS" if len(set(hashes.values())) == 1 else "FAIL", **hashes}
    bad = unexpected_proxy_changes((root / WPF_PROXY).read_text(encoding="utf-8"),
                                   (root / V2_PROXY).read_text(encoding="utf-8"))
    c["v2_proxy_is_wpf_plus_bridge"] = {"verdict": "PASS" if not bad else "FAIL", "unexpected": bad[:40]}
    profiles = freestyle_profiles((root / FREESTYLE_PROFILE).read_text(encoding="utf-8"))
    c["freestyle_v2_profile"] = {
        "verdict": "PRESENT" if (V2_TIMESTAMP, V2_IMAGE) in profiles else "MISSING",
        "profiles": [f"{t:08X}:{s:08X}" for t, s in profiles]}
    c["outfit_manager_minus_0x78"] = {
        "verdict": "PASS" if has_manager_adjust((root / V2_NATIVE).read_text(encoding="utf-8")) else "FAIL"}
    out["ok"] = all(v["verdict"] in ("PASS", "PRESENT", "MISSING") for v in c.values())
    return out


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser()
    p.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    p.add_argument("--json", action="store_true")
    a = p.parse_args(argv)
    result = audit(a.root)
    if a.json:
        print(json.dumps(result, indent=2))
    else:
        for name, v in result["checks"].items():
            print(f"{v['verdict']:8} {name}")
            for k, val in v.items():
                if k != "verdict":
                    print(f"         {k}: {val}")
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
