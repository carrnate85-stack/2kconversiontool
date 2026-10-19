"""Reference payloads from the WPF animation preview's Python runtime.

Runs the nba2k27-animation-lab decoder (the code PlayerAnimationPreview.exe
bundles) with the same steps as portable/engine.py Engine.decode/character, on
a profile prepared by the Rust launcher (Cache/player-animation-preview/
profiles/<id>). The Rust test animation_preview::tests::
live_decode_matches_python_runtime compares its own payloads with these.

  python tools/animation_preview_reference.py --profile <profile dir> --out <dir> [--extra 40]

Read-only toward the profile; writes JSON files into --out only.
"""
import argparse, gzip, json, math, random, sys, zipfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAB = HERE.parents[1] / 'nba2k27-animation-lab'
sys.path[:0] = [str(LAB), str(LAB / 'web-viewer')]

from archive import AnimationArchive  # noqa: E402
from animation_lab import decode_loaded_clip  # noqa: E402
from character_mesh import read_character, finger_rig  # noqa: E402
from hand_pose import load_library  # noqa: E402
from shot_events import shot_events  # noqa: E402
from trajectory import sample as sample_trajectory  # noqa: E402

CATALOG = HERE.parent / 'assets/animation-preview/catalog.json.gz'


def variants(data, choice):
    return data['variant_sets'][choice['variant_set']] if 'variant_set' in choice else choice['variants']


def resolve(data, fields, field, value):
    choice = fields[field]['choices'][value]
    found = [dict(v, variant_index=i) for i, v in enumerate(
        v for v in variants(data, choice) if v.get('previewable', True) and v.get('member'))]
    return dict(field=field, value=value, label=choice['label'], status=choice['status'], variants=found)


def decode(profile, archives, library, data, fields, field, value, variant=0, shot=False):
    """portable/engine.py Engine.decode create(), unchanged in substance."""
    selected = resolve(data, fields, field, value)
    v = selected['variants'][variant]
    archive = Path(v.get('archive', 'inputs/nba_animations.iff')).name
    name = v['member']
    path = profile / 'assets' / archive
    entry = archives.setdefault(archive, AnimationArchive(str(path)))
    container = entry.load_multiple(name) if name.endswith('.MultipleMoCap') else None
    if container:
        actor = next((a['actor_index'] for a in container.actors if a['joint_count'] == 22), None)
        if actor is None:
            raise ValueError('This animation contains no player actor')
        clip = container.actor(actor)
    else:
        actor = 0
        clip = entry.load(name)
    if clip.bone_count != 22:
        raise ValueError('This animation contains no player actor')
    seconds = clip.ticks[-1] / (clip.rate * clip.speed)
    if not math.isfinite(seconds) or not 0 < seconds <= 180:
        raise ValueError('Motion duration exceeds the preview budget')
    out = decode_loaded_clip(clip, str(path), name, 60, container, actor, library)
    if container:
        first = sample_trajectory(clip, 0)
        info = container.actors[actor]
        word = info['orientation_word_raw']
        out.update(use_container_placement=True, preview_anchor_placement=dict(
            position_xz_cm=info['position_xz_raw'] if first[:2] == (0, 0) else [0, 0],
            heading_units=word - 65536 if word & 32768 else word))
    if shot:
        out.pop('use_container_placement', None)
        out['shot_events'] = shot_events(clip)
    out['roster_selection'] = {k: selected[k] for k in ('field', 'value', 'label')}
    out['profile_id'] = profile.name
    return out


