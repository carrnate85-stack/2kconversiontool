import ast
import json
import tempfile
import unittest
import sys
from pathlib import Path

source = Path(__file__).resolve().parents[1] / "engine/prototype_bridge.py"
sys.path.insert(0, str(source.parent))
import prototype_blender_paths
tree = ast.parse(source.read_text())
function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "private_settings")


class RuntimePaths(unittest.TestCase):
    def test_managed_paths_follow_current_runtime_but_custom_paths_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix="cmt-runtime-paths-") as folder:
            data = Path(folder) / "app"
            data.mkdir()
            settings_path = data / "settings.json"
            root = data / "Runtime/engine-new/Backend"
            for managed in (True, False):
                base = data / "Runtime/engine-old/Backend/blender_tools" if managed else Path(folder) / "custom-addons"
                values = {key: str(base / name) for key, name in (
                    ("head_swap_tool", "NBA_Character_HeadSwap"), ("mesh_data_transfer", "mesh_data_transfer"))}
                settings_path.write_text(json.dumps(values))
                before = settings_path.read_bytes()
                namespace = dict(Path=Path, json=json, data=data, root=root, settings_path=settings_path,
                                 prototype_blender_paths=prototype_blender_paths, os=__import__('os'),
                                 original_fast_settings=lambda: dict(values))
                exec(compile(ast.Module(body=[function], type_ignores=[]), str(source), "exec"), namespace)
                result = namespace["private_settings"]()
                for key, name in (("head_swap_tool", "NBA_Character_HeadSwap"), ("mesh_data_transfer", "mesh_data_transfer")):
                    expected = str(root / "blender_tools" / name) if managed else values[key]
                    self.assertEqual(result[key], expected)
                self.assertEqual(settings_path.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
