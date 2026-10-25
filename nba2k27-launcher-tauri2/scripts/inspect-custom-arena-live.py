"""Read-only inspection of this dev session's arena fields and open IFF handles."""
import ctypes as c
import json
import os
from pathlib import Path

root = Path(__file__).resolve().parent.parent
k = c.WinDLL('kernel32', use_last_error=True)
nt = c.WinDLL('ntdll')
k.OpenProcess.argtypes = [c.c_ulong, c.c_int, c.c_ulong]
k.OpenProcess.restype = c.c_void_p
k.CloseHandle.argtypes = [c.c_void_p]
k.ReadProcessMemory.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_size_t, c.c_void_p]
k.CreateFileW.argtypes = [c.c_wchar_p, c.c_ulong, c.c_ulong, c.c_void_p, c.c_ulong, c.c_ulong, c.c_void_p]
k.CreateFileW.restype = c.c_void_p
k.ReadFile.argtypes = [c.c_void_p, c.c_void_p, c.c_ulong, c.c_void_p, c.c_void_p]
k.GetCurrentProcess.restype = c.c_void_p
k.DuplicateHandle.argtypes = [c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p, c.c_ulong, c.c_int, c.c_ulong]
k.GetFinalPathNameByHandleW.argtypes = [c.c_void_p, c.c_wchar_p, c.c_ulong, c.c_ulong]
nt.NtQuerySystemInformation.argtypes = [c.c_ulong, c.c_void_p, c.c_ulong, c.c_void_p]
nt.NtQuerySystemInformation.restype = c.c_long

def shared_json(path):
    handle = k.CreateFileW(str(path), 0x80000000, 7, None, 3, 0, None)
    assert handle not in (None, c.c_void_p(-1).value), c.get_last_error()
    try:
        buf = c.create_string_buffer(16384)
        done = c.c_ulong()
        assert k.ReadFile(handle, buf, len(buf), c.byref(done), None), c.get_last_error()
        return json.loads(buf.raw[:done.value])
    finally:
        k.CloseHandle(handle)

session = shared_json(root / 'Dev/Portable/Data/session-journal.json')
status = shared_json(root / 'Dev/Portable/Logs/native-sessions' / session['session'] / 'provider-state.json')
journal = shared_json(root / 'Dev/Portable/Data/blacktop-arena-session.json')
game = k.OpenProcess(0x1010, False, journal['game']['pid'])
assert game, c.get_last_error()
def read(address, size):
    buf = c.create_string_buffer(size)
    done = c.c_size_t()
    assert k.ReadProcessMemory(game, address, buf, size, c.byref(done)), c.get_last_error()
    assert done.value == size
    return buf.raw
try:
    fields = read(journal['record']['address'] + 0x7e, 78)
    result = {'gamePid': journal['game']['pid'], 'providerPid': status['workerPid'],
              'selection': status['arena'], 'liveArena': fields[:38].split(b'\0')[0].decode(),
              'liveFloor': fields[38:].split(b'\0')[0].decode(),
              'matchesReceipt': fields == bytes(journal['replacement'])}
finally:
    k.CloseHandle(game)

class Entry(c.Structure):
    _fields_ = [('obj', c.c_size_t), ('pid', c.c_size_t), ('handle', c.c_size_t),
                ('access', c.c_ulong), ('trace', c.c_ushort), ('kind', c.c_ushort),
                ('attributes', c.c_ulong), ('reserved', c.c_ulong)]

# A local ordinary file identifies the system's File object type. Only handles
# of that type in this session's provider are duplicated, queried, then closed.
probe = k.CreateFileW(str(root / 'Dev/Portable/Data/custom-blacktop-catalog.json'), 0x80000000, 7, None, 3, 0, None)
try:
    size = 1024 * 1024
    while True:
        buf = c.create_string_buffer(size)
        needed = c.c_ulong()
        code = nt.NtQuerySystemInformation(64, buf, size, c.byref(needed))
        if code >= 0:
            break
        assert code & 0xffffffff == 0xc0000004, hex(code & 0xffffffff)
        size = max(size * 2, needed.value + 65536)
        assert size <= 256 * 1024 * 1024
    count = c.c_size_t.from_buffer(buf).value
    entries = (Entry * count).from_buffer(buf, 2 * c.sizeof(c.c_size_t))
    kind = next(e.kind for e in entries if e.pid == os.getpid() and e.handle == probe)
    provider = k.OpenProcess(0x40, False, status['workerPid'])
    assert provider, c.get_last_error()
    matches, files, failures, iff_count = [], 0, 0, 0
    try:
        for entry in entries:
            if entry.pid != status['workerPid'] or entry.kind != kind:
                continue
            duplicate = c.c_void_p()
            if not k.DuplicateHandle(provider, entry.handle, k.GetCurrentProcess(), c.byref(duplicate), 0, False, 2):
                failures += 1
                continue
            try:
                name = c.create_unicode_buffer(32768)
                length = k.GetFinalPathNameByHandleW(duplicate, name, len(name), 0)
                if not length:
                    failures += 1
                    continue
                files += 1
                path = name.value
                iff_count += int(path.lower().endswith('.iff'))
                if '/levels/' in path.replace('\\', '/').lower() and path.lower().endswith('.iff'):
                    matches.append(path)
            finally:
                k.CloseHandle(duplicate)
    finally:
        k.CloseHandle(provider)
    result.update(providerOpenLevelFiles=sorted(set(matches)), providerFileHandles=files, providerOpenIffHandles=iff_count, handleQueryFailures=failures)
finally:
    k.CloseHandle(probe)
print(json.dumps(result, indent=2))