def character(profile):
    """portable/engine.py Engine.character create()."""
    source = read_character(profile / 'assets/character.iff')
    rig, skin = finger_rig(source)
    groups, indices, last = [], [], None
    for triangle, shader in zip(source['triangles'], source['materials']):
        mat = 1 if shader in ('torso_shader', 'arms_shader', 'legs_shader', 'hidden_shader') else 2 if shader in ('l_eye_shader', 'r_eye_shader') else 3 if shader == 'mouth_shader' else 0
        if mat != last:
            groups.append(dict(start=len(indices), count=0, material=mat))
            last = mat
        indices.extend(triangle)
        groups[-1]['count'] += 3
    return {**rig, 'schema': 'nba2k27-preview-character-v1', 'name': 'Neutral player preview',
            'source_sha256': source['source_sha256'],
            'positions_cm': [v for p in source['positions_cm'] for v in p],
            'main_rest_cm': [source['bone_rest_cm'][i] for i in source['main_joint_indices']],
            'skin_index': [i for w in skin for i in [j for j, a in w] + [0] * (4 - len(w))],
            'skin_weight': [a for w in skin for a in [a for j, a in w] + [0.] * (4 - len(w))],
            'indices': indices, 'groups': groups, 'vertex_count': len(source['positions_cm']),
            'triangle_count': len(source['triangles']), 'status': 'Neutral body with authored hand poses; approximate helper adapter'}


def selections(data, fields, extra, seed):
    picks = []
    for key, f in fields.items():
        for c in f['choices']:
            if resolve(data, fields, key, c['value'])['variants']:
                picks.append(('load', key, c['value'], 0))
                break
    for key in ('ShotJumper', 'ShotRelease', 'ShotRelease2'):
        for c in fields[key]['choices']:
            if c['status'] == 'mapped' and resolve(data, fields, key, c['value'])['variants']:
                picks.append(('shot-part', key, c['value'], 0))
                break
    seen_archives = set()
    for key, f in fields.items():
        for c in f['choices']:
            for v in resolve(data, fields, key, c['value'])['variants']:
                a = Path(v.get('archive', 'inputs/nba_animations.iff')).name
                if a not in seen_archives:
                    seen_archives.add(a)
                    picks.append(('load', key, c['value'], v['variant_index']))
    ft = next(c for c in fields['FreeThrow']['choices'] if len(resolve(data, fields, 'FreeThrow', c['value'])['variants']) > 1)
    picks += [('load', 'FreeThrow', ft['value'], i) for i in range(len(resolve(data, fields, 'FreeThrow', ft['value'])['variants']))]
    rng = random.Random(seed)
    pool = [(key, c['value']) for key, f in fields.items() for c in f['choices']]
    while extra > 0:
        key, value = rng.choice(pool)
        found = resolve(data, fields, key, value)['variants']
        if found:
            picks.append(('load', key, value, rng.randrange(len(found))))
            extra -= 1
    return list(dict.fromkeys(picks))


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--profile', type=Path, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--extra', type=int, default=40)
    p.add_argument('--seed', type=int, default=2027)
    args = p.parse_args()
    data = json.loads(gzip.decompress(CATALOG.read_bytes()))
    fields = {f['key']: f for f in data['fields']}
    args.out.mkdir(parents=True, exist_ok=True)
    hands = args.out / '_hands.iff'
    with zipfile.ZipFile(hands, 'w') as z:
        z.write(args.profile / 'assets/hand_pose_male.SingleMoCap', 'hand_pose_male.SingleMoCap')
    library = load_library(str(hands))
    archives = {}
    (args.out / 'character.json').write_text(json.dumps(
        dict(request=dict(op='character'), payload=character(args.profile)), allow_nan=False))
    picks = selections(data, fields, args.extra, args.seed)
    for n, (op, field, value, variant) in enumerate(picks):
        request = dict(op=op, field=field, value=value, variant=variant)
        try:
            payload = decode(args.profile, archives, library, data, fields, field, value, variant, op == 'shot-part')
        except (ValueError, KeyError, IndexError) as error:
            payload = dict(error=str(error))
        (args.out / f'{n:03d}-{op}-{field}-{value}-{variant}.json').write_text(
            json.dumps(dict(request=request, payload=payload), separators=(',', ':'), allow_nan=False))
        print(n, op, field, value, variant, 'error' if 'error' in payload else len(payload['frames']), flush=True)


if __name__ == '__main__':
    main()
