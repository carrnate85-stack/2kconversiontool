"""Focused non-GUI publication tests; all fixture writes belong to unique TEMP roots."""

import hashlib
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "engine"))
import prototype_player_writes as writes


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(path):
    with zipfile.ZipFile(path) as archive:
        if archive.testzip() or not any(name.lower().endswith(".scne") for name in archive.namelist()):
            raise ValueError("Invalid player IFF")


def player(path, label):
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("player.scne", label)
        archive.writestr("payload.bin", bytes(range(256)) * 32)


def wait_for(path, child):
    deadline = time.monotonic() + 15
    while not path.exists():
        if child.poll() is not None or time.monotonic() >= deadline:
            raise AssertionError(f"Worker did not reach {path.name}; exit={child.poll()}")
        time.sleep(0.005)


def worker(folder, phase):
    target, source, backup = (folder / name for name in ("target.iff", "source.iff", "backup.iff"))
    replace = writes.os.replace
    copy = writes._copy_synced

    def pause():
        (folder / "ready").touch()
        time.sleep(60)

    def held_copy(input_path, output):
        if phase == "copy" and Path(input_path) == source:
            output.write(b"partial unpublished bytes")
            output.flush()
            os.fsync(output.fileno())
            pause()
        copy(input_path, output)

    def held_replace(temporary, destination):
        if phase == "before-replace":
            pause()
        if phase == "race-replace":
            (folder / "ready").touch()
            deadline = time.monotonic() + 15
            while not (folder / "go").exists():
                if time.monotonic() >= deadline:
                    raise RuntimeError("Publication race was not released")
                time.sleep(0.001)
        replace(temporary, destination)
        if phase in ("after-replace", "race-replace"):
            pause()

    with patch.object(writes, "_copy_synced", held_copy), patch.object(writes.os, "replace", held_replace):
        writes.publish_player(source, target, backup, validate=validate, digest=digest)


