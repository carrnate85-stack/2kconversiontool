"""Headless staging regressions; all writes stay in isolated temporary folders."""
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT.parent / "character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend"
sys.path[:0] = [str(ROOT / "engine"), str(BACKEND)]
import prototype_staging as staging


def binary_document(document):
    records = []
    def visit(name, value):
        records.append((name, value))
        if isinstance(value, dict):
            for key, child in value.items():
                visit(key, child)
        elif isinstance(value, list):
            for child in value:
                visit("", child)
    name, value = next(iter(document.items()))
    visit(name, value)
    data = bytearray(24 + len(records) * 40)
    strings = bytearray()
    def pointer(at, value):
        position = len(data) + len(strings)
        strings.extend(value.encode() + b"\0")
        struct.pack_into("<q", data, at, position - at + 1)
    for index, (name, value) in enumerate(records):
        offset = 24 + index * 40
        pointer(16 if index == 0 else offset - 8, name)
        if isinstance(value, dict) or isinstance(value, list):
            tag = 1 if isinstance(value, dict) else 2
            struct.pack_into("<I", data, offset + 12, len(value))
        elif isinstance(value, str):
            tag = 3
            pointer(offset + 16, value)
        elif isinstance(value, int):
            tag = 6
            struct.pack_into("<Q", data, offset + 16, value)
        elif isinstance(value, float):
            tag = 5
            struct.pack_into("<d", data, offset + 16, value)
        else:
            tag = 0
        struct.pack_into("<I", data, offset + 4, 0x40000000 + tag)
    struct.pack_into("<IIII", data, 0, 0xe01ff891, 1, len(strings), len(records))
    return bytes(data + strings)


def player(path, legacy=False, binary=False, references=()):
    materials = ["face_shader", "torso_shader"] + (["eyes_shader"] if legacy else ["l_eye_shader", "r_eye_shader"])
    scene = {"hihead": {"Model": {"hihead": {
        "Prim": [{"Material": name, "Start": 0, "Count": 3} for name in materials],
        "VertexFormat": {"POSITION0": {"Format": "R32G32B32_FLOAT", "Stream": 0}},
        "VertexStream": [{"Binary": "vertices.bin", "Stride": 12, "Size": 36}],
        "IndexBuffer": {"Binary": "indices.bin", "Format": "R16_UINT", "Size": 6},
    }}}}
    appearance = {"appearance": {"gender": "male", "skin color": 3},
                  "body_fit": {"chestTHIN": 0.25},
                  "accessory_items": {"configurations": [{"name": "home"}, {"name": "away"}]},
                  "references": list(references)}
    with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.comment = b"keep original comment"
        archive.writestr("hihead.SCNE", binary_document(scene) if binary else json.dumps(scene))
        archive.writestr("appearance_info.json", json.dumps(appearance))
        archive.writestr("vertices.bin", struct.pack("<9f", 0, 0, 0, 1, 0, 0, 0, 1, 0))
        archive.writestr("indices.bin", struct.pack("<3H", 0, 1, 2))
        if binary:
            archive.writestr("face.TXTR", binary_document({"face": {"Binary": "face.0123456789abcdef.tld", "Width": 1}}))
        archive.writestr("untouched.bin", b"morph/rig/UV bytes")


