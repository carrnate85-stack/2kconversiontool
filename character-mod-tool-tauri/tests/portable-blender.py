import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "engine"))
from prototype_blender_paths import bundled_blender, portable_setting, select_blender


class PortableBlender(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="cmt-portable-blender-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "Release with spaces"
        self.blender = self.root / "Blender/blender.exe"
        self.blender.parent.mkdir(parents=True)
        (self.blender.parent / "5.1/scripts").mkdir(parents=True)
        self.blender.write_bytes(b"test fixture")
        self.exe = self.root / "Character-Mod-Tool.exe"

    def test_new_setup_prefers_bundle_over_installed_discovery(self):
        self.assertEqual(select_blender("installed.exe", "", self.exe), str(self.blender))

    def test_explicit_existing_install_is_preserved(self):
        custom = Path(self.temp.name) / "custom.exe"
        custom.write_bytes(b"custom fixture")
        self.assertEqual(select_blender(str(custom), str(custom), self.exe), str(custom))

    def test_missing_saved_install_falls_back_to_bundle(self):
        self.assertEqual(select_blender("discovered.exe", "missing.exe", self.exe), str(self.blender))

    def test_bundle_setting_is_saved_as_auto(self):
        self.assertEqual(portable_setting(str(self.blender), self.exe), "")

    def test_custom_setting_is_unchanged(self):
        self.assertEqual(portable_setting("custom.exe", self.exe), "custom.exe")

    def test_release_relocation_uses_current_folder(self):
        original = self.root
        moved = original.with_name("Moved release")
        original.rename(moved)
        self.assertEqual(select_blender("installed.exe", "", moved / self.exe.name), str(moved / "Blender/blender.exe"))

    def test_incomplete_bundle_does_not_override_discovery(self):
        (self.blender.parent / "5.1/scripts").rmdir()
        self.assertIsNone(bundled_blender(self.exe))
        self.assertEqual(select_blender("installed.exe", "", self.exe), "installed.exe")

    def test_absent_bundle_and_exe_environment(self):
        self.assertIsNone(bundled_blender(None))
        self.blender.unlink()
        self.assertEqual(select_blender("installed.exe", "", self.exe), "installed.exe")


if __name__ == "__main__":
    unittest.main()