class PlayerPublication(unittest.TestCase):
    def setUp(self):
        self.folder = tempfile.TemporaryDirectory(prefix="cmt-player-publication-")
        self.root = Path(self.folder.name).resolve()
        self.source = self.root / "source.iff"
        self.target = self.root / "target.iff"
        self.backup = self.root / "backup.iff"
        player(self.source, "replacement")
        player(self.target, "original user work")
        self.original = self.target.read_bytes()
        self.replacement = self.source.read_bytes()
        self.sentinel = self.root / ".target.iff.cmt-publish-preexisting.tmp"
        self.sentinel.write_bytes(b"unverified leftover")

    def tearDown(self):
        self.assertEqual(self.sentinel.read_bytes(), b"unverified leftover")
        self.assertEqual(self.root.parent, Path(tempfile.gettempdir()).resolve())
        self.assertTrue(self.root.name.startswith("cmt-player-publication-"))
        self.folder.cleanup()

    def publish(self, **options):
        writes.publish_player(self.source, self.target, self.backup,
                              validate=options.pop("validate", validate),
                              digest=options.pop("digest", digest), **options)

    def assert_unchanged(self, backup=None):
        self.assertEqual(self.target.read_bytes(), self.original)
        self.assertEqual(self.source.read_bytes(), self.replacement)
        self.assertEqual(list(self.root.glob(".target.iff.cmt-publish-*.tmp")), [self.sentinel])
        if backup is None:
            self.assertFalse(self.backup.exists())
        else:
            self.assertEqual(self.backup.read_bytes(), backup)

    def test_success_is_exact_bytes_and_preserves_source_backup_and_mode(self):
        self.target.chmod(stat.S_IREAD | stat.S_IWRITE)
        mode = stat.S_IMODE(self.target.stat().st_mode)
        self.assertIsNone(self.publish(expected_digest=digest(self.target)))
        self.assertEqual(self.target.read_bytes(), self.replacement)
        self.assertEqual(self.source.read_bytes(), self.replacement)
        self.assertEqual(self.backup.read_bytes(), self.original)
        self.assertEqual(stat.S_IMODE(self.target.stat().st_mode), mode)
        self.assertEqual(list(self.root.glob(".target.iff.cmt-publish-*.tmp")), [self.sentinel])

    def test_matching_preexisting_backup_is_reused_not_rewritten(self):
        self.backup.write_bytes(self.original)
        before = self.backup.stat()
        self.publish()
        after = self.backup.stat()
        self.assertEqual((after.st_ino, after.st_mtime_ns), (before.st_ino, before.st_mtime_ns))
        self.assertEqual(self.backup.read_bytes(), self.original)

    def test_different_preexisting_backup_is_never_overwritten_or_deleted(self):
        self.backup.write_bytes(b"existing user backup")
        with self.assertRaises(FileExistsError):
            self.publish()
        self.assert_unchanged(b"existing user backup")

    def test_source_validation_failure_does_not_create_outputs(self):
        self.source.write_bytes(b"invalid source")
        with self.assertRaises(zipfile.BadZipFile):
            self.publish()
        self.assertEqual(self.target.read_bytes(), self.original)
        self.assertFalse(self.backup.exists())
        self.assertEqual(list(self.root.glob(".target.iff.cmt-publish-*.tmp")), [self.sentinel])

    def test_target_validation_failure_does_not_replace_target(self):
        self.target.write_bytes(b"invalid target")
        with self.assertRaises(zipfile.BadZipFile):
            self.publish()
        self.assertEqual(self.target.read_bytes(), b"invalid target")
        self.assertFalse(self.backup.exists())

    def test_staged_copy_validation_failure_cleans_only_owned_files(self):
        def reject_copy(path):
            validate(path)
            if Path(path) not in (self.source, self.target):
                raise ValueError("staged validation failure")
        with self.assertRaisesRegex(ValueError, "staged validation"):
            self.publish(validate=reject_copy)
        self.assert_unchanged()

    def test_expected_digest_conflict_preserves_all_outputs(self):
        with self.assertRaisesRegex(ValueError, "changed while Blender"):
            self.publish(expected_digest="old return digest")
        self.assert_unchanged()

    def test_partial_copy_error_leaves_target_intact(self):
        def failed_copy(source, output):
            output.write(b"partial")
            raise OSError("simulated copy failure")
        with patch.object(writes, "_copy_synced", failed_copy):
            with self.assertRaisesRegex(OSError, "copy failure"):
                self.publish()
        self.assert_unchanged()

    def test_partial_backup_error_cleans_new_backup_and_temp(self):
        copy = writes._copy_synced
        def failed_backup(source, output):
            if Path(source) == self.target:
                output.write(b"partial backup")
                raise OSError("simulated backup failure")
            copy(source, output)
        with patch.object(writes, "_copy_synced", failed_backup):
            with self.assertRaisesRegex(OSError, "backup failure"):
                self.publish()
        self.assert_unchanged()

    def test_copy_digest_mismatch_cannot_publish_valid_but_wrong_zip(self):
        def wrong_copy(source, output):
            output.write(self.original)
        with patch.object(writes, "_copy_synced", wrong_copy):
            with self.assertRaisesRegex(ValueError, "source changed"):
                self.publish()
        self.assert_unchanged()

    def test_source_change_during_copy_is_rejected(self):
        copy = writes._copy_synced
        def changed_source(source, output):
            copy(source, output)
            if Path(source) == self.source:
                player(self.source, "changed source")
        with patch.object(writes, "_copy_synced", changed_source):
            with self.assertRaisesRegex(ValueError, "source changed"):
                self.publish()
        self.assertEqual(self.target.read_bytes(), self.original)
        self.assertFalse(self.backup.exists())
        self.assertEqual(list(self.root.glob(".target.iff.cmt-publish-*.tmp")), [self.sentinel])

    def test_external_target_change_before_replace_is_not_overwritten(self):
        copy = writes._copy_synced
        def changed_target(source, output):
            copy(source, output)
            if Path(source) == self.target:
                player(self.target, "newer external edit")
        with patch.object(writes, "_copy_synced", changed_target):
            with self.assertRaisesRegex(ValueError, "changed while Blender"):
                self.publish()
        with zipfile.ZipFile(self.target) as archive:
            self.assertEqual(archive.read("player.scne"), b"newer external edit")
        self.assertFalse(self.backup.exists())

    def test_replace_failure_preserves_existing_backup_and_cleans_temp(self):
        self.backup.write_bytes(self.original)
        with patch.object(writes.os, "replace", side_effect=PermissionError("locked target")):
            with self.assertRaises(PermissionError):
                self.publish()
        self.assert_unchanged(self.original)

    def test_replace_failure_cleans_new_backup_and_temp(self):
        with patch.object(writes.os, "replace", side_effect=PermissionError("locked target")):
            with self.assertRaises(PermissionError):
                self.publish()
        self.assert_unchanged()

    @unittest.skipUnless(os.name == "nt", "Windows sharing lock")
    def test_real_windows_locked_target_is_unchanged(self):
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                      wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.CloseHandle.restype = wintypes.BOOL
        handle = kernel.CreateFileW(str(self.target), 0x80000000, 3, None, 3, 0x80, None)
        self.assertNotEqual(handle, ctypes.c_void_p(-1).value)
        try:
            with self.assertRaises(PermissionError):
                self.publish()
            self.assert_unchanged()
        finally:
            self.assertTrue(kernel.CloseHandle(handle))

    @unittest.skipUnless(os.name == "nt", "Windows read-only replacement semantics")
    def test_readonly_target_failure_preserves_mode_and_cleans_temp(self):
        self.target.chmod(stat.S_IREAD)
        try:
            with self.assertRaises(PermissionError):
                self.publish()
            self.assertFalse(self.target.stat().st_mode & stat.S_IWRITE)
            self.assert_unchanged()
        finally:
            self.target.chmod(stat.S_IREAD | stat.S_IWRITE)

    def test_same_file_and_backup_aliases_are_rejected(self):
        for source, target, backup in ((self.source, self.source, self.backup),
                                       (self.source, self.target, self.target),
                                       (self.source, self.target, self.source)):
            with self.subTest(source=source, target=target, backup=backup):
                with self.assertRaises(shutil.SameFileError):
                    writes.publish_player(source, target, backup, validate=validate, digest=digest)
        self.assert_unchanged()

    def test_cleanup_does_not_delete_a_replaced_temporary(self):
        moved = self.root / "owned-temp-moved-aside"
        def fail_replace(temporary, target):
            Path(temporary).rename(moved)
            Path(temporary).write_bytes(b"replacement owned by someone else")
            raise PermissionError("simulated replaced temporary")
        with patch.object(writes.os, "replace", fail_replace):
            with self.assertRaises(PermissionError):
                self.publish()
        leftovers = [p for p in self.root.glob(".target.iff.cmt-publish-*.tmp") if p != self.sentinel]
        self.assertEqual(len(leftovers), 1)
        self.assertEqual(leftovers[0].read_bytes(), b"replacement owned by someone else")
        self.assertEqual(self.target.read_bytes(), self.original)
        self.assertFalse(self.backup.exists())

    def interrupt(self, phase, index=0):
        folder = self.root / f"worker-{phase}-{index}"
        folder.mkdir()
        source, target, backup = (folder / name for name in ("source.iff", "target.iff", "backup.iff"))
        source.write_bytes(self.replacement)
        target.write_bytes(self.original)
        child = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()),
                                  "--worker", str(folder), phase],
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
        try:
            wait_for(folder / "ready", child)
            if phase == "race-replace":
                (folder / "go").touch()
                if index % 2:
                    time.sleep(0.003)
            child.kill()
            stdout, stderr = child.communicate(timeout=10)
            self.assertNotEqual(child.returncode, 0, (stdout, stderr))
            actual = target.read_bytes()
            self.assertIn(actual, (self.original, self.replacement))
            validate(target)
            self.assertEqual(source.read_bytes(), self.replacement)
            if phase in ("copy", "before-replace"):
                self.assertEqual(actual, self.original)
            if phase == "after-replace":
                self.assertEqual(actual, self.replacement)
            if backup.exists():
                self.assertEqual(backup.read_bytes(), self.original)
            return {"phase": phase, "target": "original" if actual == self.original else "replacement"}
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=10)

    def test_process_termination_during_copy(self):
        self.interrupt("copy")

    def test_process_termination_immediately_before_atomic_replace(self):
        self.interrupt("before-replace")

    def test_process_termination_immediately_after_atomic_replace(self):
        self.interrupt("after-replace")

    def test_process_termination_racing_atomic_replace(self):
        results = []
        for index in range(8):
            with self.subTest(iteration=index):
                results.append(self.interrupt("race-replace", index))
        print("Publication termination race: " + json.dumps(results), flush=True)


if __name__ == "__main__":
    if sys.argv[1:2] == ["--worker"]:
        worker(Path(sys.argv[2]), sys.argv[3])
    else:
        unittest.main(verbosity=2)
