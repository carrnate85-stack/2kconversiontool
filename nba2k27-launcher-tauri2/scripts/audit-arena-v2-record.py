"""Bounded installed-PE arena record proof. No process access or game writes."""
from pathlib import Path
import argparse, hashlib, json, runpy, struct

BASE = Path(__file__).resolve().parents[1]
analysis = runpy.run_path(str(BASE / "scripts/audit-roster-v2-fields.py"))
SHA = analysis["IMAGE_SHA"]
RANGES = [(0x2F6AA30, 8), (0x1CCA974, 0x7D), (0x11F07BF, 0x2F), (0x2F3D698, 0x3A), (0x2F3D7A4, 0x17), (0x2F1EE7C, 14)]
ANCHORS = [
    (0x2F6AA30, "0fb78192070000c3"),
    (0x1CCA974, "0fb7be92070000"),
    (0x1CCA983, "6689be92070000"),
    (0x1CCA999, "488d4e7e488dbea4000000"),
    (0x1CCA9B2, "41b8260000004531c9e8309f7200"),
    (0x1CCA9E5, "4889f941b8280000004531c9e8fa9e7200"),
    (0x11F07BF, "488d537e488d8e5001000041b826000000e85b089d03"),
    (0x11F07D5, "488d93a4000000488d8e7601000041b828000000e842089d03"),
    (0x2F1EE7C, "0fb7889207000039ce490f45c5"),
]
STRINGS = [(0x1CCA9AB, 0x7369224, "arena_701_int"), (0x1CCA9C0, 0x7369240, "arena_701_int_floor"),
           (0x1CCA9C9, 0x7369268, "arena_inseasontournament_int"), (0x1CCA9DE, 0x73692A2, "floor_summerleaguegeneric_int_floor")]

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("image",type=Path)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    with args.image.open("rb") as file:
        assert hashlib.file_digest(file,"sha256").hexdigest().upper()==SHA,"Exact Version 2 disk image required"
    pe=analysis["pefile"].PE(str(args.image),fast_load=True)
    assert pe.FILE_HEADER.TimeDateStamp==0x6AC53765 and pe.OPTIONAL_HEADER.SizeOfImage==0x35BDE000
    for rva,expected in ANCHORS:
        b=bytes.fromhex(expected)
        assert pe.get_data(rva,len(b))==b, f"Arena record anchor changed at {rva:08X}"
    strings=[]
    for site,target,value in STRINGS:
        instruction=pe.get_data(site,7)
        assert instruction[:3]==b"\x48\x8d\x15" and site+7+struct.unpack_from("<i",instruction,3)[0]==target
        expected=value.encode("utf-16le")+b"\0\0"
        assert pe.get_data(target,len(expected))==expected
        strings.append({"siteRva":f"{site:08X}","targetRva":f"{target:08X}","value":value,"encoding":"UTF-16 literal passed to formatting/copy routine"})
    md=analysis["Cs"](analysis["CS_ARCH_X86"],analysis["CS_MODE_64"])
    contexts=[]
    for rva,length in RANGES:
        data=pe.get_data(rva,length)
        contexts.append({"rva":f"{rva:08X}","bytes":length,"sha256":hashlib.sha256(data).hexdigest(),
                         "instructions":[f"{i.address:08X} {i.bytes.hex()} {i.mnemonic} {i.op_str}" for i in md.disasm(data,rva)]})
    report={"schema":"nba2k27-v2-arena-record-disk-proof-v1","imageSha256":SHA,"pass":True,"gameProcessAccess":False,"gameFilesChanged":False,
            "recordStride":"1278","id":{"offset":"792","bytes":2,"getterRva":"02F6AA30"},
            "file":{"offset":"7E","capacityBytes":38},"floor":{"offset":"A4","capacityBytes":40},
            "directory":"owner+60 count /owner+68 table independently indexed in native arena copy loop",
            "typedLiteralEvidence":"Same arena record preserves ID+792, sets +7E to arena filename with capacity26h and +A4 to floor filename with capacity28h.",
            "strings":strings,"contexts":contexts,"writesAuthorized":False,"liveValidated":False}
    args.output.write_text(json.dumps(report,indent=2)+"\n",encoding="utf8")
    print(json.dumps({"pass":True,"anchors":len(ANCHORS),"typedLiterals":len(strings),"gameProcessAccess":False,"gameFilesChanged":False,"output":str(args.output)}))

if __name__=="__main__":main()
