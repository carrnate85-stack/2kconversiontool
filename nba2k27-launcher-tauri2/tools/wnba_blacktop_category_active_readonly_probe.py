"""Exact-lease QA065 category-open diagnostic: bounded reads, no game calls/writes."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
import pathlib
import re
import struct
from qa055_carry_readonly import ROOT, inspect, k
from wnba_priority_loaded_roster_probe import EXE_SHA, parse_lease


class Region(c.Structure):
    _fields_ = [('base', c.c_uint64), ('allocation', c.c_uint64),
                ('allocationProtect', w.DWORD), ('partition', w.DWORD),
                ('size', c.c_uint64), ('state', w.DWORD), ('protect', w.DWORD), ('type', w.DWORD)]


k.VirtualQueryEx.argtypes = [w.HANDLE, c.c_void_p, c.POINTER(Region), c.c_size_t]
k.VirtualQueryEx.restype = c.c_size_t


def ordinary(path):
    for item in (path, *path.parents):
        if item.stat().st_file_attributes & 0x400:
            raise ValueError('reparse path rejected')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=pathlib.Path, required=True)
    parser.add_argument('--label-array', action='store_true', help='Read only the native-reader-derived 647-record label array')
    args = parser.parse_args()
    out = args.out.resolve()
    if not out.is_relative_to((ROOT / 'artifacts').resolve()) or out.exists():
        raise ValueError('new artifact path required')
    leases = list((ROOT / 'Dev/Portable/Data').glob('blacktop-outfit-v2-*.lease'))
    if len(leases) != 1:
        raise ValueError('one exact lease required')
    lease = leases[0]
    ordinary(lease)
    if lease.stat().st_size != 1632:
        raise ValueError('lease size')
    lease_bytes = lease.read_bytes()
    pid, birth, image = parse_lease(lease, lease_bytes)
    nonce = lease.stem.removeprefix('blacktop-outfit-v2-')
    ordinary(pathlib.Path(image))
    sha = hashlib.sha256()
    with open(image, 'rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            sha.update(block)
    if sha.hexdigest() != EXE_SHA:
        raise ValueError('exact installed game SHA mismatch')
    handles = []
    try:
        scopes = []
        for p, b, s in ((8, 24, 96), (12, 32, 608), (16, 40, 1120)):
            process = struct.unpack_from('<I', lease_bytes, p)[0]
            born = struct.unpack_from('<Q', lease_bytes, b)[0]
            path = lease_bytes[s:s+512].decode('utf-16le').split('\0', 1)[0]
            h = k.OpenProcess(0x101410 if p == 8 else 0x101000, False, process)
            if not h:
                raise c.WinError(c.get_last_error())
            handles.append(h)
            scopes.append((h, born, path))

        def check():
            for h, born, path in scopes:
                times = [w.FILETIME() for _ in range(4)]
                observed_path, size = c.create_unicode_buffer(32768), w.DWORD(32768)
                if (not k.GetProcessTimes(h, *(c.byref(t) for t in times))
                    or (times[0].dwLowDateTime | times[0].dwHighDateTime << 32) != born
                    or k.WaitForSingleObject(h, 0) != 258
                    or not k.QueryFullProcessImageNameW(h, 0, observed_path, c.byref(size))
                    or observed_path.value.casefold() != path.casefold()):
                    raise ValueError('retained exact process identity changed')

        check()
        identity = inspect(pid, birth, image, True)
        if not identity.get('birthMatches') or not identity.get('pathMatches') or not identity.get('imageHeaderRead'):
            raise ValueError('module inventory identity failed')
        base = int(identity['imageBase'], 16)
        total = 0

        def read(at, size):
            nonlocal total
            if not 0 < size <= 65536 or at < 0x10000 or at + size >= 0x800000000000 or total + size > 200000:
                raise ValueError('fixed metadata read bound exceeded')
            check()
            buffer, got = c.create_string_buffer(size), c.c_size_t()
            if not k.ReadProcessMemory(handles[0], at, buffer, size, c.byref(got)) or got.value != size:
                raise c.WinError(c.get_last_error())
            total += size
            check()
            return buffer.raw

        def number(at, size=8):
            return int.from_bytes(read(at, size), 'little')

        def region(at, size, kind, protect=None, allocation=None):
            r = Region()
            if k.VirtualQueryEx(handles[0], at, c.byref(r), c.sizeof(r)) != c.sizeof(r):
                raise c.WinError(c.get_last_error())
            matches = (r.state == 0x1000 and r.type == kind and r.base <= at
                       and at + size <= r.base + r.size and not r.protect & 0x101
                       and (protect is None or r.protect == protect)
                       and (allocation is None or r.allocation == allocation))
            return matches, {'base': hex(r.base), 'allocation': hex(r.allocation), 'size': r.size,
                             'state': hex(r.state), 'protect': hex(r.protect), 'type': hex(r.type)}

        report = {'schema': 'wnba-blacktop-category-open-readonly-v1', 'pid': pid,
                  'gameWrites': 0, 'keyReads': 0, 'gameFunctionCalls': 0, 'checks': {}, 'mismatches': {}}
        checks, failures = report['checks'], report['mismatches']
        header = (ROOT / 'native/wnba_blacktop_category_win.hpp').read_text(encoding='utf-8-sig')
        methods = re.findall(r'\{(0x[0-9A-Fa-f]+),(\d+),"([0-9A-F]{64})"\}', header)
        if len(methods) != 7:
            raise ValueError('expected seven fixed profile methods')
        for rva, length, expected in methods:
            at, length = base + int(rva, 16), int(length)
            name = 'profile_' + rva
            valid, metadata = region(at, length, 0x1000000, 0x20, base)
            data = b''.join(read(at+i, min(65536, length-i)) for i in range(0, length, 65536))
            observed = hashlib.sha256(data).hexdigest().upper()
            checks[name] = valid and observed == expected
            if not checks[name]:
                failures[name] = {'region': metadata, 'observedSha256': observed, 'expectedSha256': expected}
        fixed = [('idGetter', 0x2DDA683, bytes.fromhex('0FB781200F0000'), 0x20),
                 ('nameGetter', 0x2DDA68E, bytes.fromhex('488D81FA020000'), 0x20),
                 ('stockUnwind', 0x85CDABC, bytes.fromhex('010704000732033002700160'), 2),
                 ('stockTable', 0x7425F4C, bytes.fromhex('CC5EEE FADA5EEE FAD35EEE FA585FEE FA2E5FEE FA375FEE FA'.replace(' ', '')), 2),
                 ('labelReady', 0xACFD610, b'\x01', 4)]
        for name, rva, expected, protection in fixed:
            valid, metadata = region(base+rva, len(expected), 0x1000000, protection, base)
            actual = read(base+rva, len(expected))
            if name == 'stockTable':
                dispatch=struct.unpack_from('<i',actual,12)[0]
                bridge=base+0x7425F4C+dispatch
                checks[name]=valid and actual[:12]+expected[12:16]+actual[16:]==expected
                report['dispatch']={'stock':actual==expected,'target':hex(bridge)}
                if actual!=expected:
                    good,meta=region(bridge,10,0x20000,0x20,bridge)
                    observed=read(bridge,10)
                    wanted=b'\xbe\x00\x00\x00\x20\xe9'+struct.pack('<i',base+0x230BE2B-(bridge+10))
                    checks['privateRxBridgeExact']=good and observed==wanted
                    if not checks['privateRxBridgeExact']:failures['privateRxBridgeExact']={'region':meta,'observedHex':observed.hex()}
            else:
                checks[name]=valid and actual==expected
            if not checks[name]:
                failures[name] = {'region': metadata, 'observedHex': actual.hex(), 'expectedHex': expected.hex()}
        # Resolve the pinned image's exception record without invoking RtlLookupFunctionEntry.
        with open(image, 'rb') as stream:
            dos = stream.read(64)
            pe = struct.unpack_from('<I', dos, 60)[0]
            stream.seek(pe)
            nt = stream.read(264)
            sections, optional_bytes = struct.unpack_from('<H', nt, 6)[0], struct.unpack_from('<H', nt, 20)[0]
            exception_rva, exception_size = struct.unpack_from('<II', nt, 24+112+3*8)
            stream.seek(pe+24+optional_bytes)
            section_headers = stream.read(sections*40)

            def offset(rva):
                for i in range(sections):
                    virtual_size, address, raw_size, raw = struct.unpack_from('<IIII', section_headers, i*40+8)
                    if address <= rva < address+max(virtual_size, raw_size):
                        return raw+rva-address
                raise ValueError('fixed exception RVA outside disk image sections')

            lo, hi, record_rva, record = 0, exception_size//12, None, None
            while lo < hi:
                mid = (lo+hi)//2
                stream.seek(offset(exception_rva+12*mid))
                candidate = stream.read(12)
                begin, end, unwind = struct.unpack('<III', candidate)
                if begin <= 0x230BE2B < end:
                    record_rva, record = exception_rva+12*mid, candidate
                    break
                if begin > 0x230BE2B:
                    hi = mid
                else:
                    lo = mid+1
            expected = struct.pack('<III', 0x230BDD0, 0x230BECB, 0x85CDABC)
            checks['stockRuntimeFunction'] = record == expected and record_rva is not None
            if record_rva is not None:
                actual = read(base+record_rva, 12)
                ok, metadata = region(base+record_rva, 12, 0x1000000, 0x80, base)
                checks['stockRuntimeFunction'] &= actual == expected and ok
                if not checks['stockRuntimeFunction']:
                    failures['stockRuntimeFunction'] = {'rva': hex(record_rva), 'observedHex': actual.hex(), 'expectedHex': expected.hex(), 'region': metadata}
        token = ROOT / f'Dev/Portable/Data/wnba-priority-v2/blacktop-categories-{nonce}.bin'
        ordinary(token)
        checks['categoryToken'] = token.stat().st_size == 72 and token.read_bytes() == struct.pack('<8sIIQ48s', b'C27WNBC2', pid, 0, birth, nonce.encode())
        pack = ROOT / 'Dev/Portable/Data/wnba-priority-v2/qa064-blacktop-source'
        ordinary(pack / '0A')
        index = (pack / '0A').read_bytes()
        checks['indexPin'] = len(index) == 144 and hashlib.sha256(index).hexdigest() == '0c25d0839b11b4af085f587c1baf4e74f821cd34d2b902fc950dbc91e5123cc9'
        if not checks['indexPin']:
            raise ValueError('fixed index differs')
        prefix = (str(pack) + '/').encode('ascii') + b'\0'

        def source():
            local = {}
            context = number(base+0xBB9A8E8+0x38)
            manager = number(context+0x100)
            mutex = number(manager+0x70)
            priority = number(manager+0x48)
            for name, ptr, size, writable in [('context', context, 0x108, False), ('manager', manager, 0x150, True), ('mutex', mutex, 0x40, True), ('priority', priority, 0x40, True)]:
                ok, meta = region(ptr, size, 0x20000)
                local[name+'Private'] = ok and (not writable or meta['protect'] in ('0x4', '0x8'))
            local['managerType'] = number(manager) == base+0x812C7C8
            local['mutexEnabled'] = number(mutex+0x3A, 1) == 1
            local['managerE0'] = number(manager+0xE0) != 0
            local['wadType'] = number(priority) == base+0x812D868
            local['wadBackptr'] = number(priority+8) == manager
            local['entryCount2'] = number(priority+0x30, 4) == 2
            local['fileCount1'] = number(priority+0x34, 4) == 1
            pointers = tuple(number(priority+n) for n in (0x10, 0x18, 0x20, 0x28))
            for name, ptr, size in zip(('raw', 'header', 'descriptor', 'prefix'), pointers, (48, 48, 48, len(prefix))):
                local[name+'Private'] = region(ptr, size, 0x20000)[0]
            raw, head, descriptor, path = (read(ptr, size) for ptr, size in zip(pointers, (48, 48, 48, len(prefix))))
            local['rawEqualsIndex'] = raw == index[96:144]
            local['headerEqualsIndex'] = head == index[:48]
            local['descriptorImmutable'] = descriptor[:8] == index[48:56] and descriptor[0x10:0x2c] == index[64:92]
            local['exactPrefixNul'] = path == prefix
            for name, ok in local.items():
                checks['source_'+name] = ok
                if not ok:
                    failures['source_'+name] = {'mismatch': True}
            return (context, manager, mutex, priority, pointers, raw, head, descriptor[:8]+descriptor[0x10:0x2c], path)

        first, second = source(), source()
        checks['sourceStableDoubleSample'] = first == second
        if args.label_array:
            # RVA3045970 is an unsigned-ID binary search over exactly 0x287
            # sixteen-byte records from ACFA610. RVA3045CE0 sorts that array
            # once, then sets ACFD610. Never adopt the initializer's old slot.
            start, count, stride = 0xACFA610, 0x287, 16
            flags = []
            for i in range(count):
                flags.append(region(base+start+i*stride, stride, 0x1000000, 4, base)[0])
            before_flag = number(base+0xACFD610, 1)
            data = read(base+start, count*stride)
            second_data = read(base+start, count*stride)
            after_flag = number(base+0xACFD610, 1)
            rows = tuple(struct.unpack_from('<IIQ', data, i*stride) for i in range(count))
            ids = [row[0] for row in rows]
            checks['labelArrayImageRw'] = all(flags)
            checks['labelArraySortedReady'] = before_flag == after_flag == 1
            checks['labelArrayStable'] = data == second_data
            checks['labelArrayIdsStrictlySorted'] = all(a < b for a, b in zip(ids, ids[1:]))
            hits = [i for i, row in enumerate(rows) if row[0] == 0xBA3C982D]
            lo, hi, located = 0, count-1, None
            while lo <= hi:
                mid = (lo+hi)//2
                if ids[mid] == 0xBA3C982D:
                    located = mid
                    break
                if ids[mid] > 0xBA3C982D:
                    hi = mid-1
                else:
                    lo = mid+1
            checks['labelArrayUniqueEuroRecord'] = len(hits) == 1 and located == hits[0] and rows[hits[0]][:2] == (0xBA3C982D, 0) and rows[hits[0]][2] in (0x6FC058B0442652CD,0x0F39A44C3426C962)
            report['labelArray'] = {'readerRva': '0x3045970', 'sortRva': '0x3045ce0',
                                   'startRva': hex(start), 'count': count, 'stride': stride,
                                   'matchCount': len(hits), 'womenLabel': bool(hits and rows[hits[0]][2] == 0x0F39A44C3426C962), 'binarySearchIndex': located,
                                   'euroRecordRva': hex(start+located*stride) if located is not None else None,
                                   'limit': 'Reader-derived search proof only; no production address adoption or write.'}
        def ctx():
            ix,alt=number(base+0xAD07BD0,4),number(base+0xAD07BCC,4)
            if ix>=128 or alt>=128:raise ValueError('context bounds')
            return {'index':ix,'alternate':alt,'mode':hex(number(base+0xAD07408+ix*64,4)),'other':hex(number(base+0xAD07408+alt*64,4)),'stage':number(base+0xAF46870+0xEA8,4)}
        report['contexts']=[ctx(),ctx()]
        checks['exactLeaseUnchanged'] = lease.read_bytes() == lease_bytes
        check()
        report['readBytes'] = total
        report['allChecksPass'] = all(checks.values())
        report['limit'] = 'External fixed-metadata proof cannot inspect C++ retained private authority/host fields or native calls.'
        with out.open('x', encoding='utf-8') as stream:
            json.dump(report, stream, indent=2)
            stream.flush()
            os.fsync(stream.fileno())
        print(json.dumps(report, indent=2))
    finally:
        for handle in handles:
            k.CloseHandle(handle)


if __name__ == '__main__':
    main()
