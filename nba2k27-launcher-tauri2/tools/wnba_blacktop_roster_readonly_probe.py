"""Exact-lease QA066 roster-callback-open diagnostic: bounded reads, no game calls/writes."""
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

        report = {'schema':'qa066-blacktop-roster-open-readonly-v1','pid':pid,'session':nonce,
                  'gameWrites':0,'keyReads':0,'gameFunctionCalls':0,'checks':{},'mismatches':{}}
        checks,failures=report['checks'],report['mismatches']
        header=(ROOT/'native/wnba_blacktop_roster_profile.hpp').read_text()
        methods=re.findall(r'\{(0x[0-9A-Fa-f]+|kGeneral|kStock),(\d+),"([0-9A-F]{64})"\}',header)
        if len(methods)!=9: raise ValueError('Expected exactly nine pinned methods')
        for rva,length,expected in methods:
            rva={'kGeneral':0x236FB30,'kStock':0x236E510}.get(rva,rva)
            rva=int(rva,16) if isinstance(rva,str) else rva
            length=int(length);valid,meta=region(base+rva,length,0x1000000,0x20,base)
            observed=hashlib.sha256(read(base+rva,length)).hexdigest().upper()
            key='method_'+hex(rva);checks[key]=valid and observed==expected
            if not checks[key]: failures[key]={'region':meta,'observedSha256':observed,'expectedSha256':expected}
        valid,meta=region(base+0x74E079A,26,0x1000000,2,base)
        asset=read(base+0x74E079A,26);checks['assetString']=valid and asset==b'roster/learn2k_roster.iff\0'
        if not checks['assetString']:failures['assetString']={'region':meta,'observedHex':asset.hex()}
        for name,rva,n,protection in [('manager',0xAB160D8,24,4),('installedDelegateSlot',0xAB160E8,8,4),('delegate',0x9B31BB0,8,None),('vtable',0x74CD158,0xD58,2)]:
            valid,meta=region(base+rva,n,0x1000000,protection,base)
            if name=='delegate':
                valid=valid and meta['protect'] in ('0x2','0x4','0x8')
                report['delegateRegion']=meta
            checks[name+'ImageProtection']=valid
            if not valid:failures[name+'ImageProtection']=meta
        d1,d2=number(base+0x9B31BB0),number(base+0x9B31BB0)
        checks['delegateExactVtable']=d1==d2==base+0x74CD158
        if not checks['delegateExactVtable']:failures['delegateExactVtable']={'firstRva':hex(d1-base),'secondRva':hex(d2-base)}
        installed1,installed2=number(base+0xAB160E8),number(base+0xAB160E8)
        report['installedDelegate']={'stable':installed1==installed2,'rva':hex(installed1-base),'isBlacktop':installed1==base+0x9B31BB0}
        report['delegateRequests']={}
        for name,rva in [('requested',0xAB16318),('playNow',0xAB3CE38)]:
            first,second=number(base+rva),number(base+rva)
            report['delegateRequests'][name]={'stable':first==second,'rva':hex(first-base) if base<=first<base+0x35BDE000 else None,'isBlacktop':first==base+0x9B31BB0}
        table1,table2=read(base+0x74CD158,0xD58),read(base+0x74CD158,0xD58)
        checks['vtableStable']=table1==table2
        values=struct.unpack('<'+'Q'*(len(table1)//8),table1)
        invalid=[{'slot':hex(i*8),'pointer':hex(x)} for i,x in enumerate(values) if not base<=x<base+0x35BDE000]
        checks['all427PointersInImage']=not invalid
        if invalid:failures['all427PointersInImage']=invalid
        checks['stockCallback']=values[0xD30//8]==base+0x236E510
        report['callbackRva']=hex(values[0xD30//8]-base)
        normalized=b''.join(struct.pack('<Q',(x-base)&0xffffffffffffffff) for x in values)
        expected='B786736A6D493C74B2083B78DA337CE8E8CC07661488A172AFFDC640BC72D983'
        actual=hashlib.sha256(normalized).hexdigest().upper();checks['vtableNormalizedHash']=actual==expected
        if actual!=expected:failures['vtableNormalizedHash']={'actual':actual,'expected':expected}
        report['neighborRvas']=[hex(x-base) for x in values[0xCF0//8:0xD58//8]]
        for prefix,magic in [('approval',b'C27WNBT1'),('blacktop-categories',b'C27WNBC2')]:
            token=ROOT/f'Dev/Portable/Data/wnba-priority-v2/{prefix}-{nonce}.bin';ordinary(token)
            checks[prefix+'Token']=token.stat().st_size==72 and token.read_bytes()==struct.pack('<8sIIQ48s',magic,pid,0,birth,nonce.encode())
        terminal=ROOT/f'Dev/Portable/Logs/game-session/wnba-source-test-{pid}.log';ordinary(terminal)
        text=terminal.read_text();checks['sourceRegisteredReceipt']=terminal.stat().st_size<=65536 and ('STATE=REGISTERED\n' in text) and f'PID={pid}\n' in text and f'SESSION={nonce}\n' in text and 'two-entry pack' in text
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
