"""Pure sparse-memory fixtures; no process access."""
import unittest
from debug_menu_v2_readonly_probe import capture, entry_valid, FEATURES

BASE = 0x140000000


class Fixture:
    def __init__(self):
        self.manager, self.table = BASE+0xA1F1260, 0x200000000
        self.bytes, self.values_read = {}, []
        self.keys = sorted([1, 2, 3, 0x40000000, *(row[1] for row in FEATURES), 0xF0000000])
        self.put(BASE+0xA1F1248, self.manager, 8)
        self.put(BASE+0xA1F1250, 0, 8)
        self.put(self.manager+0xF8, self.table, 8)
        self.put(self.manager+0x110, len(self.keys), 4)
        self.entries = {}
        for n, key in enumerate(self.keys):
            pointer = 0x210000000+n*16
            self.entries[key] = pointer
            self.put(self.table+n*8, pointer, 8)
            self.put(pointer, key, 4)
            if key in {row[1] for row in FEATURES}:
                self.put(pointer+4, 43, 4)
                self.put(pointer+8, 0, 1)
        for _name, _key, _initializer, obj, vtable in FEATURES:
            self.put(BASE+obj, BASE+vtable, 8)

    def put(self, at, value, size):
        for n, byte in enumerate(value.to_bytes(size, 'little')):
            self.bytes[at+n] = byte

    def read(self, at, size):
        for key, pointer in self.entries.items():
            if at == pointer+8:
                if key not in {row[1] for row in FEATURES}:
                    raise AssertionError('unrelated entry value read')
                self.values_read.append(key)
        return bytes(self.bytes[at+n] for n in range(size))

    def span(self, at, size, kind):
        if at < 0x10000 or size <= 0:
            raise ValueError('fixture pointer span invalid')
        if at == self.manager and kind != 'image':
            raise ValueError('static fallback must be image-backed')


class DebugProbePolicy(unittest.TestCase):
    def test_exact_three_only_and_stable(self):
        memory = Fixture()
        first = capture(memory.read, memory.span, BASE)
        self.assertEqual(first, capture(memory.read, memory.span, BASE))
        self.assertEqual(first[0][3], 'static-fallback')
        self.assertEqual(len(first[1]), 3)
        self.assertEqual(set(memory.values_read), {row[1] for row in FEATURES})

    def test_missing_manager(self):
        memory = Fixture()
        memory.put(BASE+0xA1F1248, 0, 8)
        with self.assertRaises(ValueError):
            capture(memory.read, memory.span, BASE)

    def test_count_bound(self):
        for count in (0, 100001):
            memory = Fixture()
            memory.put(memory.manager+0x110, count, 4)
            with self.assertRaises(ValueError):
                capture(memory.read, memory.span, BASE)

    def test_canonical_override(self):
        memory = Fixture()
        override = 0x220000000
        memory.put(BASE+0xA1F1250, override, 8)
        memory.put(override+0xF8, memory.table, 8)
        memory.put(override+0x110, len(memory.keys), 4)
        observed = capture(memory.read, memory.span, BASE)
        self.assertEqual(observed[0][2], override)
        self.assertEqual(observed[0][3], 'registered-private')

    def test_wrong_static_class(self):
        memory = Fixture()
        memory.put(BASE+FEATURES[0][3], BASE+FEATURES[1][4], 8)
        with self.assertRaises(ValueError):
            capture(memory.read, memory.span, BASE)

    def test_malformed_allowed_entry(self):
        for packed, value in ((42, 0), (75, 0), (43, 2)):
            memory = Fixture()
            pointer = memory.entries[FEATURES[0][1]]
            memory.put(pointer+4, packed, 4)
            memory.put(pointer+8, value, 1)
            with self.assertRaises(ValueError):
                capture(memory.read, memory.span, BASE)

    def test_duplicate_neighbor(self):
        memory = Fixture()
        key = FEATURES[0][1]
        index = memory.keys.index(key)
        neighbor = memory.entries[memory.keys[index+1]]
        memory.put(neighbor, key, 4)
        with self.assertRaises(ValueError):
            capture(memory.read, memory.span, BASE)

    def test_changed_generation(self):
        memory = Fixture()
        before = capture(memory.read, memory.span, BASE)
        memory.put(memory.entries[FEATURES[0][1]]+8, 1, 1)
        self.assertNotEqual(before, capture(memory.read, memory.span, BASE))

    def test_header_mask_semantics(self):
        self.assertTrue(entry_valid(1, 43, 0, 1))
        self.assertTrue(entry_valid(1, 43 | 0x80000000, 1, 1))
        self.assertFalse(entry_valid(2, 43, 1, 1))


if __name__ == '__main__':
    unittest.main()
