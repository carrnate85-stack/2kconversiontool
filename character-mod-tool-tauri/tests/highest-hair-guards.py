"""A verified LOD difference must not permit changed indices, positions or UVs."""
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
report = json.loads((root / "artifacts/combined-swap-parity-report.json").read_text())
actual = Path(report["output"]).parent / "png28001_geo_hair_dreads.iff"
fixture = actual.parent.parent
engine = root.parent / "character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend"
source = fixture / "source/png4109_hair_01.iff"
template = fixture / "wpf" / actual.name
sys.path.insert(0, str(engine))
from hair_tools.hair_picker import read_hair_document

class HighestHairGuards(unittest.TestCase):
    def reject_changed(self, role):
        before = hashlib.sha256(actual.read_bytes()).hexdigest()
        with zipfile.ZipFile(actual) as archive:
            entries = [(info, archive.read(info)) for info in archive.infolist()]
        scene = read_hair_document(next(data for info, data in entries if info.filename.lower().endswith(".scne")))
        model = next(iter(scene.values()))["Model"]["hihead"]
        descriptor = model["IndexBuffer"] if role == "indices" else model["VertexStream"][0 if role == "positions" else 1]
        name = descriptor["Binary"].replace(".gz", ".bin")
        with tempfile.TemporaryDirectory(prefix="CmtHairPolicyGuard-") as temporary:
            directory = Path(temporary)
            changed = directory / actual.name
            with zipfile.ZipFile(changed, "w") as archive:
                for info, data in entries:
                    if info.filename == name:
                        data = bytearray(data)
                        offset = int(model["VertexFormat"]["TEXCOORD0"]["ByteOffset"]) if role == "uvs" else 0
                        if role == "positions":
                            import struct
                            struct.pack_into("<f", data, offset, struct.unpack_from("<f", data, offset)[0] + .1)
                        else:
                            data[offset] ^= 1
                    archive.writestr(info, data)
            result = subprocess.run([sys.executable, "-B", str(root / "tests/highest-hair-policy.py"),
                                     str(engine), str(source), str(changed), str(template),
                                     str(directory / "expected.iff"), str(directory / "proof.json")],
                                    capture_output=True, text=True, timeout=30)
            self.assertNotEqual(result.returncode, 0, "Changed " + role + " incorrectly accepted")
            self.assertIn("AssertionError", result.stderr)
        self.assertEqual(hashlib.sha256(actual.read_bytes()).hexdigest(), before)

    def test_changed_indices_are_rejected(self):
        self.reject_changed("indices")

    def test_changed_positions_are_rejected(self):
        self.reject_changed("positions")

    def test_changed_uvs_are_rejected(self):
        self.reject_changed("uvs")

if __name__ == "__main__":
    unittest.main(verbosity=2)
