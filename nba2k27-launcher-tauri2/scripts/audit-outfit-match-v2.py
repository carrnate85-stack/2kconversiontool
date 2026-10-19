"""Read installed PE bytes only; emit bounded Version 2 ownership evidence.

No process APIs, native function calls, files written, or live write authority.
Run with the configured research Python (pefile) and pass NBA2K27.exe.
"""
import hashlib
import json
import struct
import sys

sys.path.insert(0, r"C:\Users\carrn\CodexProjects\2kconversiontool\research\python_modules")
import pefile

EXPECTED = "29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1"
path = sys.argv[1]
with open(path, "rb") as stream:
    sha = hashlib.file_digest(stream, "sha256").hexdigest().upper()
assert sha == EXPECTED
pe = pefile.PE(path, fast_load=True)
assert pe.FILE_HEADER.TimeDateStamp == 0x6AC53765
assert pe.OPTIONAL_HEADER.SizeOfImage == 0x35BDE000
pdata = pe.OPTIONAL_HEADER.DATA_DIRECTORY[3]
functions = {
    begin: end
    for begin, end, _ in struct.iter_unpack("<III", pe.get_data(pdata.VirtualAddress, pdata.Size))
    if begin and end > begin
}

# Derived from matching native RTTI virtual tables and stock producer/consumer
# cross-references. Not inferred by shifting old RVAs.
methods = [
    ("description-clone", 0x730190, None),
    ("private-description-copy-constructor", 0x729950, None),
    ("private-description-destructor", 0x827130, None),
    ("owned-string-list-destructor", 0x82CB20, None),
    ("owned-string-list-initialize", 0x7B3430, None),
    ("owned-string-list-set-item", 0x7B3BF0, None),
    ("description-clothing-bind", 0x17CC50, 15),
    ("create-queue-drain", 0x5FEAA0, None),
    ("create-request-dispatch", 0x5FEBC0, None),
    ("create-request-destructor", 0x80A130, None),
    ("intrusive-request-detach", 0x610F30, 29),
    ("intrusive-request-insert", 0x610F50, None),
    ("manager-secondary-create", 0x6FA680, None),
    ("manager-primary-actor-getter", 0x80F260, 8),
    ("manager-primary-evaluation", 0x6FC1C0, None),
    ("stock-create-producer", 0x5FF090, None),
    ("stock-worker-clone-branch", 0x61EB17, 31),
    ("wrapper-manager-state-roster-setter", 0x7C5E00, 48),
    ("loaded-roster-copy-with-original-reference", 0x24F4DE0, None),
    ("managed-instance-node-create", 0x6FADB0, None),
    ("character-manager-constructor", 0x5F99B0, None),
    ("native-critical-section-initialize", 0x4B99360, None),
    ("is-main-thread", 0x88700, None),
    ("main-thread-identity-set", 0x84B60, None),
    ("player-object-size", 0x826170, 6),
]
rows = []
for name, rva, explicit in methods:
    count = explicit if explicit is not None else functions[rva] - rva
    assert 1 <= count <= 65536
    data = pe.get_data(rva, count)
    assert len(data) == count
    rows.append({"name": name, "rva": rva, "bytes": count,
                 "sha256": hashlib.sha256(data).hexdigest().upper(), "hex": data.hex()})

tables = []
for rva, slots in [(0x68920C8, [0, 8, 0xA8, 0xC8]),
                   (0x68923C8, [0, 8, 0xA8, 0xC8]),
                   (0x68A2498, [0, 8]),
                   (0x68603E8, [0, 8, 0x98]),
                   (0x68844D8, [0, 8, 0x98]),
                   (0x6884898, [0, 8, 0x150, 0x180]),
                   (0x762D278, [0, 8, 0x10]),
                   (0x6890D88, [0, 8, 0x10]),
                   (0x6884378, [0, 8, 0x10]),
                   (0x68840D8, [0, 8, 0x10])]:
    tables.append({"rva": rva, "slots": [
        {"offset": slot, "method": struct.unpack("<Q", pe.get_data(rva + slot, 8))[0] - 0x140000000}
        for slot in slots]})

print(json.dumps({
    "schema": "nba2k27-v2-match-outfit-native-ownership-research-v1",
    "sha256": sha, "timestamp": "6AC53765", "imageBytes": "35BDE000",
    "gameProcessAccess": False, "gameWritten": False,
    "workerEnabled": False, "liveVerificationRequired": True,
    "methods": rows, "virtualTables": tables,
    "globals": {"activeManagers": 0xA409E90, "createQueue": 0xA409FE8,
                "createSentinel": 0xA409FF0, "createLock": 0xA40A008, "heap": 0xA40A0B8,
                "mainThread": 0x8A1F720},
    "proofNotes": [
        "7FE78F independently passes A409E60 to the stock5F99B0 constructor.",
        "Constructor+188 create queue and+190 sentinel agree with stock5FF355 lockA40A008 and producer5FF389 sentinelA409FF0.",
        "Stock5FF0B9 allocates40-byte create request from heapA40A0B8;5FF107 allocation148 is the separate manager, not the request.",
        "Stock5FEBC0 transfers packet+18 complete manager to secondary interface+78; primary manager actor getter is+110.",
        "Wrapper setter7C5E00 proves+10 manager,+18 state,+20 boolean,+28 loaded roster; +20 is NOT a pointer.",
        "Roster copy24F4DE0 calls2CF5730 on the input record then stores that exact input at loaded roster+500.",
        "Do not activate oldC++worker or treat these hashes alone as full runtime ownership proof."
        ,"88700 compares the actual native GetCurrentThreadId thunk with8A1F720, set by84B60."
        ,"Character manager constructor passes queue+20 to native lock initializer4B99360 with DL=1; it stores+3A, proving queue initialized byte+5A."
    ],
    "remaining": ["New V2 worker implementation and isolated ownership/fault tests", "Live clone/queue/rebuild verification"]
}, indent=2))
