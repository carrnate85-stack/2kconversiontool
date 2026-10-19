"""Exact-lease, read-only V2 debug-feature metadata probe. Never enables Debug.

No game function, memory-write, thread creation, codec, or signal-event API.
Only sampled entry IDs and the three allowlisted Boolean values are read.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import struct
from debug_menu_v2_disk_audit import ROOT, FEATURES, audit
from wnba_priority_loaded_roster_probe import parse_lease


def entry_valid(key, packed, value, expected):
    return key == expected and packed & 31 == 11 and packed >> 5 & 0x3FFFFFF == 1 and value in (0, 1)


def capture(read, span, base):
    def number(at, count=8):
        return int.from_bytes(read(at, count), 'little')

    def headers():
        default, override = number(base+0xA1F1248), number(base+0xA1F1250)
        selected = override or default
        if not selected:
            raise ValueError('Canonical feature manager has not initialized')
        kind = 'static-fallback' if selected == base+0xA1F1260 else 'registered-private'
        span(selected, 0x114, 'image' if kind == 'static-fallback' else 'private')
        table, count = number(selected+0xF8), number(selected+0x110, 4)
        if not 1 <= count <= 100000:
            raise ValueError('Feature pointer count outside native metadata bounds')
        span(table, count*8, 'private')
        return default, override, selected, kind, table, count

    before = headers()
    table, count = before[-2:]
    observations, traces = [], []
    for name, wanted, _initializer, obj, vtable in FEATURES:
        span(base+obj, 24, 'image')
        if number(base+obj) != base+vtable:
            raise ValueError('Exact registered feature-class vtable differs: '+name)
        path = []

        def metadata(index):
            pointer = number(table+8*index)
            span(pointer, 9, 'private')
            key = number(pointer, 4)
            path.append((index, pointer, key))
            return pointer, key

        lo, hi, lower, upper, found = 0, count, -1, 0x100000000, None
        for _ in range(32):
            if lo >= hi:
                break
            mid = (lo+hi)//2
            pointer, key = metadata(mid)
            if not lower < key < upper:
                raise ValueError('Unsigned binary-search path ordering changed')
            if key < wanted:
                lo, lower = mid+1, key
            elif key > wanted:
                hi, upper = mid, key
            else:
                found = mid, pointer
                break
        if found is None:
            raise ValueError('Allowlisted debug feature is not registered: '+name)
        mid, pointer = found
        for neighbor in (mid-1, mid+1):
            if 0 <= neighbor < count:
                _, key = metadata(neighbor)
                if key == wanted or (neighbor < mid and key > wanted) or (neighbor > mid and key < wanted):
                    raise ValueError('Allowlisted feature neighbor uniqueness/order differs')
        # Read packed metadata and value only after matching an allowed key.
        packed, value = number(pointer+4, 4), number(pointer+8, 1)
        if not entry_valid(wanted, packed, value, wanted):
            raise ValueError('Allowlisted debug feature type/count/value is invalid: '+name)
        observations.append((name, wanted, pointer, packed, value))
        traces.append(tuple(path))
    if headers() != before:
        raise ValueError('Canonical manager/table generation changed during sample')
    return before, tuple(observations), tuple(traces)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    if out.exists() or not out.is_relative_to((ROOT/'artifacts').resolve()):
        raise ValueError('new project artifact path required')
    report = dict(schema='debug-menu-v2-readonly-v1', gameWrites=0, gameCalls=0,
                  keyMaterialReads=0, productionEnabled=False, ok=False)
    handles = []
    try:
        import ctypes as c
        from ctypes import wintypes as w
        from qa055_carry_readonly import k, inspect
        from wnba_blacktop_category_readonly_probe import Region, ordinary
        leases = list((ROOT/'Dev/Portable/Data').glob('blacktop-outfit-v2-*.lease'))
        if len(leases) != 1:
            raise ValueError('One exact owned dev lease required; no process enumeration fallback')
        lease = leases[0]
        ordinary(lease)
        if lease.stat().st_size != 1632:
            raise ValueError('Unexpected exact lease size')
        lease_bytes = lease.read_bytes()
        pid, birth, expected = parse_lease(lease, lease_bytes)
        report['pid'] = pid
        ordinary(Path(expected))
        profile = audit(Path(expected))  # Full disk SHA and structure/RTTI/code proof.
        scopes = []
        expected_launcher = str(ROOT/'Dev/Portable/App/2k Launcher Rust Prototype.exe').casefold()
        for p, b, s in ((8, 24, 96), (12, 32, 608), (16, 40, 1120)):
            process = struct.unpack_from('<I', lease_bytes, p)[0]
            born = struct.unpack_from('<Q', lease_bytes, b)[0]
            path = lease_bytes[s:s+512].decode('utf-16le').split('\0', 1)[0]
            if p != 8 and path.casefold() != expected_launcher:
                raise ValueError('Lease is not bound to this exact portable dev launcher')
            handle = k.OpenProcess(0x101010 if p == 8 else 0x101000, False, process)
            if not handle:
                raise c.WinError(c.get_last_error())
            handles.append(handle)
            scopes.append((handle, born, path))

        def check():
            for handle, born, expected_path in scopes:
                times = [w.FILETIME() for _ in range(4)]
                path, size = c.create_unicode_buffer(32768), w.DWORD(32768)
                if (not k.GetProcessTimes(handle, *(c.byref(t) for t in times))
                    or (times[0].dwLowDateTime | times[0].dwHighDateTime << 32) != born
                    or k.WaitForSingleObject(handle, 0) != 258
                    or not k.QueryFullProcessImageNameW(handle, 0, path, c.byref(size))
                    or path.value.casefold() != expected_path.casefold()):
                    raise ValueError('Retained owned process PID/birth/path changed')

        check()
        identity = inspect(pid, birth, expected, True)
        if not identity.get('birthMatches') or not identity.get('pathMatches') or not identity.get('imageHeaderRead'):
            raise ValueError('Exact game image inventory failed')
        base, total = int(identity['imageBase'], 16), 0

        def read(at, size):
            nonlocal total
            if not 0 < size <= 65536 or total+size > 65536 or at < 0x10000 or at+size >= 0x800000000000:
                raise ValueError('64KB fixed code/metadata budget exceeded')
            check()
            buffer, got = c.create_string_buffer(size), c.c_size_t()
            if not k.ReadProcessMemory(handles[0], at, buffer, size, c.byref(got)) or got.value != size:
                raise c.WinError(c.get_last_error())
            total += size
            check()
            return buffer.raw

        def span(at, size, kind, executable=False):
            if at < 0x10000 or not 0 < size <= 800000 or at+size >= 0x800000000000:
                raise ValueError('Native object/table span outside fixed bounds')
            end, cursor, allocation = at+size, at, None
            for _ in range(256):
                region = Region()
                if k.VirtualQueryEx(handles[0], cursor, c.byref(region), c.sizeof(region)) != c.sizeof(region):
                    raise c.WinError(c.get_last_error())
                wanted_type = 0x1000000 if kind == 'image' else 0x20000
                allowed = (0x20,) if executable else (2, 4, 8)
                if (region.state != 0x1000 or region.type != wanted_type or region.protect not in allowed
                    or region.base > cursor or region.base+region.size <= cursor
                    or (kind == 'image' and region.allocation != base)
                    or (allocation is not None and region.allocation != allocation)):
                    raise ValueError('Native '+kind+' allocation/protection proof failed')
                allocation = region.allocation
                cursor = min(end, region.base+region.size)
                if cursor == end:
                    return
            raise ValueError('Native span region traversal exceeded bound')

        method_results = {}
        for method in profile['methods']:
            at, size = base+method['rva'], method['bytes']
            span(at, size, 'image', True)
            observed = hashlib.sha256(read(at, size)).hexdigest()
            method_results[method['name']] = observed == method['sha256']
            if not method_results[method['name']]:
                raise ValueError('In-memory complete method differs: '+method['name'])
        first, second = capture(read, span, base), capture(read, span, base)
        if first != second or lease.read_bytes() != lease_bytes:
            raise ValueError('Full feature metadata double sample or exact lease changed')
        check()
        report.update(ok=True, exactInstalledImageSha256=True, retainedProcessIdentities=True,
                      stableDoubleSample=True, readBytes=total, codeChecks=method_results,
                      canonicalManagerKind=first[0][3], usesOverride=bool(first[0][1]),
                      registeredFeatureCount=first[0][-1],
                      features=[dict(name=name, id=f'{key:08X}', exists=True, default=False,
                                     current=bool(value), booleanType=True, elementCount=1)
                                for name, key, _pointer, _packed, value in first[1]],
                      limit='Read-only metadata confirms mapping, not debug-menu behavior or safe writer admission. No toggle enabled.')
    except Exception as error:
        report['reason'] = str(error)
    finally:
        if handles:
            for handle in handles:
                k.CloseHandle(handle)
    with out.open('x', encoding='utf-8') as stream:
        json.dump(report, stream, indent=2)
        stream.flush()
        os.fsync(stream.fileno())
    print(json.dumps(report, indent=2))
    return 0 if report['ok'] else 2


if __name__ == '__main__':
    raise SystemExit(main())
