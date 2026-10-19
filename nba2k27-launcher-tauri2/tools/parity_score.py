"""Recompute the WPF -> Rust/Tauri parity score from artifacts/parity-status.json.

Usage:
  python tools/parity_score.py                 # summary + top 10 next items
  python tools/parity_score.py --table         # also print every feature
  python tools/parity_score.py --set outfit-carry=LIVE --set ball=LIVE --write
                                               # update statuses, save JSON, re-score
  python tools/parity_score.py --markdown      # scoring table as Markdown

Score: each feature is worth weight x status value (LIVE 1.0, UNV 0.7,
PART 0.4, DIS/MISS 0). Weighted % = sum(weight x value) / sum(weight).
Plain % = sum(value) / feature count. "Gain" is what a feature would add to
the weighted % if it reached LIVE.
"""
import argparse
import json
import sys
from pathlib import Path

DEFAULT = Path(__file__).resolve().parent.parent / "artifacts" / "parity-status.json"


def load(path):
    data = json.loads(path.read_text(encoding="utf-8"))
    values = data["statusValues"]
    for f in data["features"]:
        if f["status"] not in values:
            sys.exit(f"{f['id']}: unknown status {f['status']!r}")
        if not isinstance(f["weight"], (int, float)) or f["weight"] <= 0:
            sys.exit(f"{f['id']}: weight must be positive")
    return data


def score(data):
    values = data["statusValues"]
    feats = data["features"]
    total_w = sum(f["weight"] for f in feats)
    got_w = sum(f["weight"] * values[f["status"]] for f in feats)
    plain = sum(values[f["status"]] for f in feats) / len(feats)
    counts = {s: sum(1 for f in feats if f["status"] == s) for s in values}
    live_only = sum(f["weight"] for f in feats if f["status"] == "LIVE") / total_w
    areas = {}
    for f in feats:
        a = areas.setdefault(f["area"], [0.0, 0.0])
        a[0] += f["weight"] * values[f["status"]]
        a[1] += f["weight"]
    gains = sorted(
        (
            (f["weight"] * (1 - values[f["status"]]) / total_w * 100, f)
            for f in feats
            if f["status"] != "LIVE"
        ),
        key=lambda x: -x[0],
    )
    return {
        "weighted": got_w / total_w * 100,
        "plain": plain * 100,
        "liveWeighted": live_only * 100,
        "counts": counts,
        "areas": {k: v[0] / v[1] * 100 for k, v in areas.items()},
        "gains": gains,
        "n": len(feats),
        "totalWeight": total_w,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--file", type=Path, default=DEFAULT)
    ap.add_argument("--set", action="append", default=[], metavar="ID=STATUS")
    ap.add_argument("--write", action="store_true", help="save --set changes to the JSON")
    ap.add_argument("--build", help="record the port build (e.g. 0.50.0) when writing")
    ap.add_argument("--table", action="store_true")
    ap.add_argument("--markdown", action="store_true")
    ap.add_argument("--top", type=int, default=10)
    args = ap.parse_args()

    data = load(args.file)
    by_id = {f["id"]: f for f in data["features"]}
    for item in args.set:
        fid, _, status = item.partition("=")
        status = status.strip().upper()
        if fid not in by_id:
            sys.exit(f"unknown feature id {fid!r}")
        if status not in data["statusValues"]:
            sys.exit(f"unknown status {status!r}")
        print(f"{fid}: {by_id[fid]['status']} -> {status}")
        by_id[fid]["status"] = status
    if args.write:
        if args.build:
            data["portBuild"] = args.build
        args.file.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")

    s = score(data)
    c = s["counts"]
    print(f"Parity vs WPF {data['wpfReference']} at port {data['portBuild']}:")
    print(f"  weighted {s['weighted']:.1f}%  plain {s['plain']:.1f}%  "
          f"(live-verified only: {s['liveWeighted']:.1f}% of weight)")
    print(f"  {s['n']} features, weight {s['totalWeight']}: "
          + ", ".join(f"{k} {v}" for k, v in c.items()))
    print("  by area: " + ", ".join(f"{k} {v:.0f}%" for k, v in s["areas"].items()))

    if args.table or args.markdown:
        print()
        if args.markdown:
            print("| Area | Feature | W | Status | Points | Evidence |")
            print("|---|---|---|---|---|---|")
        for f in data["features"]:
            pts = f["weight"] * data["statusValues"][f["status"]]
            if args.markdown:
                print(f"| {f['area']} | {f['name']} | {f['weight']} | {f['status']} | {pts:.1f} | {f['evidence']} |")
            else:
                print(f"  {f['status']:<4} w{f['weight']} {pts:4.1f}  {f['area']}: {f['name']}")

    print(f"\nTop {args.top} by weighted gain if made LIVE:")
    for i, (g, f) in enumerate(s["gains"][: args.top], 1):
        nxt = f" -> {f['next']}" if f.get("next") else ""
        print(f"  {i:2}. +{g:.1f} pts  [{f['status']}] {f['name']}{nxt}")


if __name__ == "__main__":
    main()
