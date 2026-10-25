"""Exercise the proposed wiring in memory, without changing the shared services file."""

import ast
import importlib.util
import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "engine"))
import prototype_services


def proposed_services():
    path = ROOT / "engine" / "prototype_services.py"
    tree = ast.parse(path.read_text(encoding="utf-8-sig"), filename=str(path))
    replacements = 0
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef) and node.name == "replace_player":
            node.body = ast.parse(
                "from prototype_player_writes import publish_player\n"
                "publish_player(source, target, backup, validate=valid_player, digest=file_digest)\n"
            ).body
            replacements += 1
    if replacements != 1:
        raise AssertionError(f"Expected one nested replace_player, found {replacements}")
    exec(compile(ast.fix_missing_locations(tree), str(path), "exec"), prototype_services.__dict__)


if __name__ == "__main__":
    proposed_services()
    path = ROOT / "tests" / "blender-services.py"
    spec = importlib.util.spec_from_file_location("publication_blender_services", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(module.BlenderServices)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    raise SystemExit(0 if result.wasSuccessful() else 1)
