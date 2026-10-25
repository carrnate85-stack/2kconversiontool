"""Keep parity normalization narrow enough to expose real geometry differences."""
import gzip
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path


class ArchiveComparisonTests(unittest.TestCase):
    def compare(self, left, right, mappings=None):
        with tempfile.TemporaryDirectory(prefix="CmtArchiveCompare-") as temporary:
            root = Path(temporary)
            for side, files in (("wpf", left), ("tauri", right)):
                directory = root / side
                directory.mkdir()
                for name, entries in files.items():
                    with zipfile.ZipFile(directory / name, "w") as archive:
                        for entry, data in entries.items():
                            archive.writestr(entry, data)
            args = [sys.executable, "-B", str(Path(__file__).with_name("wpf-output-compare.py")), str(root / "wpf"), str(root / "tauri")]
            if mappings is not None:
                mapping = root / "ids.json"
                mapping.write_text(json.dumps(mappings))
                args.append(str(mapping))
            result = subprocess.run(args, capture_output=True, text=True, check=True)
            return json.loads(result.stdout)

    def test_binary_change_is_detected(self):
        result = self.compare({"player.iff": {"positions": b"abc"}}, {"player.iff": {"positions": b"abd"}})
        self.assertEqual(len(result["different"]), 1)

    def test_repeated_property_changes_are_not_dropped(self):
        left = {"player.iff": {"scene": b'{"Model":{"head":{"Size":3}},"Model":{"body":{"Size":6}}}'}}
        right = {"player.iff": {"scene": b'{"Model":{"head":{"Size":4}},"Model":{"body":{"Size":6}}}'}}
        self.assertEqual(len(self.compare(left, right)["different"]), 1)
        self.assertEqual(self.compare(left, left)["different"], [])

    def test_duplicate_properties_are_not_equal_to_collapsed_object(self):
        left = {"player.iff": {"scene": b'{"Size":3,"Size":6}'}}
        right = {"player.iff": {"scene": b'{"Size":6}'}}
        self.assertEqual(len(self.compare(left, right)["different"]), 1)

    def test_duplicate_property_order_is_preserved(self):
        left = {"player.iff": {"scene": b'{"Size":3,"Size":6}'}}
        right = {"player.iff": {"scene": b'{"Size":6,"Size":3}'}}
        self.assertEqual(len(self.compare(left, right)["different"]), 1)

    def test_objects_do_not_collide_with_pair_shaped_arrays(self):
        left = {"player.iff": {"scene": b'{"Size":3}'}}
        right = {"player.iff": {"scene": b'[["Size",3]]'}}
        self.assertEqual(len(self.compare(left, right)["different"]), 1)

    def test_asset_id_normalization_cannot_collapse_distinct_document_keys(self):
        mappings = {"wpf": {"hair_a": "hair_fixed", "hair_b": "hair_fixed"}, "tauri": {}}
        left = {"player.iff": {"scene": b'{"hair_a":1,"hair_b":2}'}}
        right = {"player.iff": {"scene": b'{"hair_fixed":2}'}}
        with self.assertRaises(subprocess.CalledProcessError):
            self.compare(left, right, mappings)

    def test_non_gzip_magic_remains_opaque(self):
        files = {"player.iff": {"vcz": b"\x1f\x8b\x00game-data"}}
        self.assertEqual(self.compare(files, files)["different"], [])

    def test_gzip_timestamp_and_json_key_order_are_ignored(self):
        left = {"player.iff": {"data": gzip.compress(b'{"x":1,"y":2}', mtime=1)}}
        right = {"player.iff": {"data": gzip.compress(b'{"y":2,"x":1}', mtime=2)}}
        self.assertEqual(self.compare(left, right)["different"], [])

    def test_only_explicit_asset_ids_are_normalized(self):
        mappings = {"wpf": {"hair_unique_a": "hair_fixed"}, "tauri": {"hair_unique_b": "hair_fixed"}}
        left = {"hair_unique_a.iff": {"appearance_info.json": b'{"path":"hair_unique_a.iff","other":1}', "positions": b"abc"}}
        right = {"hair_unique_b.iff": {"appearance_info.json": b'{"path":"hair_unique_b.iff","other":1}', "positions": b"abc"}}
        self.assertEqual(self.compare(left, right, mappings)["different"], [])
        right["hair_unique_b.iff"]["positions"] = b"abd"
        self.assertEqual(len(self.compare(left, right, mappings)["different"]), 1)

    def test_game_json_fragment_matches_wrapped_document(self):
        left = {"player.iff": {"scene": b'"head":{"Size":3}'}}
        right = {"player.iff": {"scene": b'{"head":{"Size":3}}'}}
        self.assertEqual(self.compare(left, right)["different"], [])

    def test_integral_json_numbers_are_equal_without_rounding_fractions(self):
        left = {"player.iff": {"scene": b'{"Size":3,"Min":0.125}'}}
        right = {"player.iff": {"scene": b'{"Size":3.0,"Min":0.125}'}}
        self.assertEqual(self.compare(left, right)["different"], [])
        right["player.iff"]["scene"] = b'{"Size":3.0,"Min":0.12501}'
        self.assertEqual(len(self.compare(left, right)["different"]), 1)

    def test_binary_alias_requires_present_uncompressed_member(self):
        left = {"player.iff": {"scene": b'{"Binary":"vertices.gz"}', "vertices.bin": b"abc"}}
        right = {"player.iff": {"scene": b'{"Binary":"vertices.bin"}', "vertices.bin": b"abc"}}
        self.assertEqual(self.compare(left, right)["different"], [])
        del left["player.iff"]["vertices.bin"]
        del right["player.iff"]["vertices.bin"]
        self.assertEqual(len(self.compare(left, right)["different"]), 1)

    def test_embedded_shared_texture_requires_exact_reconstruction_hash(self):
        data = b"DDS " + bytes(range(128))
        mappings = {"wpf": {}, "tauri": {}, "sharedDds": {"wpf": {"player.iff": {"color.dds": hashlib.sha256(data).hexdigest()}}}}
        left = {"player.iff": {"color.TXTR": b"metadata"}}
        right = {"player.iff": {"color.TXTR": b"metadata", "color.dds": data}}
        matched = self.compare(left, right, mappings)
        self.assertEqual(matched["different"], [])
        self.assertEqual(len(matched["resolvedSharedTextures"]), 1)
        right["player.iff"]["color.dds"] += b"changed"
        self.assertEqual(len(self.compare(left, right, mappings)["different"]), 1)


if __name__ == "__main__":
    unittest.main(verbosity=2)