def companion(path, refs=()):
    with zipfile.ZipFile(path, "x") as archive:
        archive.writestr("texture.TXTR", json.dumps({"texture": {"references": list(refs)}}))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class PlayerStagingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cmt-player-staging-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.source = self.root / "source"
        self.source.mkdir()
        self.session = self.root / "session"
        self.session.mkdir()
        self.output = self.root / "output"
        self.bridge = types.SimpleNamespace(fast_settings=lambda: {"output_dir": str(self.output)},
                                           game_root_for=lambda *_: "unused-game-root")
        self.main = self.source / "png0001.iff"
        player(self.main)
        self.before = digest(self.main)

    def run_stage(self, source=None, **kwargs):
        return staging.stage_player({"filePath": str(source or self.main), **kwargs}, self.bridge, self.session)

    def assert_clean_failure(self):
        self.assertEqual(digest(self.main), self.before)
        self.assertFalse(list(self.output.glob("Edited Players/*")))
        self.assertFalse(list(self.session.iterdir()))

    def test_plain_copy_exact_unique_durable_and_original_unchanged(self):
        first, second = self.run_stage(), self.run_stage()
        self.assertNotEqual(first["directory"], second["directory"])
        self.assertEqual(digest(Path(first["mainPath"])), self.before)
        self.assertEqual(digest(Path(second["mainPath"])), self.before)
        self.assertEqual(digest(self.main), self.before)
        self.assertEqual(first["sourcePath"], str(self.main))
        self.assertEqual(first["gameLabel"], "NBA 2K27")
        self.assertFalse(Path(first["directory"]).is_relative_to(self.session))
        self.assertFalse(list(self.session.iterdir()))

    def test_all_configs_companions_and_body_dependencies_no_unrelated_files(self):
        selected = ["png0001_config_home.iff", "png0001_config_away.iff", "png0001_geo_hair_01.iff",
                    "png0001_item_hair_01.iff", "face0001.iff", "male_skin.iff", "chest_color_03.iff",
                    "torso_color_03.iff", "legs_color_03.iff"]
        for name in selected + ["png0002_config_home.iff", "png0001_backup.iff", "female_skin.iff", "chest_color_04.iff"]:
            companion(self.source / name)
        (self.source / "keep.txt").write_text("unrelated")
        result = self.run_stage()
        self.assertEqual(set(p.name for p in Path(result["directory"]).iterdir()), {"png0001.iff", *selected})
        self.assertEqual(result["companions"], len(selected))
        for name in selected:
            self.assertEqual(digest(self.source / name), digest(Path(result["directory"]) / name))

    def test_referenced_adjacent_dependency_and_recursive_config_reference(self):
        companion(self.source / "png0001_config_away.iff", ["char/shared_detail.iff"])
        companion(self.source / "shared_detail.iff", ["additional_skin.iff"])
        companion(self.source / "additional_skin.iff")
        result = self.run_stage()
        self.assertTrue((Path(result["directory"]) / "shared_detail.iff").is_file())
        self.assertTrue((Path(result["directory"]) / "additional_skin.iff").is_file())

    def test_zip_includes_referenced_and_appearance_body_dependencies(self):
        companion(self.source / "png0001_config_home.iff", ["shared_detail.iff"])
        companion(self.source / "shared_detail.iff")
        companion(self.source / "male_skin.iff")
        companion(self.source / "chest_color_04.iff")
        package = self.root / "package.zip"
        with zipfile.ZipFile(package, "x") as archive:
            for source in self.source.iterdir():
                archive.write(source, "nested/" + source.name)
            archive.writestr("readme.txt", "not a player")
        before = digest(package)
        result = self.run_stage(package)
        self.assertEqual(digest(package), before)
        self.assertEqual(digest(Path(result["mainPath"])), self.before)
        names = {p.name for p in Path(result["directory"]).iterdir()}
        self.assertIn("shared_detail.iff", names)
        self.assertIn("male_skin.iff", names)
        self.assertNotIn("chest_color_04.iff", names)
        self.assertFalse(list(self.session.iterdir()))

    def test_explicit_legacy_hint_rejects_without_outputs(self):
        with self.assertRaisesRegex(ValueError, "Convert older"):
            self.run_stage(gameLabel="NBA 2K26")
        self.assert_clean_failure()

    def test_unified_eye_legacy_cannot_be_relabelled_by_27_hint(self):
        legacy = self.source / "png0002.iff"
        player(legacy, legacy=True)
        before = digest(legacy)
        with self.assertRaisesRegex(ValueError, "older unified-eye"):
            self.run_stage(legacy, gameLabel="NBA2K27")
        self.assertEqual(digest(legacy), before)
        self.assert_clean_failure()

    def test_unknown_character_archive_is_not_accepted(self):
        fake = self.source / "fake.iff"
        with zipfile.ZipFile(fake, "x") as archive:
            archive.writestr("random.scne", "{}")
        with self.assertRaisesRegex(ValueError, "full NBA 2K27"):
            self.run_stage(fake, gameLabel="NBA 2K27")
        self.assert_clean_failure()

    def test_unsafe_reference_rejects_and_cleans_owned_copy(self):
        companion(self.source / "png0001_config_home.iff", ["../secret.iff"])
        with self.assertRaisesRegex(ValueError, "Unsafe character package path"):
            self.run_stage()
        self.assert_clean_failure()

    def test_symlink_dependency_rejected_without_touching_target(self):
        external = self.root / "external.iff"
        companion(external)
        before = digest(external)
        try:
            (self.source / "png0001_config_home.iff").symlink_to(external)
        except OSError:
            self.skipTest("Symlink creation not permitted on this host")
        with self.assertRaisesRegex(ValueError, "links"):
            self.run_stage()
        self.assertEqual(digest(external), before)
        self.assert_clean_failure()

    def test_package_referenced_symlink_is_rejected(self):
        companion(self.source / "png0001_config_home.iff", ["shared_detail.iff"])
        package = self.root / "links.zip"
        with zipfile.ZipFile(package, "x") as archive:
            archive.write(self.main, self.main.name)
            archive.write(self.source / "png0001_config_home.iff", "png0001_config_home.iff")
            link = zipfile.ZipInfo("shared_detail.iff")
            link.create_system = 3
            link.external_attr = (stat.S_IFLNK | 0o777) << 16
            archive.writestr(link, "../outside.iff")
        with self.assertRaisesRegex(ValueError, "regular files"):
            self.run_stage(package)
        self.assert_clean_failure()

    def test_source_reparse_attribute_is_rejected(self):
        original = Path.lstat
        def metadata(path):
            value = original(path)
            if path == self.main:
                return types.SimpleNamespace(st_mode=value.st_mode, st_file_attributes=0x400)
            return value
        with patch.object(Path, "lstat", metadata):
            with self.assertRaisesRegex(ValueError, "reparse"):
                self.run_stage()
        self.assert_clean_failure()

    def test_expanded_output_budget_is_enforced(self):
        original = staging._inspect_copy
        def expand(path, bridge):
            result = original(path, bridge)
            with path.open("ab") as file:
                file.write(b"x" * 4096)
            return result
        with patch.object(staging, "_inspect_copy", side_effect=expand), patch.object(staging, "MAX_TOTAL", self.main.stat().st_size + 100):
            with self.assertRaisesRegex(ValueError, "Expanded editing copies"):
                self.run_stage()
        self.assert_clean_failure()

    def test_package_referenced_traversal_is_rejected(self):
        package = self.root / "paths.zip"
        with zipfile.ZipFile(package, "x") as archive:
            archive.write(self.main, self.main.name)
            archive.writestr("../male_skin.iff", b"unexpected")
        with self.assertRaisesRegex(ValueError, "Unsafe character package path"):
            self.run_stage(package)
        self.assert_clean_failure()

    def test_copy_failure_preserves_original_and_existing_output(self):
        existing = self.output / "Edited Players/keep"
        existing.mkdir(parents=True)
        (existing / "keep.txt").write_text("keep")
        with patch.object(staging, "_inspect_copy", side_effect=ValueError("controlled failure")):
            with self.assertRaisesRegex(ValueError, "controlled failure"):
                self.run_stage()
        self.assertEqual(digest(self.main), self.before)
        self.assertEqual(list(existing.parent.iterdir()), [existing])
        self.assertEqual((existing / "keep.txt").read_text(), "keep")

    def test_source_conflict_refuses_publication(self):
        original_inspect = staging._inspect_copy
        def inspect(*args):
            value = original_inspect(*args)
            with self.main.open("ab") as file:
                file.write(b"external change")
            return value
        with patch.object(staging, "_inspect_copy", side_effect=inspect):
            with self.assertRaisesRegex(ValueError, "source player changed"):
                self.run_stage()
        self.assertTrue(self.main.read_bytes().endswith(b"external change"))
        self.assertFalse(list(self.output.glob("Edited Players/*")))

    def test_size_limit_rejects_without_publication(self):
        with patch.object(staging, "MAX_FILE", 64):
            with self.assertRaisesRegex(ValueError, "512 MB"):
                self.run_stage()
        self.assert_clean_failure()

    def test_explicit_output_and_session_output_rejection(self):
        result = self.run_stage(outputDir=str(self.root / "other"))
        self.assertTrue(Path(result["directory"]).is_relative_to(self.root / "other"))
        with self.assertRaisesRegex(ValueError, "durable output"):
            self.run_stage(outputDir=str(self.session / "output"))
        self.assertEqual(digest(self.main), self.before)

    def test_binary_metadata_normalized_only_on_copy_using_native_decoder(self):
        build = ROOT / "dist/BUILD.json"
        if not build.is_file():
            self.skipTest("Built native decoder unavailable")
        version = json.loads(build.read_text(encoding="utf-8-sig"))["version"]
        exe = ROOT / "dist" / f"Character-Mod-Tool-Tauri-Preview-{version}.exe"
        if not exe.is_file():
            self.skipTest("Built native decoder unavailable")
        source = self.source / "png0003.iff"
        player(source, binary=True)
        before = digest(source)
        with patch.dict(os.environ, CMT_RUST_EXE=str(exe)):
            result = self.run_stage(source)
        self.assertEqual(digest(source), before)
        with zipfile.ZipFile(source) as original, zipfile.ZipFile(result["mainPath"]) as output:
            self.assertEqual(original.namelist(), output.namelist())
            self.assertEqual(original.comment, output.comment)
            self.assertFalse(output.read("hihead.SCNE").startswith(staging.BINARY_DOCUMENT))
            for name in ("vertices.bin", "indices.bin", "appearance_info.json", "untouched.bin"):
                self.assertEqual(original.read(name), output.read(name))
        self.assertTrue(any("expanded 2" in warning for warning in result["warnings"]))

    def test_service_registration_uses_private_session_and_default_output(self):
        from prototype_services import commands
        self.bridge.command_run_swap = lambda _: {}
        self.bridge.command_health = lambda _: {}
        self.bridge.command_health_repair = lambda _: {}
        with patch.dict(os.environ, CMT_SESSION=str(self.session)):
            result = commands(self.bridge, self.root, self.root)["stage-player"]({"filePath": str(self.main)})
        self.assertTrue(Path(result["mainPath"]).is_file())
        self.assertEqual(digest(self.main), self.before)


if __name__ == "__main__":
    unittest.main()
