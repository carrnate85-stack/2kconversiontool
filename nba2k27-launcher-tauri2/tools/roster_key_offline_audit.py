"""File-only PE inspection for roster decoder research. Never opens a process."""
import argparse
import bisect
import json
import pathlib
import re
import struct

from wnba_controlflow_audit import Image, decoder, instruction_row, direct_calls


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('image', type=pathlib.Path)
    parser.add_argument('--dump', type=lambda x: int(x, 0), nargs='*', default=[])
    parser.add_argument('--refs', type=lambda x: int(x, 0), nargs='*', default=[])
    parser.add_argument('--calls', type=lambda x: int(x, 0), nargs='*', default=[])
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--ref-context', action='store_true')
    args = parser.parse_args()
    image = Image(args.image)
    md = decoder()
    spans = sorted((int(e.struct.BeginAddress), int(e.struct.EndAddress))
                   for e in getattr(image.pe, 'DIRECTORY_ENTRY_EXCEPTION', []))
    starts = [s[0] for s in spans]
    def span_at(rva):
        i = bisect.bisect_right(starts, rva) - 1
        return spans[i] if i >= 0 and rva < spans[i][1] else None
    result = {'fileOnly': True, 'image': str(args.image), 'refs': [], 'calls': {}, 'dumps': {}}
    targets = set(args.refs)
    # Common scalar RIP-relative load/store/LEA, validated with Capstone.
    pattern = re.compile(rb'[\x40-\x4f]?[\x8b\x89\x8d][\x05\x0d\x15\x1d\x25\x2d\x35\x3d]')
    for section in image.executable_sections() if targets else []:
        raw = image.data[section['raw']:section['raw'] + section['raw_size']]
        for match in pattern.finditer(raw):
            at = match.start()
            disp_at = match.end()
            if disp_at + 4 > len(raw):
                continue
            target = section['rva'] + disp_at + 4 + struct.unpack_from('<i', raw, disp_at)[0]
            if target not in targets:
                continue
            decoded = next(md.disasm(raw[at:at+16], section['rva'] + at, count=1), None)
            if decoded is None:
                continue
            row = instruction_row(decoded)
            if target not in row.get('targets', []):
                continue
            row['function'] = span_at(decoded.address)
            if args.ref_context:
                row['following'] = [instruction_row(i) for i in md.disasm(raw[at:at+100], decoded.address)]
            result['refs'].append(row)
    if args.calls:
        result['calls'] = {hex(k): [{'rva': site, 'function': span_at(site)} for site in sites]
                           for k, sites in direct_calls(image, set(args.calls)).items()}
    for rva in args.dump:
        span = span_at(rva)
        start, end = span or (rva, rva + 256)
        result['dumps'][hex(rva)] = [f'{i.address:08X}: {i.mnemonic} {i.op_str}'
                                   for i in md.disasm(image.bytes(start, min(end-start, 24000)), start)]
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # Create-only artifacts: an audit must never overwrite prior evidence.
    with args.output.open('x', encoding='utf-8') as out:
        json.dump(result, out, indent=2)
    print(json.dumps({'output': str(args.output), 'referenceCount': len(result['refs']),
                      'calls': result['calls'], 'dumpCount': len(result['dumps'])}))


if __name__ == '__main__':
    main()
