"""Compare bounded team metadata evidence in a running game; VM_READ only."""
import ctypes as c
from ctypes import wintypes as w
import hashlib, json, sys
from audit_roster_current import ROOT, pefile

def main():
    pid = int(sys.argv[1])
    k = c.WinDLL('kernel32', use_last_error=True)
    ps = c.WinDLL('psapi', use_last_error=True)
    k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]
    k.OpenProcess.restype = w.HANDLE
    k.CloseHandle.argtypes = [w.HANDLE]
    k.ReadProcessMemory.argtypes = [w.HANDLE, c.c_void_p, c.c_void_p, c.c_size_t, c.POINTER(c.c_size_t)]
    ps.EnumProcessModules.argtypes = [w.HANDLE, c.POINTER(c.c_void_p), w.DWORD, c.POINTER(w.DWORD)]
    k.QueryFullProcessImageNameW.argtypes = [w.HANDLE, w.DWORD, w.LPWSTR, c.POINTER(w.DWORD)]
    h = k.OpenProcess(0x0400 | 0x0010, False, pid)
    if not h: raise c.WinError(c.get_last_error())
    try:
        path = c.create_unicode_buffer(32768); size = w.DWORD(len(path))
        if not k.QueryFullProcessImageNameW(h, 0, path, c.byref(size)): raise c.WinError(c.get_last_error())
        assert path.value.lower().endswith('\\nba2k27.exe')
        modules = (c.c_void_p * 1024)(); needed = w.DWORD()
        if not ps.EnumProcessModules(h, modules, c.sizeof(modules), c.byref(needed)): raise c.WinError(c.get_last_error())
        base = modules[0]
        pe = pefile.PE(path.value, fast_load=True)
        pe.parse_data_directories(directories=[5])
        reloc = [e.rva for b in pe.DIRECTORY_ENTRY_BASERELOC for e in b.entries if e.type == 10]
        def read(rva, n):
            assert 0 < n <= 32768
            data = c.create_string_buffer(n); got = c.c_size_t()
            if not k.ReadProcessMemory(h, base + rva, data, n, c.byref(got)) or got.value != n: raise c.WinError(c.get_last_error())
            return data.raw
        rows=[]
        for span in json.loads((ROOT/'assets/roster-v2-team-metadata-proof.json').read_text())['spans']:
            rva,n=span['rva'],span['bytes']; disk=pe.get_data(rva,n); live=read(rva,n)
            diffs=[i for i,(a,b) in enumerate(zip(disk,live)) if a!=b]
            rows.append(dict(rva=hex(rva),bytes=n,matches=not diffs,diffs=[dict(offset=hex(i),disk=disk[i],live=live[i]) for i in diffs[:80]],relocations=[hex(x-rva) for x in reloc if rva<=x<rva+n],disk=disk.hex() if n<=432 else None,live=live.hex() if n<=432 else None))
        result=dict(pid=pid,base=hex(base),preferredBase=hex(pe.OPTIONAL_HEADER.ImageBase),gameWrites=0,rows=rows)
        (ROOT/'artifacts/roster-metadata-live-20261025.json').write_text(json.dumps(result,indent=2))
        print(json.dumps(result,indent=2))
    finally: k.CloseHandle(h)

if __name__=='__main__': main()
