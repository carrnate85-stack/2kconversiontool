"""Member digests of WPF's verified animation-preview profile assets.

WPF's Nba2kVczDecoder re-zips members (.NET CompressionLevel.Optimal), so the
Rust launcher cannot reproduce its file bytes; member content is comparable.
Reads a WPF-prepared profile whose files match revision.json sha256 values and
writes assets/animation-preview/member-digests.json:
  digest = sha256 over sorted members of  name + "\n" + sha256(content) + "\n"
hands.iff covers hand_pose_male.SingleMoCap only (the launcher keeps that clip).

  python tools/animation_preview_member_digests.py <WPF profile dir>
"""
import hashlib, json, sys, zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
OUT = HERE.parent / 'assets/animation-preview/member-digests.json'


def member_digest(path, only=None):
    digest = hashlib.sha256()
    with zipfile.ZipFile(path) as archive:
        for name in sorted(archive.namelist()):
            if only and name != only:
                continue
            digest.update(f'{name}\n{hashlib.sha256(archive.read(name)).hexdigest()}\n'.encode())
    return digest.hexdigest()


def main():
    profile = Path(sys.argv[1])
    revision = json.loads((HERE.parent / 'assets/animation-preview/revision.json').read_text())
    result = {'schema': 1, 'source': 'WPF PlayerAnimationPreview verified profile (file sha256 = revision.json)', 'assets': {}}
    for name, meta in revision['assets'].items():
        path = profile / 'assets' / name
        file_sha = hashlib.sha256(path.read_bytes()).hexdigest()
        if file_sha != meta['sha256']:
            raise SystemExit(f'{name}: {file_sha} is not the verified asset')
        result['assets'][name] = member_digest(path, 'hand_pose_male.SingleMoCap' if name == 'hands.iff' else None)
        print(name, result['assets'][name])
    OUT.write_text(json.dumps(result, indent=1) + '\n')


if __name__ == '__main__':
    main()
