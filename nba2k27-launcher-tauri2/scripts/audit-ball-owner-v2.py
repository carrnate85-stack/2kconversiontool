"""Bounded exact-Version-2 disk proof of gameplay ball ownership. No process API."""
from pathlib import Path
import argparse, bisect, hashlib, json, re, runpy, struct

ROOT = Path(__file__).resolve().parents[1]
A = runpy.run_path(str(ROOT / "scripts/audit-roster-v2-fields.py"))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("image", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    with args.image.open("rb") as source:
        assert hashlib.file_digest(source, "sha256").hexdigest().upper() == A["IMAGE_SHA"]
    pe = A["pefile"].PE(str(args.image), fast_load=True)
    md = A["Cs"](A["CS_ARCH_X86"], A["CS_MODE_64"])
    md.detail = True
    pdata = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".pdata")
    functions = sorted((s, e) for s, e, _ in struct.iter_unpack("<III", pdata.get_data()[:pdata.SizeOfRawData // 12 * 12]) if 0x1000 <= s < e < 0x6500000)
    starts = [s for s, _ in functions]
    text = next(s for s in pe.sections if s.Name.rstrip(b"\0") == b".text")
    text_data = text.get_data()

    def function_for(rva):
        # These leaf functions have no unwind entry. Their fixed, inspected
        # instruction boundaries are explicit; never round an arbitrary hit.
        for start, end in [(0x4140C30, 0x4140C38), (0x417AB70, 0x417AB7F), (0x177B7A0, 0x177B7A8),
                           (0x3C96870, 0x3C968AC), (0x3C968B0, 0x3C9693D), (0x3C969B0, 0x3C969F6),
                           (0x11F7CB0, 0x11F7CCC), (0x255EA90, 0x255EA9C)]:
            if start <= rva < end:
                return start, end
        index = bisect.bisect_right(starts, rva) - 1
        assert index >= 0 and rva < functions[index][1], f"Not in an unwind function: {rva:08X}"
        return functions[index]

    def instructions(rva, before=180, after=220):
        start, end = function_for(rva)
        assert end - start <= 131072
        ins = list(md.disasm(pe.get_data(start, end - start), start))
        rows = []
        for item in ins:
            if not max(start, rva - before) <= item.address < min(end, rva + after):
                continue
            row = {"rva": f"{item.address:08X}", "bytes": item.bytes.hex(), "asm": f"{item.mnemonic} {item.op_str}"}
            for operand in item.operands:
                if operand.type == A["X86_OP_MEM"] and operand.mem.base == A["X86_REG_RIP"]:
                    target = item.address + item.size + operand.mem.disp
                    row["ripTarget"] = f"{target:08X}"
                    try:
                        raw = pe.get_data(target, 240)
                        ascii_text = raw.split(b"\0")[0]
                        if len(ascii_text) >= 5 and all(32 <= n < 127 for n in ascii_text):
                            row["ascii"] = ascii_text.decode("ascii")
                    except Exception:
                        pass
            rows.append(row)
        return {"functionStart": f"{start:08X}", "functionEnd": f"{end:08X}", "anchor": f"{rva:08X}", "instructions": rows}

    targets = {
        "typed-AI-BALL-constructor": 0x3C894A0,
        "typed-basketball-manager-constructor": 0x428A24B,
        "typed-InitBalls-model-assignment": 0x428A367,
        "GameData-model-to-setup74": 0x41400E4,
        "setup74-to-InitBalls": 0x44272CB,
        "gameplay-render-model-consumer": 0x417AC49,
        "main-world-pointer-getter": 0x4140C30,
        "world-manager-getter": 0x417AB70,
        "manager-iterator-init": 0x3C96870,
        "manager-iterator-first": 0x3C968B0,
        "manager-iterator-next": 0x3C969B0,
        "gameplay-model-preload": 0x2E95A4,
        "gameplay-model-release": 0x2E9932,
        "gamedata-from-358C": 0x11DEEE4,
        "gamedata-from-3590": 0x11DEF44,
        "358C-3590-source-root": 0x11F7CB0,
        "358C-3590-source-owner": 0x177B7A0,
        "gamedata-script-ball-selection": 0x11DDD33,
        "native-pending-ball-source-getter": 0x255EA90,
        "pending-ball-to-GameData-setter": 0x3D8850,
        "pending-ball-to-native-preload": 0x3D8874,
        "native-ball-GameData-setter": 0x26FA902,
    }
    proof = {name: instructions(rva) for name, rva in targets.items()}
    candidate_fields = {}
    for label, field in [("source-358C", 0x358C), ("source-3590", 0x3590), ("pending-ball-source-126C", 0x126C), ("AI-ball-model-34C-writers", 0x34C)]:
        found = []
        visited = set()
        for hit in re.finditer(re.escape(struct.pack("<I", field)), text_data):
            rva = text.VirtualAddress + hit.start()
            try:
                start, end = function_for(rva)
            except AssertionError:
                continue
            if end - start > 131072:
                continue
            if (start, rva) in visited:
                continue
            visited.add((start, rva))
            decoded = list(md.disasm(pe.get_data(start, min(end - start, 131072)), start))
            exact = [i for i in decoded if i.address <= rva < i.address + i.size and any(o.type == A["X86_OP_MEM"] and o.mem.disp == field for o in i.operands)]
            if not exact:
                continue
            if field == 0x34C and not any(i.operands and i.operands[0].type == A["X86_OP_MEM"] and i.operands[0].mem.disp == field and i.mnemonic.startswith(("mov", "vmov", "xor", "and", "or")) for i in exact):
                continue
            found.append(instructions(exact[0].address))
            assert len(found) <= 256, f"Candidate budget: {label}"
        candidate_fields[label] = found
    report = {"schema": "nba2k27-v2-gameplay-ball-owner-proof-v1", "imageSha256": A["IMAGE_SHA"], "gameProcessAccess": False, "gameFilesChanged": False,
              "provenance": proof, "candidateFields": candidate_fields,
              "iteratorSentinelOffsets": list(struct.unpack("<3Q", pe.get_data(0x7C81750, 24))),
              "writesAuthorized": False, "newWriteTargetEnabled": False}
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"output": str(args.output), "candidateCounts": {k: len(v) for k, v in candidate_fields.items()}, "gameProcessAccess": False}))


if __name__ == "__main__":
    main()
