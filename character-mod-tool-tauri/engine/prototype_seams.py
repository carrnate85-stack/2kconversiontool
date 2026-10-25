"""Conservative, position-only repair of converted neck/chest material borders."""

import math
import copy
import struct
import zipfile
from collections import Counter, defaultdict


REGIONS = {"face_shader", "arms_shader", "arm_shader", "chest_shader", "torso_shader"}
PAIR_REGIONS = (("face_shader", "arms_shader"), ("face_shader", "arm_shader"),
                ("face_shader", "chest_shader"), ("arms_shader", "torso_shader"),
                ("arm_shader", "torso_shader"), ("chest_shader", "torso_shader"))


def group_gap(positions, aa, bb):
    """Symmetric nearest-sample gap between two UV-split border groups."""
    return max(
        max(min(math.dist(positions[i], positions[j]) for j in bb) for i in aa),
        max(min(math.dist(positions[j], positions[i]) for i in aa) for j in bb),
    )


def border(positions, triangles, epsilon):
    """Group UV-split copies for analysis only; no vertex is welded or removed."""
    bins, groups, canonical = defaultdict(list), defaultdict(list), {}
    for i in sorted({i for t in triangles for i in t}):
        p = positions[i]
        cell = tuple(math.floor(v / epsilon) for v in p)
        candidates = [j for x in range(-1, 2) for y in range(-1, 2) for z in range(-1, 2)
                      for j in bins[(cell[0] + x, cell[1] + y, cell[2] + z)]
                      if math.dist(p, positions[j]) <= epsilon]
        j = min(candidates, key=lambda j: (math.dist(p, positions[j]), j)) if candidates else i
        canonical[i] = j
        groups[j].append(i)
        if not candidates:
            bins[cell].append(i)
    edges = Counter()
    for triangle in triangles:
        a, b, c = (canonical[i] for i in triangle)
        if len({a, b, c}) == 3:
            edges.update((tuple(sorted((a, b))), tuple(sorted((b, c))), tuple(sorted((c, a)))))
    adjacency = defaultdict(set)
    for (a, b), count in edges.items():
        if count == 1:
            adjacency[a].add(b)
            adjacency[b].add(a)
    return groups, adjacency


