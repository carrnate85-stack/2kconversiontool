"""Bounded exact-lease read-only inspection of current V2 source-manager metadata.
Never reads the wrapper key fields, calls game functions, or writes game memory.
"""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import json
import pathlib
import struct
from qa055_carry_readonly import k, inspect, ROOT


def main():
    leases = list((ROOT / 'Dev/Portable/Data').glob('blacktop-outfit-v2-*.lease'))
    if len(leases) != 1:
        raise ValueError('Expected one current exact lease')
    data = leases[0].read_bytes()
    if len(data) != 1632 or data[:8] != b'2KOV2L01':
        raise ValueError('Unexpected lease ABI')
    pid, birth = struct.unpack_from('<I', data, 8)[0], struct.unpack_from('<Q', data, 24)[0]
    expected = data[96:608].decode('utf-16le').split('\0', 1)[0]
    identity = inspect(pid, birth, expected, True)
    if not identity.get('birthMatches') or not identity.get('pathMatches') or not identity.get('imageHeaderRead'):
        raise ValueError('Exact current-game identity proof failed')
    if hashlib.sha256(pathlib.Path(expected).read_bytes()).hexdigest() != '29a96471c55909fc2bcea6c699c59238656a501c19b50c74ed64b0d842e576a1':
        raise ValueError('Disk image is not pinned V2')
    handle = k.OpenProcess(0x101010, False, pid)  # synchronize/query-limited/read only
    if not handle:
        raise c.WinError(c.get_last_error())
    try:
        def check():
            times = [w.FILETIME() for _ in range(4)]
            if not k.GetProcessTimes(handle, *(c.byref(t) for t in times)):
                raise c.WinError(c.get_last_error())
            observed = times[0].dwLowDateTime | times[0].dwHighDateTime << 32
            path, count = c.create_unicode_buffer(32768), w.DWORD(32768)
            if observed != birth or k.WaitForSingleObject(handle, 0) != 258 or not k.QueryFullProcessImageNameW(handle, 0, path, c.byref(count)) or path.value.casefold() != expected.casefold():
                raise ValueError('Retained game identity changed')
        total = 0
        def read(at, count):
            nonlocal total
            if at < 65536 or at+count >= 0x800000000000 or count not in (4, 8):
                raise ValueError('Read outside fixed metadata bounds')
            check()
            buf, got = c.create_string_buffer(count), c.c_size_t()
            if not k.ReadProcessMemory(handle, at, buf, count, c.byref(got)) or got.value != count:
                raise c.WinError(c.get_last_error())
            total += count
            check()
            return int.from_bytes(buf.raw, 'little')
        base = int(identity['imageBase'], 16)
        # Inline manager object, as in wnba_roster_v2::capture_key (not a
        # singleton pointer; its first qword is the vtable pointer).
        wrapper = base+0xBB9A8E8
        context = read(wrapper+0x38, 8)
        manager = read(context+0x100, 8)
        if manager < 65536 or manager >= 0x800000000000:
            result = {'schema': 'wnba-fastload-current-readonly-v1', 'pid': pid,
                      'gameWrites': 0, 'keyReads': 0, 'readBytes': total,
                      'wrapperTypePointerRva': hex(read(wrapper,8)-base),
                      'contextTypePointerRva': hex(read(context,8)-base),
                      'managerFieldValue': hex(manager), 'backendFieldsRead': False,
                      'limit': 'Context+0x100 is not a readable object pointer. No source manager assumed or dereferenced.'}
            out = ROOT / 'artifacts/qa057-wnba-fastload-current-readonly-final-20261018.json'
            with out.open('x', encoding='utf-8') as f:
                json.dump(result, f, indent=2)
            print(json.dumps(result, indent=2))
            return
        table = read(manager, 8)-base
        if table != 0x812C7C8:
            raise ValueError(f'Unexpected VCFAST_LOAD_WIN32 type RVA {table:x}; no backend fields sampled')
        backends = []
        known = {0x812DDB8: 'development-files', 0x812C7C8: 'fast-load-win32'}
        for offset in (0x48, 0x50, 0x58, 0x60, 0x68):
            pointer = read(manager+offset, 8)
            item = {'slot': hex(offset), 'present': bool(pointer)}
            if pointer:
                vtable = read(pointer, 8)-base
                item.update(vtableRva=hex(vtable), knownType=known.get(vtable, 'unresolved'))
                if vtable == 0x812DDB8:
                    item['entryCount'] = read(pointer+0x28, 4)
                    item['ownedEntryArrayPresent'] = bool(read(pointer+0x10, 8))
                    item['nameBufferPresent'] = bool(read(pointer+0x18, 8))
            backends.append(item)
        if read(wrapper+0x38,8) != context or read(context+0x100,8) != manager or read(manager,8)-base != table:
            raise ValueError('Source-manager chain changed; discard sample')
        result = {'schema': 'wnba-fastload-current-readonly-v1', 'pid': pid,
                  'gameWrites': 0, 'keyReads': 0, 'readBytes': total,
                  'managerVtableRva': hex(table), 'backends': backends,
                  'limit': 'Metadata existence only; no manifest injection, runtime route or ownership transfer proven.'}
        out = ROOT / 'artifacts/qa057-wnba-fastload-current-readonly-final-20261018.json'
        with out.open('x', encoding='utf-8') as f:
            json.dump(result, f, indent=2)
        print(json.dumps(result, indent=2))
    finally:
        k.CloseHandle(handle)


if __name__ == '__main__':
    main()
