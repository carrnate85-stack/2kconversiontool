"""Offline control tests using a small, already mapped stock music resource."""
import hashlib
import json
import pathlib

from roster_wrapper_offline import ARTIFACTS, decode_wrapper, oodle_decoder


def main():
    game = pathlib.Path(r'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27')
    scratch = pathlib.Path(r'C:\Users\carrn\Documents\Codex\2026-10-17\task\roster-scratch')
    provenance = json.loads((scratch / 'asset-provenance.json').read_text())
    control = next(row for row in provenance['results'] if row['path'] == 'music.iff')
    assert control['archive'] == '0B' and control['offset'] == 983242178 and control['size'] == 3850
    with (game / '0B').open('rb') as archive:
        archive.seek(control['offset'])
        packed = archive.read(control['size'])
    assert hashlib.sha256(packed).hexdigest() == control['sha256']
    decompress = oodle_decoder(game / 'data' / 'oodle' / 'oo2core_9_win64.dll')
    decoded, metadata = decode_wrapper(packed, decompress)
    assert decoded == (scratch / 'music.decoded.bin').read_bytes()
    from cryptography.hazmat.decrepit.ciphers.algorithms import Blowfish
    from cryptography.hazmat.primitives.ciphers import Cipher, modes
    key = b'offline-test-key'  # synthetic test fixture only, NOT a game key
    assert len(key) == 16
    payload = packed[16:-8]
    payload += bytes(-len(payload) % 8)
    encryptor = Cipher(Blowfish(key), modes.ECB()).encryptor()
    encrypted = encryptor.update(payload) + encryptor.finalize()
    wrapped = packed[:3] + b'\x84' + packed[4:16] + encrypted + packed[-8:]
    roundtrip, _ = decode_wrapper(wrapped, decompress, key)
    assert roundtrip == decoded
    roster = (scratch / 'learn2k_roster.iff').read_bytes()
    try:
        decode_wrapper(roster, lambda *_: (_ for _ in ()).throw(AssertionError('decoder called')))
    except ValueError as e:
        assert 'decryption key' in str(e)
    else:
        raise AssertionError('Encrypted roster was not refused')
    report = {'fileOnly': True, 'gameFilesChanged': False, 'processAccess': False,
              'plainMusicExactMatch': True, 'authoredMusicSizeAndCrcVerified': True,
              'syntheticEncryptedMusicRoundtrip': True, 'actualRosterMissingKeyRefusedBeforeDecode': True,
              'actualRosterDecoded': False, 'mixedRosterVerified': False,
              'musicOutputBytes': len(decoded), 'musicOutputSha256': hashlib.sha256(decoded).hexdigest(),
              'musicWrapper': metadata}
    ARTIFACTS.mkdir(parents=True, exist_ok=True)
    with (ARTIFACTS / 'decoder-control-results.json').open('x', encoding='utf-8') as file:
        json.dump(report, file, indent=2)
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
