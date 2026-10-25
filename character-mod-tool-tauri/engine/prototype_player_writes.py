"""Publish validated player bytes without writing through the destination file."""

import os
import shutil
import stat
import tempfile
from pathlib import Path


CONFLICT_MESSAGE = (
    "The staged player changed while Blender was open. "
    "Reopen it before returning tweaks."
)


def _identity(metadata):
    return metadata.st_dev, metadata.st_ino


def _remove_owned(path, identity):
    try:
        metadata = path.stat(follow_symlinks=False)
        if stat.S_ISREG(metadata.st_mode) and _identity(metadata) == identity:
            if not metadata.st_mode & stat.S_IWRITE:
                path.chmod(metadata.st_mode | stat.S_IWRITE)
            path.unlink()
    except OSError:
        # A locked or replaced leftover is not permission to delete another file.
        pass


def _copy_synced(source, destination):
    with source.open("rb") as stream:
        shutil.copyfileobj(stream, destination, length=1024 * 1024)
    destination.flush()
    os.fsync(destination.fileno())


def publish_player(source, target, backup, *, validate, digest, expected_digest=None):
    """Copy, validate, back up, then atomically replace an existing player.

    ``validate(path)`` must raise for an invalid player; ``digest(path)`` must
    return a content digest (the services use SHA-256). Neither callback may
    modify files. The source remains intact. Existing backups are reused only
    when they exactly match the original target, never overwritten or removed.
    ``expected_digest`` optionally enforces the existing Blender return contract.

    Callers must serialize app-owned mutations. Digest checks detect external
    changes during preparation, but are not a filesystem compare-and-swap.
    Process termination may leave a uniquely named temp or a backup; the target
    remains either the complete original or the complete replacement. This is
    process-interruption safety, not a guarantee against storage/power failure.
    """
    source = Path(source).resolve(strict=True)
    target = Path(target).resolve(strict=True)
    backup = Path(backup).resolve()
    if source.samefile(target):
        raise shutil.SameFileError("Player source and target must be different files.")
    if backup in (source, target) or (
        backup.exists() and (backup.samefile(source) or backup.samefile(target))
    ):
        raise shutil.SameFileError("Player backup must be a separate file.")

    validate(source)
    validate(target)
    source_digest = digest(source)
    target_digest = digest(target)
    if expected_digest is not None and target_digest != expected_digest:
        raise ValueError(CONFLICT_MESSAGE)

    temporary = None
    temporary_identity = None
    backup_identity = None
    published = False
    try:
        descriptor, name = tempfile.mkstemp(
            prefix=f".{target.name}.cmt-publish-", suffix=".tmp", dir=target.parent
        )
        temporary = Path(name)
        with os.fdopen(descriptor, "wb") as output:
            temporary_identity = _identity(os.fstat(output.fileno()))
            _copy_synced(source, output)
        validate(temporary)
        if digest(temporary) != source_digest or digest(source) != source_digest:
            raise ValueError("The player source changed while preparing publication.")
        if digest(target) != target_digest:
            raise ValueError(CONFLICT_MESSAGE)

        try:
            output = backup.open("xb")
        except FileExistsError:
            if not backup.is_file() or digest(backup) != target_digest:
                raise FileExistsError("The player backup already exists with different bytes.") from None
        else:
            with output:
                backup_identity = _identity(os.fstat(output.fileno()))
                _copy_synced(target, output)
        if digest(backup) != target_digest:
            raise ValueError("The staged player changed while preparing its backup.")

        shutil.copymode(target, temporary)
        if digest(target) != target_digest:
            raise ValueError(CONFLICT_MESSAGE)
        if digest(source) != source_digest or digest(temporary) != source_digest:
            raise ValueError("The player source changed while preparing publication.")
        if _identity(temporary.stat(follow_symlinks=False)) != temporary_identity:
            raise ValueError("The prepared player file was replaced before publication.")
        os.replace(temporary, target)
        published = True
    finally:
        if temporary is not None and temporary_identity is not None:
            _remove_owned(temporary, temporary_identity)
        if not published and backup_identity is not None:
            _remove_owned(backup, backup_identity)
