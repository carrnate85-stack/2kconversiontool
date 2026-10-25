"""Real snapshot -> reference decode -> Blender -> Python atomic publication.

Run with the packaged Backend/python_runtime/python.exe and -B. No app, GUI,
CDP, Rust host lease, or mocked Blender child is involved. --check-only never
launches Blender. Only the two small named report/log files survive cleanup.
"""
import argparse
import ast
import base64
import csv
import hashlib
import importlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import traceback
import types
from unittest.mock import patch
import zipfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
REPORT_NAME = "managed-blender-roundtrip-report.json"
LOG_NAME = "managed-blender-roundtrip.log"


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def preflight(artifacts):
    setup = read_json(artifacts / "blender-test-setup.json")
    engine = Path(setup["settings"]["head_swap_tool"]).parent.parent.resolve()
    for path in (Path(setup["target"]), Path(setup["settings"]["blender_exe"]),
                 engine / "python_runtime/python.exe", engine / "python_bridge.py",
                 engine / "electron/blender_open_output.py",
                 artifacts / "blender-test-bundle.json", artifacts / "blender-test-jersey.iff",
                 ROOT / "tests/blender-inspect.py"):
        require(path.is_file(), f"Missing prerequisite: {path}")
    for name in ("prototype_services", "prototype_blender_snapshot", "prototype_player_writes"):
        ast.parse((ROOT / "engine" / f"{name}.py").read_text(encoding="utf-8-sig"))
    tree = ast.parse((ROOT / "engine/prototype_services.py").read_text(encoding="utf-8-sig"))
    opening = next(node for node in ast.walk(tree)
                   if isinstance(node, ast.FunctionDef) and node.name == "blender_open")
    names = {node.id for node in ast.walk(opening) if isinstance(node, ast.Name)}
    require({"launch_inputs", "copy_input"} <= names,
            "Source snapshot launch_inputs/copy_input wiring has not landed; do not launch Blender.")
    require(shutil.which("node"), "Node.js is required for the isolated reference decode.")
    return setup, engine


# Use the existing original parser and current tangent projection, without a UI
# worker or native transport. Every archive read must be a sealed snapshot file.
DECODE = r"""
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
(async () => {
  const [root, manifestPath, output] = process.argv.slice(1);
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const allowed = new Set(manifest.files.map(x => path.resolve(x.path).toLowerCase()));
  const reads = new Set();
  const read = fsp.readFile;
  fsp.readFile = async function(file, ...args) {
    const key = path.resolve(String(file)).toLowerCase();
    if (!allowed.has(key)) throw new Error('Decoder escaped captured inputs: ' + file);
    reads.add(key);
    return read.call(this, file, ...args);
  };
  const parser = path.resolve(root, '../character-mod-tool-webview2/Reference/src/core/preview.js');
  const { createIffPreview } = require(parser);
  const { selectTangentMeshes } = await import(pathToFileURL(path.join(root, 'worker/staged-projection.mjs')));
  const { projectTangentSpaceMeshes } = await import(pathToFileURL(path.join(root, 'worker/tangent-frame.js')));
  const resources = manifest.resources;
  const preview = await createIffPreview(resources.filePath, {
    ...resources.options, maxTriangles: 1000000, maxModels: 1000, skipTextures: false,
  });
  projectTangentSpaceMeshes({ meshes: selectTangentMeshes(preview.meshes, 'all') }, preview);
  const ordinary = new Set(resources.options.companionPaths.filter(x => x.kind === 'geometry').map(x => x.name.toLowerCase()));
  const kind = mesh => /facialhair|facial_hair|beard|goatee|scruff/i.test(`${mesh.sourceName} ${mesh.name}`)
    ? 'facialhair' : /(?:^|[_\s])hair(?:[_\s.]|$)/i.test(`${mesh.sourceName} ${mesh.name}`) ? 'hair' : '';
  const meshes = preview.meshes.filter(mesh => mesh.sourceKind === 'geometry' && kind(mesh)
    && (!String(mesh.sourceName).includes('_tangentspace.iff')
      || !ordinary.has(String(mesh.sourceName).toLowerCase().replace('_tangentspace.iff', '.iff'))))
    .map(mesh => ({ name: mesh.name, kind: kind(mesh), sourceName: mesh.sourceName,
      positions: Array.from(mesh.positions), uvs: Array.from(mesh.uvs || []),
      indices: Array.from(mesh.indices), groups: mesh.groups || [] }));
  const textures = preview.textures.filter(x => x.bytes).map(x => ({ name: x.name, bytes: x.bytes,
    binding: x.binding || null, assignments: x.assignments || [],
    inputColorSpace: x.inputColorSpace || 'none', range: x.range || null }));
  const bundle = { meshes, textures, eye: { irisColor: resources.player.irisColor, irisSize: resources.player.irisSize } };
  fs.writeFileSync(output, JSON.stringify({ bundle, reads: [...reads], parser, skipped: preview.skipped }));
})().catch(error => { console.error(error); process.exitCode = 1; });
"""


