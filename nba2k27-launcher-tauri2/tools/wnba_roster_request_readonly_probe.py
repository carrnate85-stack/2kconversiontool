"""Exact-lease QA068 native roster-request-service diagnostic: bounded reads, no game calls/writes."""
import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import os
import pathlib
import re
import struct
import time
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
    parser.add_argument("--monitor-seconds", type=int, choices=(0,15), default=0, help="Optional bounded15-second natural-transition observation, samples every2seconds")
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

        profile=json.loads((ROOT/'artifacts/qa068-blacktop-source-unconsumed-research-20261018.json').read_text())
        report={'schema':'qa068-native-roster-request-readonly-v1','pid':pid,'session':nonce,
                'gameWrites':0,'gameCalls':0,'keyReads':0,'monitorSeconds':args.monitor_seconds,'samples':[]}
        methods=profile['methods']
        if len(methods)!=7:raise ValueError('Expected seven exact native request methods')
        expected_methods={
            0x236FB30:(26,'360DEB306FA92D1B7BA00C8E4057BE6F404742B84721B4506CBEC2C6F55820F6'),
            0x2F1E0B0:(536,'00FEEDC69CBCB51B65D70047C84AD4B43C50E5DBDFD0E7E641F8B1A6E6075B45'),
            0x2F1E360:(432,'B36DEF76D979613A797A0ECED5451AAFCC9203674C1A3044DC94A507DE8D46BF'),
        }
        # Complete hashes below are the disk-research output, pinned again as
        # constants so editing that report cannot authorize a new runtime read.
        REQUEST_METHODS = {36085088: (246, 'B783E4F813B797403593AA9824D0194082364610B7062C30B70CEBA9AB846EF8'), 36080848: (130, '145972587D0F4700B2B8C7C8CEF5D4D988280E04CFBD5950EE4E0895FF089D30'), 36078768: (157, 'BB2A8A27251CF200E94935364E34230A594AD8184C5ADE62A8B9895B3A00D35D'), 36077200: (206, 'B15C27B81D251AF1788D181B910AFCD4E0EBE33D77F4F293E757F69E2DE62ECA')}
        expected_methods.update(REQUEST_METHODS)
        if len(expected_methods)!=7:raise ValueError('Request profile count')
        def prove():
            for rva,(length,expected) in expected_methods.items():
                valid,metadata=region(base+rva,length,0x1000000,0x20,base)
                if not valid or hashlib.sha256(read(base+rva,length)).hexdigest().upper()!=expected:
                    raise ValueError('Exact request method or image region differs at '+hex(rva))
            for rva,target in [(0x74C2FD8+0x98,0x2267E90)]:
                if not region(base+rva,8,0x1000000,2,base)[0] or number(base+rva)!=base+target:
                    raise ValueError('Native service submit virtual target differs')
        def sample():
            prove()
            if not region(base+0xAB2F1F0,0x50,0x1000000,4,base)[0]:raise ValueError('Static service imageRW span differs')
            if number(base+0xAB2F1F0)!=base+0x74C2F28 or number(base+0xAB2F228)!=base+0x74C2FD8:
                raise ValueError('Static service exact two native classes differ')
            head=number(base+0xAB2F230);serial=number(base+0xAB2F1B0)
            item={'globalRequestUid':serial,'headPresent':bool(head),'typedRosterRequests':[],'unknownTypeEncountered':False,'boundedLimitReached':False}
            seen=set();pointer=head
            for index in range(8):
                if not pointer:break
                if pointer in seen:raise ValueError('Request list cycle')
                seen.add(pointer)
                if not region(pointer,0x160,0x20000,4)[0]:raise ValueError('Request complete privateRW span differs')
                tag=number(pointer)
                if tag!=base+0x74C3118:
                    item['unknownTypeEncountered']=True
                    break
                uid=number(pointer+0x38);next_pointer=number(pointer+0x20)
                filename=read(pointer+0x50,26);flag=number(pointer+0x150,1)
                known={name:filename.startswith(name.encode('ascii')+b'\0') for name in ('roster/learn2k_roster.iff','roster/roster.iff')}
                if number(pointer)!=tag or number(pointer+0x38)!=uid or number(pointer+0x20)!=next_pointer:
                    raise ValueError('Typed request changed during metadata sample')
                item['typedRosterRequests'].append({'uid':uid,'knownPathEquals':known,'loadBoolValid':flag in (0,1),'loadBool':flag if flag in (0,1) else None})
                pointer=next_pointer
            else:item['boundedLimitReached']=bool(pointer)
            if number(base+0xAB2F230)!=head or number(base+0xAB2F1B0)!=serial:
                raise ValueError('Request service changed during metadata sample')
            return item
        started=time.monotonic()
        for index in range(8 if args.monitor_seconds else 1):
            if index:
                delay=started+index*2-time.monotonic()
                if delay>0:time.sleep(delay)
            check()
            entry={'elapsedMs':int((time.monotonic()-started)*1000)}
            try:
                first,second=sample(),sample()
                entry['stable']=first==second
                entry['metadata']=first if entry['stable'] else {'reason':'Service changed between double samples'}
            except (ValueError,OSError) as error:
                entry['stable']=False;entry['reason']=str(error)
            report['samples'].append(entry)
            if lease.read_bytes()!=lease_bytes:raise ValueError('Exact lease changed')
        check()
        report['readBytes']=total
        report['exactLeaseUnchanged']=lease.read_bytes()==lease_bytes
        report['limit']='Completed requests may already be gone; an empty queue or global UID alone cannot prove callback invocation. Unknown request types are not traversed or decoded.'
        with out.open('x',encoding='utf-8') as stream:
            json.dump(report,stream,indent=2);stream.flush();os.fsync(stream.fileno())
        print(json.dumps(report,indent=2))
    finally:
        for handle in handles:k.CloseHandle(handle)

if __name__=='__main__':main()