def seam_plan(positions, regions):
    if not positions or len(positions) > 500000:
        raise ValueError("Unsupported player vertex count for seam closing.")
    if not all(math.isfinite(v) for p in positions for v in p):
        raise ValueError("Player contains non-finite positions.")
    height = max(p[1] for p in positions) - min(p[1] for p in positions)
    if not 100 < height < 300:
        raise ValueError("Seam closing requires native NBA 2K player coordinates.")
    # Converted neck seams can be visibly open, but a broad nearest-border search
    # can mistake nearby body openings for a seam. Keep the automatic search to
    # a narrow fraction of player height.
    epsilon, max_gap = height * 0.0001, height * 0.004
    borders = {name: border(positions, tris, epsilon) for name, tris in regions.items() if name in REGIONS}
    pairs, directed_pairs, warnings = [], [], []
    for left, right in PAIR_REGIONS:
        if left not in borders or right not in borders:
            continue
        lg, la = borders[left]
        rg, ra = borders[right]
        if not la or not ra:
            continue
        if len(la) * len(ra) > 2000000:
            raise ValueError("Player borders are too fragmented for safe automatic seam closing.")
        nearest = lambda i, ids: min(ids, key=lambda j: (math.dist(positions[i], positions[j]), j))
        lr = {i: nearest(i, ra) for i in la}
        rl = {j: nearest(j, la) for j in ra}
        matches = {i: j for i, j in lr.items() if rl[j] == i and math.dist(positions[i], positions[j]) <= max_gap}
        # A matching strip of border edges distinguishes a seam from nearby skin.
        graph = {i: {n for n in la[i] if n in matches and matches[n] in ra[j]} for i, j in matches.items()}
        remaining = set(graph)
        accepted = set()
        while remaining:
            stack, component = [remaining.pop()], set()
            while stack:
                i = stack.pop()
                component.add(i)
                for n in graph[i] & remaining:
                    remaining.remove(n)
                    stack.append(n)
            if len(component) >= 4:
                accepted.update(component)
        for i in sorted(accepted):
            j = matches[i]
            pair = (tuple(lg[i]), tuple(rg[j]))
            pairs.append(pair)
            directed_pairs.append((*pair, left == "face_shader"))
        rejected = sum(math.dist(positions[i], positions[j]) > epsilon for i,j in matches.items() if i not in accepted)
        if rejected:
            warnings.append(f"Skipped {rejected} ambiguous {left}/{right} border matches.")
    # Preserve the target body silhouette at face/body seams. The face/neck is
    # the converted side; moving both sides to a midpoint can push the otherwise
    # correct arms/chest outward into a jersey. Other tiny body-material seams
    # still meet at their midpoint.
    targets = defaultdict(list)
    for aa, bb, preserve_body in directed_pairs:
        ids = aa + bb
        if len(ids) > 128:
            raise ValueError("Too many overlapping border copies for a safe seam repair; no changes were applied.")
        diameter = max(math.dist(positions[i], positions[j]) for i in ids for j in ids)
        if diameter > max_gap:
            raise ValueError("Combined border matches exceed the safe seam distance; no changes were applied.")
        if diameter <= 0.0005:
            continue
        if preserve_body:
            for i in aa:
                anchor = min(bb, key=lambda j: (math.dist(positions[i], positions[j]), j))
                targets[i].append(positions[anchor])
        else:
            samples = sorted({positions[i] for i in ids})
            center = tuple(sum(p[a] for p in samples) / len(samples) for a in range(3))
            for i in ids:
                targets[i].append(center)
    changes = {}
    for i, candidate_targets in targets.items():
        if max((math.dist(a, b) for a in candidate_targets for b in candidate_targets), default=0.0) > epsilon:
            raise ValueError("A border vertex received conflicting seam matches; no changes were applied.")
        changes[i] = tuple(sum(p[a] for p in candidate_targets) / len(candidate_targets) for a in range(3))
    gap = max((group_gap(positions, aa, bb) for aa, bb in pairs), default=0.0)
    if not pairs:
        warnings.append("No safely matching neck/chest border strips were found; no geometry was changed.")
    max_move = max((math.dist(positions[i], p) for i, p in changes.items()), default=0.0)
    return changes, pairs, {"applied": bool(changes), "correctedVertices": len(changes),
                            "pairs": len(pairs), "maxGapBefore": gap, "maxGapAfter": gap,
                            "maxVertexMove": max_move,
                            "warnings": warnings}