def exercise(artifacts, fixture, timeout, report):
    setup, engine = preflight(artifacts)
    source = Path(setup["target"]).resolve()
    original_files = sorted(source.parent.glob("*.iff"))
    preserved = {str(path): digest(path) for path in original_files}
    preserved.update({str(artifacts / name): digest(artifacts / name) for name in (
        "blender-test-setup.json", "blender-test-bundle.json", "blender-test-jersey.iff")})
    report["preservedSourceCount"] = len(preserved)
    data, live, game = [fixture / name for name in ("data", "live", "game")]
    session = data / "session"
    for folder in (data, session, live, game):
        folder.mkdir()
    for path in original_files:
        shutil.copyfile(path, live / path.name)
    target = live / source.name
    other = live / "managed-roundtrip-other-player.iff"
    shutil.copyfile(source, other)
    # A tiny real manifest archive exercises the real jersey extraction path.
    shutil.copyfile(artifacts / "blender-test-jersey.iff", game / "0A")
    (game / "mod.exe").write_bytes(b"test discovery marker; never executed")
    with (game / "manifest").open("w", newline="", encoding="utf-8") as stream:
        csv.writer(stream).writerow(["char/jersey_nike_v.iff", "0A", 0, (game / "0A").stat().st_size])
    settings = {**setup["settings"], "game_root": str(game), "game_root_2k27": str(game),
                "output_dir": str(data / "outputs")}
    (data / "settings.json").write_text(json.dumps(settings), encoding="utf-8")
    os.environ.update(CMT_SESSION=str(session), CMT_RUST_DATA=str(data),
                      CHARACTER_MOD_TOOL_ROOT=str(engine), LOCALAPPDATA=str(data),
                      APPDATA=str(data), PYTHONDONTWRITEBYTECODE="1",
                      CMT_BLENDER_TEST_REPORT=str(fixture / "inspection.json"))
    sys.path[:0] = [str(ROOT / "engine"), str(engine)]
    services = importlib.import_module("prototype_services")
    snapshots = importlib.import_module("prototype_blender_snapshot")
    publication = importlib.import_module("prototype_player_writes")
    app_settings = importlib.import_module("app_settings")
    app_settings._USER_DATA_DIR = data
    app_settings.SETTINGS_PATH = data / "settings.json"
    app_settings.LOG_DIR = data / "logs"
    app_settings.DIAGNOSTICS_DIR = data / "diagnostics"
    original_bridge = importlib.import_module("python_bridge")
    for module in (services, snapshots, publication):
        require(Path(module.__file__).resolve().parent == ROOT / "engine", "Packaged code shadowed source")
    report["sourceHashes"] = {Path(module.__file__).name: digest(module.__file__)
                              for module in (services, snapshots, publication)}
    report["runtime"] = str(sys.executable)
    report["packagedEngine"] = str(engine)
    events = []
    bridge = types.SimpleNamespace(
        fast_settings=lambda: read_json(data / "settings.json"),
        game_root_for=original_bridge.game_root_for,
        command_run_swap=original_bridge.command_run_swap,
        command_health=original_bridge.command_health,
        command_health_repair=original_bridge.command_health_repair,
        emit_event=lambda kind, message, **extra: events.append({"type": kind, "message": message, **extra}))
    commands = services.commands(bridge, engine, data)
    commands.update(snapshots.commands(bridge, engine, data))
    snapshot = session / "blender-snapshot-managed-roundtrip"
    snapshot.mkdir()
    payload = {"target": str(target), "_blenderSnapshot": str(snapshot),
               "includeJersey": True, "includeHair": True, "includeFacialHair": True,
               "includeTextures": True, "managedReturn": False}
    resources = commands["blender-snapshot-prepare"](payload)
    require(commands["blender-snapshot-validate"](payload) is True, "Snapshot validation failed")
    manifest = snapshots.load(payload, session)
    sealed = {str(Path(row["path"]).resolve()): row["digest"] for row in manifest["files"]}
    for path, expected in sealed.items():
        require(Path(path).is_relative_to(snapshot), f"Unowned captured dependency: {path}")
        require(digest(path) == expected, "Captured bytes differ")
    companions = resources["options"]["companionPaths"]
    require(any(row["kind"] == "geometry" for row in companions), "No captured geometry")
    jersey = next(row for row in companions if row["kind"] == "preview-jersey" and "jersey" in row["name"])
    require(digest(jersey["path"]) == digest(artifacts / "blender-test-jersey.iff"), "Wrong captured jersey")
    decoded_path = fixture / "decoded.json"
    subprocess.run([shutil.which("node"), "-e", DECODE, str(ROOT), str(snapshot / "snapshot.json"),
                    str(decoded_path)], check=True, timeout=timeout, cwd=fixture)
    decoded = read_json(decoded_path)
    bundle = decoded["bundle"]
    baseline = read_json(artifacts / "blender-test-bundle.json")
    key = lambda mesh: (mesh["sourceName"], mesh["name"], mesh["kind"])
    require({key(mesh) for mesh in bundle["meshes"]} == {key(mesh) for mesh in baseline["meshes"]},
            "Fresh captured reference mesh identities differ from the setup bundle")
    require(bundle["textures"], "Captured decode produced no textures")
    mesh_digest = digest_bytes(json.dumps(bundle["meshes"], sort_keys=True).encode("utf-8"))
    texture_digests = [digest_bytes(base64.b64decode(row["bytes"], validate=True))
                       for row in bundle["textures"]]
    reads = set(decoded["reads"])
    for row in companions:
        if row["kind"] in ("config", "item", "geometry"):
            require(str(Path(row["path"]).resolve()).lower() in reads,
                    f"Decoder did not consume captured dependency: {row['name']}")
    require(str(Path(manifest["source"]).resolve()).lower() in reads, "Decoder missed captured main")
    payload["referenceBundle"] = bundle
    report["capturedFiles"] = len(sealed)
    report["capturedDecodeReads"] = len(reads)
    report["referenceMeshes"] = len(bundle["meshes"])
    report["referenceTextures"] = len(bundle["textures"])
    # Model the already-validated launch: originals are no longer usable as inputs.
    for path in live.glob("*.iff"):
        if path not in (target, other):
            path.write_bytes(b"retired private live dependency")
    (game / "0A").write_bytes(b"retired private game resource")
    (data / "settings.json").write_text(json.dumps({**settings, "blender_exe": "unusable-live-setting.exe"}), encoding="utf-8")
    unchanged = {str(path): digest(path) for path in live.glob("*.iff") if path != target}
    initial = digest(target)
    copies, launches, publications = [], [], []
    real_copy, real_launch = snapshots.copy_input, snapshots.launch_inputs
    real_popen, real_replace = subprocess.Popen, os.replace
    children = []
    deadline = time.monotonic() + timeout

    def launch_inputs(*args, **kwargs):
        value = real_launch(*args, **kwargs)
        launches.append(str(value["source"]))
        return value

    def copy_input(captured, src, dst):
        require(str(Path(src).resolve()) in sealed, "Service copied an uncaptured input")
        real_copy(captured, src, dst)
        require(digest(dst) == sealed[str(Path(src).resolve())], "Launch copy differs from capture")
        copies.append({"source": str(src), "destination": str(dst)})

    def replace(src, dst):
        if Path(dst).resolve() == target:
            require(Path(src).parent.resolve() == target.parent, "Publication must be same-directory")
            require(".cmt-publish-" in Path(src).name, "Service did not use atomic publication")
            services.valid_player(src)
            publications.append({"before": digest(target), "returned": digest(src)})
        return real_replace(src, dst)

    class BlenderChild:
        def __init__(self, args, **kwargs):
            require(not children, "Expected exactly one Blender child")
            require(Path(args[0]).resolve() == Path(settings["blender_exe"]).resolve(), "Live settings leaked")
            boundary = args.index("--")
            value = lambda flag: Path(args[args.index(flag) + 1])
            self.folder = value("--signal").parent
            require(digest(value("--iff")) == initial, "Wrong main input at launch")
            require(value("--iff") != target and value("--iff").is_relative_to(session), "Live main launched")
            require(digest(value("--jersey")) == sealed[str(Path(jersey["path"]).resolve())], "Wrong jersey launch")
            launch_bundle = read_json(value("--reference-bundle"))
            require(digest_bytes(json.dumps(launch_bundle["meshes"], sort_keys=True).encode("utf-8")) == mesh_digest,
                    "Captured reference geometry changed before launch")
            require(len(launch_bundle["textures"]) == len(texture_digests), "Launch texture count changed")
            for row, expected in zip(launch_bundle["textures"], texture_digests):
                path = Path(row["path"])
                require(path.is_relative_to(self.folder) and path.read_bytes()[:4] == b"DDS ", "Bad launch DDS path")
                require(digest(path) == expected, "Captured reference texture changed before launch")
            require(launches == [manifest["source"]], "Real launch_inputs was not used exactly once")
            require({row["source"] for row in copies} >= {manifest["source"], jersey["path"]}, "Real copy_input was bypassed")
            injected = [args[0], "--background", "--factory-startup", *args[1:boundary],
                        "--python-exit-code", "17", "--python", str(ROOT / "tests/blender-inspect.py"),
                        *args[boundary:], "--auto-send", "--test-nudge", "--test-edit-mode"]
            report["launchFlags"] = ["--background", "--factory-startup", "--auto-send", "--test-nudge", "--test-edit-mode"]
            kwargs.update(stdout=sys.stdout, stderr=sys.stderr, env=dict(os.environ),
                          creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            self.process = real_popen(injected, **kwargs)
            children.append(self)

        def poll(self):
            if time.monotonic() > deadline:
                self.process.kill()
                self.process.wait()
                raise TimeoutError("Real Blender exceeded the test deadline")
            result = self.process.poll()
            # A fast background process may exit between service signal polls.
            # One extra poll lets the service observe its real completed signal.
            if result is not None and not getattr(self, "exit_seen", False):
                self.exit_seen = True
                return None
            return result

        def wait(self):
            result = self.process.wait(timeout=max(1, deadline - time.monotonic()))
            require(result == 0, f"Blender exited with {result}; inspect the preserved log")
            returned = self.folder / target.name
            services.valid_player(returned)
            require(digest(returned) == digest(target) != initial, "Real return was not published or unchanged")
            require(digest(self.folder / "backup.iff") == initial, "Publication backup differs from original")
            marker = read_json(self.folder / ".cmt-return.json")
            require(marker["appliedDigest"] == digest(target), "Atomic return was not confirmed")
            return result

    try:
        with patch.object(snapshots, "launch_inputs", launch_inputs), patch.object(snapshots, "copy_input", copy_input), \
                patch.object(services.subprocess, "Popen", BlenderChild), patch.object(os, "replace", replace):
            result = commands["blender-open"](payload)
        require(len(children) == 1 and len(publications) == 1, "Missing real Blender/atomic publication")
        inspection = read_json(fixture / "inspection.json")
        require(inspection["signal"]["eyesRecalculated"] is True, "Eye recalculation missing")
        require(any(row["state"] == "complete" and row.get("eyesRecalculated") for row in events), "No completed service event")
        require(not any(row["state"] in ("return-ready", "error", "closed") for row in events), "Unexpected managed/error event")
        require(not children[0].folder.exists(), "Service did not clean confirmed handoff")
        services.valid_player(target)
        with zipfile.ZipFile(target) as archive, zipfile.ZipFile(source) as before:
            changed = [name for name in set(archive.namelist()) & set(before.namelist())
                       if digest_bytes(archive.read(name)) != digest_bytes(before.read(name))]
        require(any(name.lower().endswith((".gz", ".bin")) for name in changed), "No actual mesh buffer changed")
        require(not list(live.glob(".*.cmt-publish-*.tmp")), "Publication temp leaked")
        for path, expected in unchanged.items():
            require(digest(path) == expected, f"Other private companion changed: {path}")
        for path, expected in sealed.items():
            require(digest(path) == expected, f"Captured input was modified: {path}")
        report.update(result=result, inspection=inspection,
                      changedMembers=changed, launchCopies=copies, atomicPublications=publications,
                      eventStates=[row["state"] for row in events], targetBefore=initial, targetAfter=digest(target))
    finally:
        for child in children:
            if child.process.poll() is None:
                child.process.kill()
                child.process.wait(timeout=15)
        for path, expected in preserved.items():
            require(digest(path) == expected, f"Original source/fixture changed: {path}")
        report["originalSourcesPreserved"] = True


def digest_bytes(value):
    return hashlib.sha256(value).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--artifacts", type=Path, default=ROOT / "artifacts")
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--check-only", action="store_true")
    parser.add_argument("--worker", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    artifacts = args.artifacts.resolve()
    setup, engine = preflight(artifacts)
    require(30 <= args.timeout <= 600, "Timeout must be between 30 and 600 seconds")
    if args.check_only:
        print("Source snapshot wiring and real Blender prerequisites present; no launch performed.")
        return
    if args.worker:
        report = {"status": "failed", "scope": "Current Python services + real background Blender; managedReturn=false",
                  "limitations": ["No Rust host lease, UI, or CDP exercised", "Original JS fallback parser, not native geometry transport",
                                  "Private one-resource manifest; shared game textures are not exercised"]}
        try:
            exercise(artifacts, args.worker, args.timeout, report)
            report["status"] = "passed"
        except Exception:
            report["status"] = "failed"
            report["error"] = traceback.format_exc()
            traceback.print_exc()
        finally:
            (args.worker / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
        raise SystemExit(0 if report["status"] == "passed" else 1)
    started = time.monotonic()
    with tempfile.TemporaryDirectory(prefix="cmt-managed-blender-") as temporary:
        fixture = Path(temporary).resolve()
        with (fixture / "run.log").open("w", encoding="utf-8") as log:
            child = subprocess.Popen([str(engine / "python_runtime/python.exe"), "-B", str(Path(__file__).resolve()),
                                      "--artifacts", str(artifacts), "--timeout", str(args.timeout), "--worker", str(fixture)],
                                     cwd=fixture, stdout=log, stderr=subprocess.STDOUT,
                                     env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"},
                                     creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            try:
                code = child.wait(timeout=args.timeout * 2 + 60)
            except subprocess.TimeoutExpired:
                # Kill only this owned worker tree, including its Blender child.
                subprocess.run(["taskkill", "/PID", str(child.pid), "/T", "/F"], capture_output=True, timeout=20)
                child.wait(timeout=20)
                code = 1
        report = read_json(fixture / "report.json") if (fixture / "report.json").exists() else {
            "status": "failed", "error": "Worker timed out or exited without a report"}
        report["elapsedSeconds"] = round(time.monotonic() - started, 2)
        text = (fixture / "run.log").read_text(encoding="utf-8", errors="replace")
        require(len(text) < 2 * 1024 * 1024, "Log exceeded bounded artifact size")
        (artifacts / LOG_NAME).write_text(text, encoding="utf-8")
    report["fixtureCleaned"] = not fixture.exists()
    (artifacts / REPORT_NAME).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps({key: report.get(key) for key in (
        "status", "elapsedSeconds", "capturedFiles", "referenceMeshes", "referenceTextures", "fixtureCleaned", "error")}, indent=2))
    raise SystemExit(code if code else 0 if report["status"] == "passed" else 1)


if __name__ == "__main__":
    main()
