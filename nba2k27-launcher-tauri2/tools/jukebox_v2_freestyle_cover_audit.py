"""Offline-only Version 2 Jukebox Freestyle-playlist / cover-art audit.

Opens NBA2K27.exe, the `0A` audio index and the `manifest` (plus a few small
cover ranges of the archives it names) as ordinary read-only files. It never
opens a process, writes game files or executes game code. Every code check is
pinned to the exact Version 2 image (PE 6AC53765 / SizeOfImage 35BDE000) and
verifies fixed instruction bytes, so it runs in seconds.

What it proves (see artifacts/jukebox-v2-freestyle-cover-research-20261017.md):

Freestyle (WPF FREESTYLE_PLAYLIST=992, native freestyle_redirect):
1. The event-name key table moved to 0x677F930 and still holds 0xFAB3211A at
   index 992 (the hash appears exactly once in the image).
2. The bank-34 music-event object WPF read live is the V2 global 0xA31E7D0:
   its initializer stores bank 0x22 at +0x14C and flag 1 at +0x44, the play
   path checks [obj+0x44]==1, stores the event index at +0x40 before opening
   audio and sets +0x10 only after the open -- WPF's exact field contract.
3. A 7-byte `lea rcx,[0xA31E7D0]` anchor exists for a V2 build profile.
4. All 58 WPF event-992 track paths exist (once each) in the V2 0A index.
   eventmusic IDs are dense there, so membership needs a live proof.

Cover art (WPF jukebox-artwork-session.js):
5. Stock cover TLDs keep the 256x256 BC1 header the Rust codec writes; V2
   stock payloads are compressed (stored size < 32768), Rust writes stored ==
   raw 32768, which is what WPF published.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import mmap
import pathlib
import re
import struct
import sys

EXPECTED_SHA256 = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
EXPECTED_STAMP = 0x6AC53765
EXPECTED_IMAGE_SIZE = 0x35BDE000
DEFAULT_GAME = r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27"

FREESTYLE_EVENT = 992
FREESTYLE_KEY = 0xFAB3211A
FREESTYLE_BANK = 0x22
# Same list as nba2k27-version-proxy/src/freestyle_audio.hpp (old-build live capture).
FREESTYLE_TRACKS = (
    4400, 4405, 4408, 4412, 4417, 4421, 4433, 4438, 4447, 4451, 4456, 4461, 4463, 4465,
    4475, 4477, 4481, 4488, 4494, 4498, 4502, 4506, 4511, 4515, 4520, 4523, 4527, 4536,
    4540, 4550, 4554, 4568, 4572, 4576, 4582, 4587, 4590, 4597, 4602, 4607, 4611, 4614,
    4617, 4622, 4627, 4394, 4429, 4426, 4443, 4469, 4472, 4485, 4531, 4544, 4547, 4559,
    4563, 4593,
)
V2 = {
    "keys": 0x677F930,
    "event": 0xA31E7D0,
    "anchor": 0x25C4A7,
    "playEvent": 0x25C0C0,
    "startTrack": 0x25C600,
}
# Old-build profiles from freestyle_build_profile.hpp, for the report only.
OLD_PROFILES = {
    "original 6A974D13/368C3000": {"anchor": 0x25984E, "event": 0xA2908F0, "keys": 0x67184A0},
    "september11 6AA2013D/36558000": {"anchor": 0x25994E, "event": 0xA2948F0, "keys": 0x671B4C0},
}
# The 28 fixed header bytes jukebox_artwork.rs writes before its stored size.
COVER_HEADER = bytes.fromhex("544c442000000000052147011f000000000100010100010000800000")
COVER_LOGICAL = re.compile(r"^shared/([0-9a-f]{2})/(\d{3}_[a-z0-9_]+)\.([0-9a-f]{16})\.tld$")


def archive_key(name: str) -> int:
    """jukebox_v2_resources.rs archive_key: CRC-64/XZ of the upper-cased path."""
    crc = (1 << 64) - 1
    for b in name.upper().encode("ascii"):
        crc ^= b
        for _ in range(8):
            crc = (crc >> 1) ^ (0xC96C5795D7870F42 if crc & 1 else 0)
    return crc ^ ((1 << 64) - 1)


def audio_path(folder: str, slot: int) -> str:
    return f"newaudio/{folder}/{slot:06d}-000000-000000-000000-000000-000000.wav"


class Image:
    def __init__(self, data):
        self.data = data
        pe = struct.unpack_from("<I", data, 0x3C)[0]
        if bytes(data[pe : pe + 4]) != b"PE\0\0":
            raise ValueError("not a PE image")
        count = struct.unpack_from("<H", data, pe + 6)[0]
        self.stamp = struct.unpack_from("<I", data, pe + 8)[0]
        optional = struct.unpack_from("<H", data, pe + 20)[0]
        self.image_size = struct.unpack_from("<I", data, pe + 24 + 56)[0]
        self.sections = []
        for i in range(count):
            o = pe + 24 + optional + 40 * i
            name = bytes(data[o : o + 8]).rstrip(b"\0").decode("ascii", "replace")
            vsize, va, rsize, raw = struct.unpack_from("<IIII", data, o + 8)
            self.sections.append((name, va, vsize, raw, rsize))

    def section_of(self, rva):
        for s in self.sections:
            if s[1] <= rva < s[1] + max(s[2], s[4]):
                return s
        return None

    def file_backed(self, rva):
        s = self.section_of(rva)
        return bool(s) and rva < s[1] + s[4]

    def read(self, rva, size):
        s = self.section_of(rva)
        if not s or rva + size > s[1] + s[4]:
            raise ValueError(f"rva {rva:#x} is not file-backed")
        off = s[3] + rva - s[1]
        return bytes(self.data[off : off + size])

    def u32(self, rva):
        return struct.unpack("<I", self.read(rva, 4))[0]


def rip_target(image, rva, prefix, trailing=0):
    """Target of a rip-relative instruction `prefix disp32 [imm trailing]` at rva."""
    if image.read(rva, len(prefix)) != prefix:
        return None
    disp = struct.unpack("<i", image.read(rva + len(prefix), 4))[0]
    return rva + len(prefix) + 4 + trailing + disp


def branch_target(image, rva):
    op = image.read(rva, 1)
    if op not in (b"\xe8", b"\xe9"):
        return None
    return rva + 5 + struct.unpack("<i", image.read(rva + 1, 4))[0]


def check_freestyle(image, index_bytes=None):
    ev, keys = V2["event"], V2["keys"]
    checks = []

    def add(name, ok, detail):
        checks.append({"check": name, "ok": bool(ok), "detail": detail})

    key = image.u32(keys + FREESTYLE_EVENT * 4)
    add("keyTable[992] == 0xFAB3211A", key == FREESTYLE_KEY, f"{key:#010x} at {keys + FREESTYLE_EVENT * 4:#x}")
    needle = struct.pack("<I", FREESTYLE_KEY)
    hits, i = [], image.data.find(needle)
    while i != -1 and len(hits) < 3:
        hits.append(i)
        i = image.data.find(needle, i + 1)
    add("0xFAB3211A unique in image", len(hits) == 1, f"{len(hits)} file hit(s)")
    for rva, prefix in ((0x25C66A, b"\x48\x8d\x0d"), (0x25E3E6, b"\x48\x8d\x15")):
        t = rip_target(image, rva, prefix)
        add(f"lea @{rva:#x} -> key table", t == keys, f"{t:#x}" if t is not None else "pattern mismatch")
    t = rip_target(image, 0x25C671 - 7, b"\x48\x8d\x0d")
    add("startTrack indexes keys[esi] (42 8b 14 b9)", image.read(0x25C671, 4) == bytes.fromhex("428b14b9") and t == keys, "")

    anchor = image.read(V2["anchor"], 7)
    t = rip_target(image, V2["anchor"], b"\x48\x8d\x0d")
    add("anchor lea rcx,[event]", t == ev, f"{anchor.hex()} -> {t:#x}" if t is not None else anchor.hex())
    t = rip_target(image, 0x288026, b"\x48\x8d\x35")
    add("initializer lea rsi,[event]", t == ev, f"{t:#x}" if t is not None else "pattern mismatch")
    t = rip_target(image, 0x288036, b"\xc7\x05", 4)
    imm = image.u32(0x288036 + 6)
    add("initializer [event+0x14C] = 0x22 (bank 34)", t == ev + 0x14C and imm == FREESTYLE_BANK, f"{t:#x} imm {imm:#x}" if t else "")
    t = rip_target(image, 0x2880F4, b"\xc6\x05", 1)
    imm = image.read(0x2880F4 + 6, 1)[0]
    add("initializer [event+0x44] = 1", t == ev + 0x44 and imm == 1, f"{t:#x} imm {imm}" if t else "")
    t = rip_target(image, 0x25C5E3, b"\x48\x8d\x0d")
    add("lea rcx,[event]; jmp playEvent", t == ev and branch_target(image, 0x25C5F5) == V2["playEvent"], "0x25c5e3/0x25c5f5")
    t = rip_target(image, 0x25ED4E, b"\x48\x8d\x0d")
    add("lea rcx,[event]; call playEvent", t == ev and branch_target(image, 0x25ED5A) == V2["playEvent"], "0x25ed4e/0x25ed5a")
    add("playEvent checks [rcx+0x44]==1 and reads +0x40",
        image.read(0x25C10F, 4) == bytes.fromhex("80794401") and image.read(0x25C119, 3) == bytes.fromhex("8b4e40"), "0x25c10f/0x25c119")
    add("playEvent calls startTrack", branch_target(image, 0x25C3EC) == V2["startTrack"], "0x25c3ec")
    add("startTrack checks +0x44, reads bank +0x14C",
        image.read(0x25C634, 4) == bytes.fromhex("80794401") and image.read(0x25C656, 6) == bytes.fromhex("8b8f4c010000"), "0x25c634/0x25c656")
    add("startTrack stores index +0x40 before open", image.read(0x25C707, 3) == bytes.fromhex("897740"), "mov [rdi+0x40],esi @0x25c707")
    add("startTrack sets +0x10 active after open", image.read(0x25C82E, 4) == bytes.fromhex("c6471001"), "mov byte [rdi+0x10],1 @0x25c82e")
    add("event object is runtime-only (.data bss)", not image.file_backed(ev) and image.section_of(ev)[0] == ".data", f"{ev:#x}")

    tracks = None
    if index_bytes is not None:
        def count(path):
            k = struct.pack("<Q", archive_key(path))
            n, j = 0, index_bytes.find(k)
            while j != -1 and n < 2:
                n += 1
                j = index_bytes.find(k, j + 1)
            return n
        missing = [t for t in FREESTYLE_TRACKS if count(audio_path("eventmusic", t)) != 1]
        dense = sum(1 for t in range(4300, 4700) if count(audio_path("eventmusic", t)) == 1)
        jukebox = sum(1 for s in range(1, 72) if count(audio_path("jukeboxmusic", s)) == 1)
        add("58 WPF eventmusic paths each indexed once", not missing, f"missing {missing}")
        add("71 jukeboxmusic redirect targets indexed", jukebox == 71, f"{jukebox}/71")
        tracks = {"freestyleListed": len(FREESTYLE_TRACKS), "missing": missing,
                  "eventmusicPresent4300to4699": dense, "jukeboxmusicSlotsPresent": jukebox,
                  "membershipNeedsLiveProof": dense > len(FREESTYLE_TRACKS)}
    return checks, tracks


def check_covers(game):
    rows, ranges = [], {}
    with open(game / "manifest", "rb") as f:
        for raw in f:
            parts = raw.decode("ascii", "replace").strip().split(",")
            if len(parts) == 4 and COVER_LOGICAL.match(parts[0]):
                rows.append((parts[0], parts[1], int(parts[2]), int(parts[3])))
    same, stored, other, covers, archives = 0, [], [], [], set()
    for logical, archive, offset, size in rows:
        with open(game / archive, "rb") as a:
            a.seek(offset)
            head = a.read(32)
        if head[:28] == COVER_HEADER:
            same += 1
            stored.append(struct.unpack_from("<I", head, 28)[0])
            covers.append(COVER_LOGICAL.match(logical).group(2))
            archives.add(archive)
        else:
            other.append(logical)
    leaf = {}
    for logical, *_ in rows:
        name = COVER_LOGICAL.match(logical).group(2)
        leaf[name] = leaf.get(name, 0) + 1
    return {
        "numberedCoverRows": len(rows),
        "uniqueNames": len(leaf),
        "duplicateNames": sorted(n for n, c in leaf.items() if c > 1)[:20],
        "header256Bc1Matches": same,
        "header256Bc1Archives": sorted(archives),
        "header256Bc1FirstLast": [min(covers), max(covers)] if covers else None,
        "otherHeaderCount": len(other),
        "storedSizeMin": min(stored) if stored else None,
        "storedSizeMax": max(stored) if stored else None,
        "storedEqualsRaw32768": sum(1 for s in stored if s == 32768),
        "contentAddressed": "shared/<first byte>/<name>.<16hex>.tld; identical content shares a hash",
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--game", default=DEFAULT_GAME)
    ap.add_argument("--skip-hash", action="store_true", help="skip the full SHA-256 of NBA2K27.exe")
    ap.add_argument("--json", help="write the report here as well")
    a = ap.parse_args(argv)
    game = pathlib.Path(a.game)
    with open(game / "NBA2K27.exe", "rb") as f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ) as m:
        image = Image(m)
        report = {"schema": "nba2k27-v2-jukebox-freestyle-cover-audit-v1",
                  "stamp": f"{image.stamp:08X}", "imageSize": f"{image.image_size:08X}",
                  "gameProcessAccess": False, "gameFilesChanged": False}
        if image.stamp != EXPECTED_STAMP or image.image_size != EXPECTED_IMAGE_SIZE:
            report["error"] = "not the pinned Version 2 image"
            print(json.dumps(report, indent=1))
            return 2
        if not a.skip_hash:
            h = hashlib.sha256()
            for i in range(0, len(m), 1 << 24):
                h.update(m[i : i + (1 << 24)])
            report["sha256"] = h.hexdigest().upper()
            if report["sha256"] != EXPECTED_SHA256:
                report["error"] = "SHA-256 mismatch"
                print(json.dumps(report, indent=1))
                return 2
        with open(game / "0A", "rb") as ix, mmap.mmap(ix.fileno(), 0, access=mmap.ACCESS_READ) as index:
            checks, tracks = check_freestyle(image, index)
        anchor = image.read(V2["anchor"], 7)
    report["freestyle"] = {
        "checks": checks,
        "tracks": tracks,
        "proposedProfile": {"timestamp": f"{EXPECTED_STAMP:#x}", "imageSize": f"{EXPECTED_IMAGE_SIZE:#x}",
                            "anchor": f"{V2['anchor']:#x}", "event": f"{V2['event']:#x}",
                            "keys": f"{V2['keys']:#x}", "bytes": anchor.hex()},
        "oldProfiles": {k: {n: f"{v:#x}" for n, v in p.items()} for k, p in OLD_PROFILES.items()},
    }
    report["covers"] = check_covers(game)
    ok = all(c["ok"] for c in checks) and report["covers"]["header256Bc1Matches"] > 0
    report["result"] = "PASS" if ok else "FAIL"
    text = json.dumps(report, indent=1)
    print(text)
    if a.json:
        pathlib.Path(a.json).write_text(text + "\n", encoding="utf-8")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