def close_seams(source, output):
    """Patch the existing position stream and prove all other member bytes unchanged."""
    from hair_tools.regular_hair_tangent_fit import _archive_member, _load_mesh, _triangles, _cross, _sub, _dot
    with zipfile.ZipFile(source) as archive:
        entries = archive.infolist()
        if len(entries) > 10000 or sum(row.file_size for row in entries) > 512 * 1024 * 1024:
            raise ValueError("Player archive exceeds the seam closer size limit.")
        if len({row.filename.casefold() for row in entries}) != len(entries):
            raise ValueError("Player archive has duplicate member names.")
    mesh = _load_mesh(source, require_face=True)
    regions = {name: _triangles(mesh, name) for name in REGIONS
               if any(p.get("Material", "").lower() == name for p in mesh.model.get("Prim", []))}
    if "face_shader" not in regions or "torso_shader" not in regions:
        raise ValueError("Choose a converted full player with face and torso geometry.")
    changes, pairs, report = seam_plan(mesh.positions, regions)
    if not changes:
        return report
    descriptor = mesh.model["VertexFormat"]["POSITION0"]
    stream = mesh.streams[int(descriptor.get("Stream", 0))]
    stride, offset = int(stream["Stride"]), int(descriptor.get("ByteOffset", 0))
    fmt = descriptor["Format"].upper()
    if fmt not in ("R21G21B22_UINT", "R32G32B32_FLOAT"):
        raise ValueError("Unsupported position encoding for safe seam closing.")
    width = 8 if fmt == "R21G21B22_UINT" else 12
    if offset < 0 or offset + width > stride:
        raise ValueError("Invalid player position stream layout.")
    with zipfile.ZipFile(source) as archive:
        name = _archive_member(archive, stream["Binary"])
        original = archive.read(name)
        if len(original) != int(stream["Size"]):
            raise ValueError("Seam closing requires an uncompressed converted vertex buffer.")
        patched = bytearray(original)
        for i, p in changes.items():
            if fmt == "R32G32B32_FLOAT":
                encoded = struct.pack("<fff", *p)
            else:
                scales, offsets = descriptor["Scale"], descriptor["Offset"]
                if any(not math.isfinite(s) or s <= 0 for s in scales[:3]):
                    raise ValueError("Invalid packed position scales.")
                raw = [round((p[a] - offsets[a]) / scales[a]) for a in range(3)]
                if any(not 0 <= raw[a] <= ((1 << (22 if a == 2 else 21)) - 1) for a in range(3)):
                    raise ValueError("Corrected seam exceeds native position encoding range.")
                encoded = struct.pack("<Q", raw[0] | raw[1] << 21 | raw[2] << 42)
            cursor = i * stride + offset
            patched[cursor:cursor + width] = encoded
        with zipfile.ZipFile(output, "x") as fixed:
            fixed.comment = archive.comment
            for entry in entries:
                fixed.writestr(copy.copy(entry), patched if entry.filename == name else archive.read(entry))
    verified = _load_mesh(output, require_face=True)
    if len(verified.positions) != len(mesh.positions) or verified.indices != mesh.indices or verified.uvs != mesh.uvs:
        raise ValueError("Seam correction changed topology or UVs; player was not published.")
    for a,b,c in _triangles(mesh):
        if not any(i in changes for i in (a,b,c)):
            continue
        old_normal = _cross(_sub(mesh.positions[b],mesh.positions[a]), _sub(mesh.positions[c],mesh.positions[a]))
        new_normal = _cross(_sub(verified.positions[b],verified.positions[a]), _sub(verified.positions[c],verified.positions[a]))
        if _dot(old_normal,old_normal) > 1e-10 and _dot(old_normal,new_normal) <= 0:
            raise ValueError("Closing this seam would invert a triangle; no changes were applied.")
        old_area = math.sqrt(_dot(old_normal, old_normal))
        new_area = math.sqrt(_dot(new_normal, new_normal))
        if old_area > 1e-5 and not 0.6 <= new_area / old_area <= 1.75:
            raise ValueError("Closing this seam would distort a triangle too far; no changes were applied.")
        for i, j in ((a, b), (b, c), (c, a)):
            old_edge = math.dist(mesh.positions[i], mesh.positions[j])
            new_edge = math.dist(verified.positions[i], verified.positions[j])
            if old_edge > 1e-5 and not 0.6 <= new_edge / old_edge <= 1.75:
                raise ValueError("Closing this seam would stretch an edge too far; no changes were applied.")
    with zipfile.ZipFile(source) as before, zipfile.ZipFile(output) as after:
        if before.namelist() != after.namelist() or after.testzip():
            raise ValueError("Seam correction archive validation failed.")
        for entry in entries:
            if entry.filename != name and before.read(entry) != after.read(entry.filename):
                raise ValueError("Seam correction changed a non-position member.")
    report["maxGapAfter"] = max((group_gap(verified.positions, aa, bb) for aa,bb in pairs), default=0.0)
    verified_height = max(p[1] for p in verified.positions) - min(p[1] for p in verified.positions)
    seam_tolerance = max(0.0006, verified_height * 0.0001)
    if report["maxGapAfter"] > seam_tolerance:
        raise ValueError("Seam correction did not close the matched borders; player was not published.")
    if report["maxGapAfter"] > 0.0006:
        report["warnings"].append(
            f"Preserved the target body border; residual UV-split gap {report['maxGapAfter']:.4g} is within native tolerance."
        )
    return report
