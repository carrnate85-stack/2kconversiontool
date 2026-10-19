import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wpf_proxy_parity_audit as audit  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]


class Pure(unittest.TestCase):
    def test_bridge_hunks_allowed(self):
        wpf = "a\nb\nc\n"
        v2 = "#define NBA2K27_SESSION_OUTFITS_V2 1\na\n  outfit_match_v2::bridge::start(m);\nb\nc\n"
        self.assertEqual(audit.unexpected_proxy_changes(wpf, v2), [])

    def test_removed_wpf_line_flagged(self):
        self.assertEqual(audit.unexpected_proxy_changes("a\nkeep();\n", "a\n"), ["-keep();"])

    def test_foreign_addition_flagged(self):
        bad = audit.unexpected_proxy_changes("a\n", "a\nWriteProcessMemory(x);\n")
        self.assertEqual(bad, ["+WriteProcessMemory(x);"])

    def test_profiles_parsed(self):
        text = "if(timestamp==0x6A974D13 && image_size==0x368C3000)return &o;"
        self.assertEqual(audit.freestyle_profiles(text), [(0x6A974D13, 0x368C3000)])

    def test_embedded_path(self):
        rs = 'const PROXY: &[u8] = include_bytes!("../x/version.dll");'
        self.assertEqual(audit.embedded_legacy_dll(rs), "../x/version.dll")

    def test_manager_adjust(self):
        self.assertTrue(audit.has_manager_adjust("const auto primary=q(table)==base+kSecondary?table-0x78:table;"))
        self.assertFalse(audit.has_manager_adjust("const auto primary=table;"))


@unittest.skipUnless((ROOT / audit.SHIPPED).exists(), "workspace not present")
class Workspace(unittest.TestCase):
    def test_real_tree(self):
        r = audit.audit(ROOT)
        c = r["checks"]
        self.assertEqual(c["legacy_dll_identity"]["verdict"], "PASS")
        self.assertEqual(c["v2_proxy_is_wpf_plus_bridge"]["verdict"], "PASS", c["v2_proxy_is_wpf_plus_bridge"])
        self.assertIn(c["freestyle_v2_profile"]["verdict"], ("PRESENT", "MISSING"))


if __name__ == "__main__":
    unittest.main()
