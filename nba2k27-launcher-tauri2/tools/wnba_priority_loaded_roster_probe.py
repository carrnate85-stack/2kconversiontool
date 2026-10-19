"""Explicit manual exact-lease read-only loaded Freestyle roster inventory.

Never calls game functions, reads key fields, signals events, or writes memory.
Only two DWORDs per team are sampled (ID's low16 bits; type's high6 bits).
Do not interpret this inventory as proof of gameplay or ciphertext admission.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
import pathlib
import re
import struct

EXE_SHA = '29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1'
TEAM_STRIDE = 0x1550
EXPECTED_CLUBS = set(range(300, 312)) | {315, 316, 317}


def parse_lease(path: pathlib.Path, data: bytes) -> tuple[int, int, str]:
    nonce = path.stem.removeprefix('blacktop-outfit-v2-')
    if (len(data) != 1632 or data[:8] != b'2KOV2L01'
            or not re.fullmatch('[0-9a-f]{48}', nonce)
            or data[48:96] != nonce.encode('ascii')):
        raise ValueError('lease ABI or filename/body nonce differs')
    pid = struct.unpack_from('<I', data, 8)[0]
    birth = struct.unpack_from('<Q', data, 24)[0]
    path_bytes = data[96:608]
    expected = path_bytes.decode('utf-16le').split('\0', 1)[0]
    if not pid or not birth or pathlib.PureWindowsPath(expected).name.casefold() != 'nba2k27.exe':
        raise ValueError('lease game identity is invalid')
    if not pathlib.PureWindowsPath(expected).is_absolute():
        raise ValueError('lease game image must have an absolute path')
    return pid, birth, expected


def capture_headers(read, base: int, blacktop: bool = False) -> tuple:
    modes = []
    for rva in (0xAD07BD0, 0xAD07BCC):
        index = read(base + rva, 4)
        if not 0 <= index < 128:
            raise ValueError('Freestyle mode index outside bounds')
        mode = read(base + 0xAD07408 + index * 64, 4)
        if mode != (0x92DF06DE if blacktop else 0xF54D5D12):
            raise ValueError('waiting for explicit Freestyle mode in both slots')
        modes.append((index, mode))
    if blacktop:
        tag = read(base + 0xAF46870, 8)
        stage = read(base + 0xAF46870 + 0xEA8, 4)
        owner = read(base + 0xAB5CE20, 8)
        if not 0x10000 <= owner < 0x800000000000 - 4:
            raise ValueError('Blacktop context owner outside bounds')
        active = read(base + 0xAB5D790, 4) & 255
        kind = read(owner, 4) & 255
        if tag != base + 0x791E158 or stage != 3 or active or kind:
            raise ValueError('waiting for inactive Blacktop player selection')
        setup = (tag, stage, owner, active, kind)
    else:
        if read(base + 0xB1F9050, 8) != base + 0x7B6B768:
            raise ValueError('waiting for Freestyle picker class')
        setup = tuple(read(base + 0xB1EA768 + n, 4) for n in (0, 12, 16, 20))
        if setup != (6, 1, 1, 4):
            raise ValueError('waiting for initialized Freestyle team picker')
    root = read(base + 0x9ACE338, 8)
    if not 0x10000 <= root < 0x800000000000 - 0xB0:
        raise ValueError('roster root pointer outside bounds')
    owner = read(root + 0xA8, 8)
    if not 0x10000 <= owner < 0x800000000000 - 0x90:
        raise ValueError('roster owner pointer outside bounds')
    count = read(owner + 0x80, 4)
    table = read(owner + 0x88, 8)
    if not 1 <= count <= 1024 or not 0x10000 <= table < 0x800000000000 - count * TEAM_STRIDE:
        raise ValueError('loaded team table outside bounded inventory range')
    return tuple(modes), setup, root, owner, count, table


def capture(read, base: int, blacktop: bool = False) -> tuple:
    before = capture_headers(read, base, blacktop)
    count, table = before[-2:]
    teams = tuple((read(table + i * TEAM_STRIDE + 0xF20, 4) & 0xffff,
                   read(table + i * TEAM_STRIDE + 0x123C, 4) >> 26)
                  for i in range(count))
    if capture_headers(read, base, blacktop) != before:
        raise ValueError('roster chain or Freestyle headers changed; sample discarded')
    return before, teams


def inventory(teams: tuple) -> dict:
    counts = Counter(kind for _, kind in teams)
    clubs = [team_id for team_id, kind in teams if kind == 29]
    duplicate_clubs = len(clubs) != len(set(clubs))
    exact_clubs = set(clubs) == EXPECTED_CLUBS and not duplicate_clubs
    return {'teams': len(teams), 'typeCounts': dict(sorted(counts.items())),
            'nbaTeams': counts[0], 'wnbaTeams': counts[29],
            'wnbaClubIds': sorted(set(clubs)), 'duplicateWnbaClubIds': duplicate_clubs,
            'matches30Nba15Wnba': counts[0] == 30 and counts[29] == 15 and exact_clubs,
            'expectedWnbaClubsMatch': exact_clubs}


def priority_headers(read, base: int) -> tuple:
    context = read(base + 0xBB9A8E8 + 0x38, 8)
    manager = read(context + 0x100, 8)
    manager_type = read(manager, 8) - base
    if manager_type != 0x812C7C8:
        raise ValueError('source manager does not match pinned WIN32 type')
    priority = read(manager + 0x48, 8)
    if not priority:
        return context, manager, manager_type, priority, 0, 0
    vtable = read(priority, 8) - base
    entries = read(priority + 0x30, 4) if vtable == 0x812D868 else None
    if entries is not None and entries > 10000000:
        raise ValueError('priority WAD entry count outside bounds')
    return context, manager, manager_type, priority, vtable, entries


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--out', type=pathlib.Path, required=True)
    p.add_argument('--priority-manager', action='store_true')
    p.add_argument('--blacktop', action='store_true', help='Require inactive Blacktop player selection instead of Freestyle')
    a = p.parse_args()
    # Importing this Windows helper configures read-only API signatures only.
    # Offline tests import the pure capture/lease/inventory functions above.
    import ctypes as c
    from ctypes import wintypes as w
    from qa055_carry_readonly import k, inspect, ROOT
    out = a.out.resolve()
    if not out.is_relative_to((ROOT / 'artifacts').resolve()) or out.exists():
        raise ValueError('output must be a new project artifact')
    leases = list((ROOT / 'Dev/Portable/Data').glob('blacktop-outfit-v2-*.lease'))
    if len(leases) != 1:
        raise ValueError('expected one exact current dev lease')
    lease = leases[0]
    lease_bytes = lease.read_bytes()
    pid, birth, expected = parse_lease(lease, lease_bytes)
    identity = inspect(pid, birth, expected, True)
    if (not identity.get('birthMatches') or not identity.get('pathMatches')
            or not identity.get('imageHeaderRead') or identity.get('aliveWait') != 258):
        raise ValueError('exact live game identity proof failed')
    digest = hashlib.sha256()
    with pathlib.Path(expected).open('rb') as image:
        for block in iter(lambda: image.read(1024 * 1024), b''):
            digest.update(block)
    if digest.hexdigest() != EXE_SHA:
        raise ValueError('disk image differs from pinned V2 executable')
    handle = k.OpenProcess(0x101010, False, pid)  # sync/query-limited/VM_READ
    if not handle:
        raise c.WinError(c.get_last_error())
    try:
        def check():
            times = [w.FILETIME() for _ in range(4)]
            if not k.GetProcessTimes(handle, *(c.byref(t) for t in times)):
                raise c.WinError(c.get_last_error())
            observed = times[0].dwLowDateTime | times[0].dwHighDateTime << 32
            path, size = c.create_unicode_buffer(32768), w.DWORD(32768)
            if (observed != birth or k.WaitForSingleObject(handle, 0) != 258
                    or not k.QueryFullProcessImageNameW(handle, 0, path, c.byref(size))
                    or path.value.casefold() != expected.casefold()):
                raise ValueError('retained game identity changed')
        total = 0
        def read(at, size):
            nonlocal total
            if (size not in (4, 8) or at < 0x10000 or at + size >= 0x800000000000
                    or total + size > 32768):
                raise ValueError('read outside fixed metadata/budget bounds')
            check()
            buf, got = c.create_string_buffer(size), c.c_size_t()
            if not k.ReadProcessMemory(handle, at, buf, size, c.byref(got)) or got.value != size:
                raise c.WinError(c.get_last_error())
            total += size
            check()
            return int.from_bytes(buf.raw, 'little')
        base = int(identity['imageBase'], 16)
        if read(base, 4) & 0xffff != 0x5A4D:
            raise ValueError('retained module MZ header differs')
        priority = priority_headers(read, base) if a.priority_manager else None
        first = capture(read, base, a.blacktop)
        second = capture(read, base, a.blacktop)
        if first != second:
            raise ValueError('team ID/type inventory changed; sample discarded')
        if priority is not None and priority_headers(read, base) != priority:
            raise ValueError('source priority chain changed; sample discarded')
        if lease.read_bytes() != lease_bytes:
            raise ValueError('exact dev lease changed; sample discarded')
        check()
        result = {'schema': 'wnba-loaded-roster-readonly-v1', 'gameWrites': 0,
                  'keyReads': 0, 'gameFunctionCalls': 0, 'stableDoubleSample': True,
                  'freestylePickerVerified': not a.blacktop, 'blacktopPickerVerified': a.blacktop, 'readBytes': total,
                  'inventory': inventory(first[1]),
                  'limit': 'Inventory only; no roster source, native admission or gameplay claim.'}
        if priority is not None:
            result['priority'] = {'present': bool(priority[3]),
                                  'vtableRva': hex(priority[4]) if priority[3] else None,
                                  'entryCount': priority[5]}
        with out.open('x', encoding='utf-8') as stream:
            json.dump(result, stream, indent=2)
        print(json.dumps(result, indent=2))
    finally:
        k.CloseHandle(handle)


if __name__ == '__main__':
    main()
