#!/usr/bin/env python3
"""Validate preserved mixed-roster evidence without opening NBA 2K27.

This is intentionally an evidence check, not a runtime capability test.  It
validates the archived read-only Version 1 snapshot and the current Version 2
disk-only control-flow audit.  It must never be used to enable a writer.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


HERE = Path(__file__).resolve()
PROJECT = HERE.parents[1]
WORKSPACE = HERE.parents[2]

SNAPSHOT = WORKSPACE / "research_2k27" / "blacktop-session-17264-readonly.json"
V2_AUDIT = PROJECT / "artifacts" / "agent-wnba-v2-binary-audit.json"
WRAPPER_AUDIT = PROJECT / "artifacts" / "learn2k-v2-wrapper-audit-20261017.json"
WRAPPER_COPY = Path(r"C:\Users\carrn\Documents\Codex\2026-10-17\task\roster-scratch\learn2k_roster.iff")
GAME_ARCHIVE_0B = Path(r"C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\0B")

SNAPSHOT_SHA256 = "73753DA738BFC5250716B24EC1D482BA44A8E4B1309313440E81464E42A9550A"
V2_EXE_SHA256 = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
WRAPPER_SHA256 = "69445E272C0C02F6905A99BC0926E2D76E331AB7925F3621B799AC33FD6C28C2"
WRAPPER_OFFSET = 0x3BFD2276
WRAPPER_SIZE = 1_762_528
WRAPPER_HEADER = bytes.fromhex("1F 8B 21 84 00 00 00 00 00 FF 04 00 56 43 5A 00")

NBA_IDS = set(range(30)) - {7} | {31}
WNBA_IDS = set(range(300, 312)) | {315, 316, 317}
WNBA_NAMES = {
    300: "Aces",
    301: "Dream",
    302: "Fever",
    303: "Liberty",
    304: "Lynx",
    305: "Mercury",
    306: "Mystics",
    307: "Sky",
    308: "Sparks",
    309: "Storm",
    310: "Sun",
    311: "Wings",
    315: "Valkyries",
    316: "Tempo",
    317: "Fire",
}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def main() -> None:
    require(SNAPSHOT.is_file(), f"missing preserved snapshot: {SNAPSHOT}")
    require(V2_AUDIT.is_file(), f"missing Version 2 audit: {V2_AUDIT}")
    require(WRAPPER_AUDIT.is_file(), f"missing Version 2 wrapper audit: {WRAPPER_AUDIT}")
    require(sha256(SNAPSHOT) == SNAPSHOT_SHA256, "preserved snapshot hash changed")

    snapshot = json.loads(SNAPSHOT.read_text(encoding="utf-8"))
    require(snapshot.get("schema") == "blacktop-team-picker-readonly-v1", "unexpected snapshot schema")
    require(snapshot.get("readOnly") is True, "snapshot was not declared read-only")
    require(snapshot.get("writes") == 0, "snapshot recorded memory writes")
    require(snapshot.get("gameCalls") == 0, "snapshot recorded game calls")
    require(snapshot.get("teamCount") == 403, "preserved mixed generation is not 403 teams")

    rows = snapshot.get("teams", [])
    nba = {row.get("id"): row for row in rows if row.get("id") in NBA_IDS}
    wnba = {row.get("id"): row for row in rows if row.get("id") in WNBA_IDS}
    require(set(nba) == NBA_IDS, "preserved generation is missing a current NBA team row")
    require(set(wnba) == WNBA_IDS, "preserved generation is missing an expected WNBA row")
    require(all(row.get("type") == 0 for row in nba.values()), "current NBA rows changed type")
    require(all(row.get("type") == 29 for row in wnba.values()), "WNBA rows changed type")
    require(
        all(wnba[team_id].get("name") == name for team_id, name in WNBA_NAMES.items()),
        "WNBA names do not match the preserved mixed generation",
    )

    audit = json.loads(V2_AUDIT.read_text(encoding="utf-8"))
    require(audit.get("imageSha256") == V2_EXE_SHA256, "Version 2 executable identity changed")
    require(audit.get("gameProcessAccess") is False, "Version 2 audit accessed a game process")
    require(audit.get("gameFilesChanged") is False, "Version 2 audit changed game files")
    require(audit.get("candidateOnly") is True, "Version 2 audit was promoted beyond candidate evidence")
    require(audit.get("writesSupported") is False, "Version 2 audit unexpectedly supports writes")

    thunks = {item.get("filename"): item for item in audit.get("learn2kRosterThunks", [])}
    general = thunks.get("roster/learn2k_roster.iff")
    women = thunks.get("roster/learn2k_wnba_roster.iff")
    require(general is not None and women is not None, "Version 2 Learn2K thunks are incomplete")
    require(general.get("startRva") == 0x236FB30, "general Learn2K thunk moved")
    require(women.get("startRva") == 0x236FF70, "WNBA Learn2K thunk moved")
    require(
        any(owner.get("name") == ".?AVTKU_DELEGATE@@" and owner.get("entryOffset") == 0xD30
            for owner in general.get("rttiOwners", [])),
        "general Learn2K thunk is not owned by TKU_DELEGATE +0xD30",
    )
    require(
        any(owner.get("name") == ".?AVTKU_WNBA_DELEGATE@@" and owner.get("entryOffset") == 0xD30
            for owner in women.get("rttiOwners", [])),
        "WNBA Learn2K thunk is not owned by TKU_WNBA_DELEGATE +0xD30",
    )

    manifest = Path(audit["image"]).with_name("manifest")
    require(manifest.is_file(), f"missing current game manifest: {manifest}")
    manifest_text = manifest.read_text(encoding="utf-8", errors="replace").lower()
    require("roster/learn2k_roster.iff" not in manifest_text, "general Learn2K asset unexpectedly entered the manifest")
    require("roster/learn2k_wnba_roster.iff" not in manifest_text, "WNBA Learn2K asset unexpectedly entered the manifest")

    wrapper_audit = json.loads(WRAPPER_AUDIT.read_text(encoding="utf-8"))
    require(wrapper_audit.get("gameProcessAccess") is False, "wrapper audit accessed a game process")
    require(wrapper_audit.get("gameFilesChanged") is False, "wrapper audit changed game files")
    require(wrapper_audit.get("version2MixedContentProven") is False, "wrapper audit overstates Version 2 content proof")
    require(wrapper_audit.get("writerMayBeEnabled") is False, "wrapper audit unexpectedly enables a writer")
    require(WRAPPER_COPY.is_file(), f"missing verified Learn2K wrapper copy: {WRAPPER_COPY}")
    require(WRAPPER_COPY.stat().st_size == WRAPPER_SIZE, "Learn2K wrapper copy size changed")
    require(sha256(WRAPPER_COPY) == WRAPPER_SHA256, "Learn2K wrapper copy hash changed")
    with WRAPPER_COPY.open("rb") as stream:
        require(stream.read(16) == WRAPPER_HEADER, "Learn2K wrapper header changed")
    require(GAME_ARCHIVE_0B.is_file(), f"missing stock archive: {GAME_ARCHIVE_0B}")
    with GAME_ARCHIVE_0B.open("rb") as stream:
        stream.seek(WRAPPER_OFFSET)
        archive_range = stream.read(WRAPPER_SIZE)
    require(len(archive_range) == WRAPPER_SIZE, "stock 0B archive range is truncated")
    require(hashlib.sha256(archive_range).hexdigest().upper() == WRAPPER_SHA256,
            "stock 0B archive range no longer matches the verified wrapper")

    print(json.dumps({
        "ok": True,
        "scope": "offline evidence only; no runtime writer proof",
        "preservedSnapshot": str(SNAPSHOT),
        "preservedSnapshotSha256": SNAPSHOT_SHA256,
        "preservedMixedTeams": snapshot["teamCount"],
        "preservedNbaRows": len(nba),
        "preservedWnbaRows": len(wnba),
        "version2ImageSha256": V2_EXE_SHA256,
        "version2GeneralLearn2kThunkRva": hex(general["startRva"]),
        "version2WnbaLearn2kThunkRva": hex(women["startRva"]),
        "version2ManifestContainsLearn2kRoster": False,
        "version2Learn2kArchive": "0B",
        "version2Learn2kArchiveOffset": hex(WRAPPER_OFFSET),
        "version2Learn2kPackedSize": WRAPPER_SIZE,
        "version2Learn2kWrapperSha256": WRAPPER_SHA256,
        "version2ArchiveRangeMatchesWrapper": True,
        "version2WrapperDecoded": False,
        "version2MixedContentProven": False,
        "writerMayBeEnabled": False,
    }, indent=2))


if __name__ == "__main__":
    main()
