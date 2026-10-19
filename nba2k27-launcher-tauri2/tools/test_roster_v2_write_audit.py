"""Regenerates the Version 2 roster write proof from the exact disk image and
requires it to equal the committed asset. Disk only; never opens the game.

    set C27_GAME_EXE=...\\NBA2K27.exe && python -I -m unittest discover -s tools -p "test_roster_v2_write_audit.py"
"""
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IMAGE = os.environ.get("C27_GAME_EXE")


@unittest.skipUnless(IMAGE and Path(IMAGE).is_file(), "C27_GAME_EXE (exact Version 2 image) not set")
class WriteProofIsReproducible(unittest.TestCase):
    def test_regenerated_asset_matches_committed_asset(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "proof.json"
            result = subprocess.run([sys.executable, "-I", str(ROOT / "tools/roster_v2_write_audit.py"), IMAGE, "--asset", str(out)],
                                    capture_output=True, text=True, timeout=600)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertEqual(out.read_bytes(), (ROOT / "assets/roster-v2-write-proof.json").read_bytes())


if __name__ == "__main__":
    unittest.main()
