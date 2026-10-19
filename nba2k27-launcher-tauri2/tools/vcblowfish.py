"""Pure-Python Blowfish (ECB, big-endian blocks) matching mod.exe VCBLOWFISH.

mod.exe 0x14E180 is the textbook key schedule (18 P dwords XOR cycled key
bytes, 4 KB S-box copy) and 0x14E0A0 decrypts 8-byte blocks in place with
bswapped (big-endian) halves and no chaining. The initial P/S constants are the
fractional hex digits of pi, computed here so no third-party package or
download is needed. Keys are supplied by the caller and never logged.
"""
import functools
import struct

_MASK = 0xFFFFFFFF


@functools.lru_cache(maxsize=1)
def _pi_words():
    # 1042 dwords = 8336 hex digits of frac(pi), via Machin's formula.
    digits = 1042 * 8
    bits = digits * 4 + 64
    one = 1 << bits

    def arctan_inv(x):
        total = term = one // x
        x2, n, sign = x * x, 1, -1
        while term:
            term //= x2
            n += 2
            total += sign * (term // n)
            sign = -sign
        return total

    pi = 4 * (4 * arctan_inv(5) - arctan_inv(239))
    frac = (pi - (3 << bits)) >> 64
    hexed = f'{frac:0{digits}x}'[:digits]
    words = [int(hexed[i:i + 8], 16) for i in range(0, digits, 8)]
    if words[0] != 0x243F6A88 or words[18] != 0xD1310BA6:
        raise RuntimeError('pi constant generation failed')
    return tuple(words)


class Blowfish:
    def __init__(self, key):
        if not 1 <= len(key) <= 72:
            raise ValueError('Blowfish key must be 1..72 bytes')
        words = _pi_words()
        p = list(words[:18])
        s = [list(words[18 + 256 * i:18 + 256 * (i + 1)]) for i in range(4)]
        j = 0
        for i in range(18):
            d = 0
            for _ in range(4):
                d = (d << 8) | key[j]
                j = (j + 1) % len(key)
            p[i] ^= d
        self.p, self.s = p, s
        left = right = 0
        for i in range(0, 18, 2):
            left, right = self._encrypt(left, right)
            p[i], p[i + 1] = left, right
        for box in s:
            for i in range(0, 256, 2):
                left, right = self._encrypt(left, right)
                box[i], box[i + 1] = left, right

    def _f(self, x):
        s0, s1, s2, s3 = self.s
        return ((((s0[x >> 24] + s1[(x >> 16) & 0xFF]) & _MASK) ^ s2[(x >> 8) & 0xFF]) + s3[x & 0xFF]) & _MASK

    def _encrypt(self, left, right):
        p, f = self.p, self._f
        for i in range(16):
            left ^= p[i]
            right ^= f(left)
            left, right = right, left
        left, right = right, left
        return left ^ p[17], right ^ p[16]

    def _decrypt(self, left, right):
        p, f = self.p, self._f
        for i in range(17, 1, -1):
            left ^= p[i]
            right ^= f(left)
            left, right = right, left
        left, right = right, left
        return left ^ p[0], right ^ p[1]

    def _ecb(self, data, block):
        if len(data) % 8:
            raise ValueError('Blowfish ECB data must be 8-byte aligned')
        words = struct.unpack(f'>{len(data) // 4}I', data)
        out = []
        for i in range(0, len(words), 2):
            out.extend(block(words[i], words[i + 1]))
        return struct.pack(f'>{len(out)}I', *out)

    def decrypt(self, data):
        return self._ecb(data, self._decrypt)

    def encrypt(self, data):
        return self._ecb(data, self._encrypt)
