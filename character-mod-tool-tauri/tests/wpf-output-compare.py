"""Compare archive contents, ignoring ZIP timestamps and gzip wrapper metadata."""
import gzip
import hashlib
import json
import sys
import zipfile
from pathlib import Path

left, right = map(Path, sys.argv[1:3])
mappings = json.loads(Path(sys.argv[3]).read_text()) if len(sys.argv) > 3 else {"wpf": {}, "tauri": {}}

def rename(value, side):
    for original, common in mappings[side].items():
        value = value.replace(original, common)
    return value

def files(root, side):
    result = {rename(path.name, side): path for path in root.glob("*.iff")}
    if len(result) != len(list(root.glob("*.iff"))):
        raise ValueError("Asset ID normalization collided")
    return result

baseline, migrated = files(left, "wpf"), files(right, "tauri")
report = {"archives": len(baseline), "entries": 0, "different": [], "missing": sorted(baseline.keys() - migrated.keys()), "extra": sorted(migrated.keys() - baseline.keys()), "resolvedSharedTextures": []}

class ObjectPairs(list):
    """Preserve repeated game-document properties instead of dropping earlier values."""

def canonical(data, side, aliases=None):
    if data.startswith(b"\x1f\x8b\x08"):
        data = gzip.decompress(data)
    try:
        text = data.decode("utf-8-sig")
        try:
            value = json.loads(text, object_pairs_hook=ObjectPairs)
        except ValueError:
            value = json.loads("{" + text + "}", object_pairs_hook=ObjectPairs)
        def walk(item):
            if isinstance(item, str):
                return rename(item, side)
            if isinstance(item, ObjectPairs):
                keys = {}
                pairs = []
                for key, child in item:
                    normalized = rename(key, side)
                    if normalized in keys and keys[normalized] != key:
                        raise RuntimeError("Document key normalization collided")
                    keys[normalized] = key
                    if key == "Binary" and isinstance(child, str) and aliases:
                        child = aliases.get(child, child)
                    pairs.append([normalized, walk(child)])
                # Sort distinct keys but retain the order of repeated properties.
                return {"objectPairs": sorted(pairs, key=lambda pair: pair[0])}
            if isinstance(item, list):
                return [walk(child) for child in item]
            if isinstance(item, float) and item.is_integer():
                return int(item)
            return item
        return json.dumps(walk(value), sort_keys=True, separators=(",", ":")).encode()
    except (UnicodeError, ValueError):
        return data

for name in baseline.keys() & migrated.keys():
    with zipfile.ZipFile(baseline[name]) as a, zipfile.ZipFile(migrated[name]) as b:
        ae = {rename(entry, "wpf"): entry for entry in a.namelist()}
        be = {rename(entry, "tauri"): entry for entry in b.namelist()}
        if len(ae) != len(a.namelist()) or len(be) != len(b.namelist()):
            raise ValueError("Archive entry normalization collided")
        an, bn = set(ae), set(be)
        aliases = {}
        for side, entries in (("wpf", ae), ("tauri", be)):
            aliases[side] = {entry[:-4] + ".gz": entry for entry in entries if entry.endswith(".bin") and entry[:-4] + ".gz" not in entries}
        for entry in sorted(an ^ bn):
            side, archive, entries = ("wpf", a, ae) if entry in an else ("tauri", b, be)
            other = "tauri" if side == "wpf" else "wpf"
            expected = mappings.get("sharedDds", {}).get(other, {}).get(name, {}).get(entry)
            if entry.lower().endswith(".dds") and expected and hashlib.sha256(archive.read(entries[entry])).hexdigest() == expected:
                report["resolvedSharedTextures"].append({"archive": name, "entry": entry, "embeddedIn": side})
            else:
                report["different"].append({"archive": name, "entry": entry, "reason": "entry missing"})
        for entry in sorted(an & bn):
            report["entries"] += 1
            original_data, converted_data = a.read(ae[entry]), b.read(be[entry])
            for side, data in (("wpf", original_data), ("tauri", converted_data)):
                override = mappings.get("documents", {}).get(side, {}).get(name, {}).get(entry)
                if override is not None:
                    if not entry.lower().endswith(".txtr") or not data.startswith(b"\x91\xf8\x1f\xe0"):
                        raise ValueError("Only binary TXTR metadata can use a decoded override")
                    decoded = json.dumps(override).encode()
                    if side == "wpf":
                        original_data = decoded
                    else:
                        converted_data = decoded
            original, converted = canonical(original_data, "wpf", aliases["wpf"]), canonical(converted_data, "tauri", aliases["tauri"])
            if original != converted:
                report["different"].append({"archive": name, "entry": entry, "baselineSha256": hashlib.sha256(original).hexdigest(), "tauriSha256": hashlib.sha256(converted).hexdigest(), "baselineBytes": len(original), "tauriBytes": len(converted)})
print(json.dumps(report))
