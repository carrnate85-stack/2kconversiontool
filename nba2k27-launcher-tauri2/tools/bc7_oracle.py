"""Developer-only BC7 oracle: decodes generated BC7 blocks with DirectXTex
texconv.exe (the converter WPF's roster artwork used) and writes
assets/bc7-oracle.json for the Rust decoder tests. No game data involved.

    python -I tools/bc7_oracle.py <texconv.exe>
"""
import json, os, random, struct, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MODES = {  # mode: (subsets, partition bits, rotation, index-select, color bits, alpha bits, endpoint pbit, shared pbit, index bits, index2 bits)
    0: (3, 4, 0, 0, 4, 0, 1, 0, 3, 0), 1: (2, 6, 0, 0, 6, 0, 0, 1, 3, 0), 2: (3, 6, 0, 0, 5, 0, 0, 0, 2, 0),
    3: (2, 6, 0, 0, 7, 0, 1, 0, 2, 0), 4: (1, 0, 2, 1, 5, 6, 0, 0, 2, 3), 5: (1, 0, 2, 0, 7, 8, 0, 0, 2, 2),
    6: (1, 0, 0, 0, 7, 7, 1, 0, 4, 0), 7: (2, 6, 0, 0, 5, 5, 1, 0, 2, 0)}

class Bits:
    def __init__(self): self.value, self.n = 0, 0
    def put(self, v, k):
        self.value |= (v & ((1 << k) - 1)) << self.n; self.n += k
    def block(self, rng):
        while self.n < 128: self.put(rng.getrandbits(1), 1)
        return self.value.to_bytes(16, "little")

def random_block(rng, mode):
    b = Bits(); b.put(1 << mode, mode + 1)
    while b.n < 128: b.put(rng.getrandbits(1), 1)
    return b.value.to_bytes(16, "little")

def probe(mode, partition, subsets):
    """All indices 0: each pixel shows endpoint 0 of its subset (subset k -> channel k max)."""
    ns, pb, rb, isb, cb, ab, epb, spb, ib, ib2 = MODES[mode]
    b = Bits(); b.put(1 << mode, mode + 1); b.put(partition, pb)
    top = (1 << cb) - 1
    for channel in range(3):
        for s in range(ns):
            b.put(top if (channel == s or (s == 0 and False)) else 0, cb)  # endpoint 0
            b.put(0, cb)                                                  # endpoint 1
    for _ in range(ns * 2 if epb else (ns if spb else 0)): b.put(0, 1)
    while b.n < 128: b.put(0, 1)
    return b.value.to_bytes(16, "little")

def dds(blocks, width):
    height = len(blocks) // (width // 4) * 4
    header = bytearray(148)
    struct.pack_into("<4sIIIIIII", header, 0, b"DDS ", 124, 0x1007 | 0x80000, height, width, len(blocks) * 16, 0, 1)
    struct.pack_into("<II4s", header, 76, 32, 4, b"DX10")
    struct.pack_into("<IIIII", header, 128, 98, 3, 0, 1, 0)
    return bytes(header) + b"".join(blocks), height

def main():
    texconv = sys.argv[1]
    rng = random.Random(0xBC7)
    blocks = []
    for mode in range(8):
        blocks += [random_block(rng, mode) for _ in range(160)]
    blocks += [bytes(16)] * 8  # reserved mode
    probes = [(m, p) for m in (0, 1, 2, 3, 7) for p in range(1 << MODES[m][1])]
    blocks += [probe(m, p, MODES[m][0]) for m, p in probes]
    while len(blocks) % 16: blocks.append(bytes(16))
    data, height = dds(blocks, 64)
    with tempfile.TemporaryDirectory() as folder:
        source = Path(folder) / "in.dds"; source.write_bytes(data)
        out = Path(folder) / "out"; out.mkdir()
        subprocess.run([texconv, "-nologo", "-y", "-ft", "dds", "-f", "R8G8B8A8_UNORM", "-m", "1", "-o", str(out), str(source)], check=True, capture_output=True)
        decoded = (out / "in.dds").read_bytes()
    offset = 128 + (20 if decoded[84:88] == b"DX10" else 0)
    pixels = decoded[offset:offset + 64 * height * 4]
    rows = []
    for i, block in enumerate(blocks):
        bx, by = (i % 16) * 4, (i // 16) * 4
        rgba = b"".join(pixels[((by + y) * 64 + bx) * 4:((by + y) * 64 + bx + 4) * 4] for y in range(4))
        rows.append([block.hex(), rgba.hex()])
    (ROOT / "assets/bc7-oracle.json").write_text(json.dumps({"schema": "nba2k27-bc7-oracle-v1", "decoder": "DirectXTex texconv", "blocks": rows}), encoding="utf8")
    print(json.dumps({"blocks": len(rows), "probes": len(probes)}))

if __name__ == "__main__":
    main()
