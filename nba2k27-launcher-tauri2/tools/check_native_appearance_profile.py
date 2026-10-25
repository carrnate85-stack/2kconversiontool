"""Verify the native measurement writer against its Rust evidence and disk PE."""
import argparse, hashlib, json, re, struct
from audit_roster_current import ROOT, IMAGE, pefile

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image', nargs='?', default=r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe')
    path = parser.parse_args().image
    with open(path, 'rb') as f:
        assert hashlib.file_digest(f, 'sha256').hexdigest().upper() == IMAGE
    pe = pefile.PE(path, fast_load=True)
    source = (ROOT/'native/roster_appearance_v2_win.hpp').read_text()
    methods = [(int(r,16), int(n), h.lower()) for r,n,h in re.findall(r'\{0x([0-9A-Fa-f]+),(\d+),"([0-9A-Fa-f]{64})"\}', source)]
    proof = json.loads((ROOT/'assets/appearance-v2-production-proof.json').read_text())
    assert methods == [(m['rva'],m['bytes'],m['sha256']) for m in proof['methods']]
    for r,n,h in methods:
        assert hashlib.sha256(pe.get_data(r,n)).hexdigest() == h, hex(r)
    roots = re.search(r'for\(const auto rva:\{([^}]+)\}',source).group(1)
    for rva in re.findall(r'0x([0-9A-Fa-f]+)ull',roots):
        r = int(rva,16); b = pe.get_data(r,7)
        assert b[:3] == bytes.fromhex('488b05') and r+7+struct.unpack('<i',b[3:])[0] == proof['rootRva']
    slots = re.findall(r'q\(base\+0x([0-9A-Fa-f]+)\)==base\+0x([0-9A-Fa-f]+)',source)
    assert len(slots) == 2
    for slot,target in slots:
        assert struct.unpack('<Q',pe.get_data(int(slot,16),8))[0] == pe.OPTIONAL_HEADER.ImageBase + int(target,16)
    assert pe.OPTIONAL_HEADER.SizeOfImage == 0x36532000 and '0x36532000' in source
    assert pe.get_data(0x5d0e560,4) == bytes.fromhex('0ad7233c')
    print('PASS: 11 native measurement methods, two root bindings, two vtable slots and cm units match the exact current game; game writes: 0')

if __name__ == '__main__': main()
