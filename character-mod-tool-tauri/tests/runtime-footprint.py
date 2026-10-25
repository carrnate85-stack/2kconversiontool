import argparse
import ast
import io
import json
import struct
import zipfile
from collections import defaultdict
from pathlib import Path

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser()
parser.add_argument("--exe", type=Path)
parser.add_argument("--report", default="runtime-footprint-report.json")
args = parser.parse_args()
metadata = json.loads((root / "dist/BUILD.json").read_text(encoding="utf-8-sig"))
exe = args.exe or root / f"dist/Character-Mod-Tool-Tauri-Preview-{metadata['version']}.exe"
binary = exe.read_bytes()
assert binary[-64:-48] == b"CMT-RUST-PAYLOAD"
offset, length = struct.unpack("<QQ", binary[-48:-32])
assert offset + length == len(binary) - 64
groups = defaultdict(lambda: {"files": 0, "expandedBytes": 0, "compressedBytes": 0})
references = []
candidates = {"pip", "PyInstaller", "_pyinstaller_hooks_contrib", "setuptools", "_distutils_hack",
              "altgraph", "pefile", "ordlookup", "packaging", "win32ctypes", "pkg_resources", "capstone"}
with zipfile.ZipFile(io.BytesIO(binary[offset:offset + length])) as archive:
    for item in archive.infolist():
        prefix = "Backend/python_runtime/Lib/site-packages/"
        if item.filename.startswith(prefix):
            group = "site-packages/" + item.filename[len(prefix):].split("/")[0]
        elif item.filename.startswith("Backend/python_runtime/"):
            group = "python_runtime/" + item.filename.split("/")[2]
        else:
            group = item.filename.split("/")[1]
        row = groups[group]
        row["files"] += 1
        row["expandedBytes"] += item.file_size
        row["compressedBytes"] += item.compress_size
        if not item.filename.endswith(".py") or item.filename.startswith(("Backend/python_runtime/", "Backend/blender_tools/")):
            continue
        try:
            tree = ast.parse(archive.read(item).decode("utf-8-sig"), filename=item.filename)
        except (SyntaxError, UnicodeError):
            continue
        for node in ast.walk(tree):
            imports = [entry.name for entry in node.names] if isinstance(node, ast.Import) else (
                [node.module or ""] if isinstance(node, ast.ImportFrom) else [])
            for name in imports:
                if name.split(".")[0] in candidates:
                    references.append({"file": item.filename, "line": node.lineno, "module": name})
report = {
    "exe": str(exe), "exeBytes": len(binary), "payloadBytes": length,
    "expandedBytes": sum(row["expandedBytes"] for row in groups.values()),
    "files": sum(row["files"] for row in groups.values()),
    "groups": dict(sorted(groups.items(), key=lambda row: -row[1]["expandedBytes"])),
    "directBuildPackageImports": references,
}
(root / "artifacts" / args.report).write_text(json.dumps(report, indent=2), encoding="utf-8")
print(json.dumps({**{key: report[key] for key in ("exeBytes", "payloadBytes", "expandedBytes", "files", "directBuildPackageImports")},
                  "largestGroups": list(report["groups"].items())[:22]}, indent=2))
