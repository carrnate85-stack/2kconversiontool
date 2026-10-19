"""One bounded read-only inspection of the QA054 carry lease and exact processes.
No write-process, thread creation, signal-event, or game function APIs are used.
"""
import ctypes as c
from ctypes import wintypes as w
import json
import pathlib
import struct

ROOT = pathlib.Path(__file__).resolve().parents[1]
k = c.WinDLL('kernel32', use_last_error=True)
k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
k.OpenProcess.restype = w.HANDLE
k.CloseHandle.argtypes = [w.HANDLE]
k.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
k.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, c.POINTER(w.DWORD)]
k.ReadProcessMemory.argtypes = [w.HANDLE, c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
k.OpenEventW.argtypes = [w.DWORD, w.BOOL, w.LPCWSTR]
k.OpenEventW.restype = w.HANDLE
k.WaitForSingleObject.argtypes = [w.HANDLE, w.DWORD]


def inspect(pid, birth, expected, read_image=False):
    access = 0x100000 | 0x1000 | (0x10 if read_image else 0)
    handle = k.OpenProcess(access, False, pid)
    if not handle:
        return {'pid': pid, 'openError': c.get_last_error()}
    try:
        times = [w.FILETIME() for _ in range(4)]
        if not k.GetProcessTimes(handle, *(c.byref(t) for t in times)):
            raise c.WinError(c.get_last_error())
        observed = times[0].dwLowDateTime | times[0].dwHighDateTime << 32
        path, count = c.create_unicode_buffer(32768), w.DWORD(32768)
        if not k.QueryFullProcessImageNameW(handle, 0, path, c.byref(count)):
            raise c.WinError(c.get_last_error())
        result = {'pid': pid, 'birthMatches': observed == birth,
                  'pathMatches': path.value.casefold() == expected.casefold(),
                  'aliveWait': k.WaitForSingleObject(handle, 0), 'path': path.value}
        # Read just DOS header from a module base obtained through the exact PID.
        if read_image and result['birthMatches'] and result['pathMatches']:
            import subprocess
            # Module base is supplied by the OS process module inventory.
            text = subprocess.check_output(['powershell', '-NoProfile', '-Command',
                f'(Get-Process -Id {pid}).MainModule.BaseAddress.ToInt64()'], text=True)
            base = int(text.strip())
            data, got = c.create_string_buffer(64), c.c_size_t()
            ok = k.ReadProcessMemory(handle, base, data, 64, c.byref(got))
            result['imageHeaderRead'] = bool(ok and got.value == 64 and data.raw[:2] == b'MZ')
            result['imageBase'] = hex(base)
        return result
    finally:
        k.CloseHandle(handle)


def main():
    leases = list((ROOT / 'Dev/Portable/Data').glob('blacktop-outfit-v2-*.lease'))
    if len(leases) != 1:
        raise ValueError('Expected one exact current lease')
    data = leases[0].read_bytes()
    if len(data) != 1632:
        raise ValueError('Unexpected lease size')
    u32 = lambda at: struct.unpack_from('<I', data, at)[0]
    u64 = lambda at: struct.unpack_from('<Q', data, at)[0]
    path = lambda at: data[at:at+512].decode('utf-16le').split('\0', 1)[0]
    report = {'schema': 'qa055-carry-readonly-v1', 'gameWrites': 0,
              'processAccess': 'QUERY_LIMITED_INFORMATION|SYNCHRONIZE; game adds VM_READ',
              'processes': [inspect(u32(p), u64(b), path(s), p == 8)
                            for p, b, s in [(8,24,96), (12,32,608), (16,40,1120)]]}
    nonce = leases[0].stem.removeprefix('blacktop-outfit-v2-')
    report['events'] = {}
    for name in ['Cancel', 'Permit']:
        event = k.OpenEventW(0x100000, False, f'Local\\NBA2K27OutfitsV2-{nonce}-{name}')
        report['events'][name] = {'wait': k.WaitForSingleObject(event, 0)} if event else {'openError': c.get_last_error()}
        if event:
            k.CloseHandle(event)
    output = ROOT / 'artifacts/qa055-carry-readonly-20261018.json'
    with output.open('x', encoding='utf-8') as stream:
        json.dump(report, stream, indent=2)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
