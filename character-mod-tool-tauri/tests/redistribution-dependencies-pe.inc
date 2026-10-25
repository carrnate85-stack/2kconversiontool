"""Small read-only PE/CLI inventory parser; no assemblies or DLLs are loaded."""


class PE:
    def __init__(self, data):
        self.data = data
        assert data[:2] == b'MZ', 'Not a PE image'
        pe = self.u32(0x3c)
        assert data[pe:pe + 4] == b'PE\0\0', 'Invalid PE signature'
        self.machine, count = struct.unpack_from('<HH', data, pe + 4)
        opt = pe + 24
        self.magic = self.u16(opt)
        assert self.magic in (0x10b, 0x20b), 'Unsupported PE optional header'
        self.base = struct.unpack_from('<Q' if self.magic == 0x20b else '<I', data,
                                       opt + (24 if self.magic == 0x20b else 28))[0]
        self.subsystem = self.u16(opt + 68)
        directory = opt + (112 if self.magic == 0x20b else 96)
        self.directories = [struct.unpack_from('<II', data, directory + i * 8) for i in range(16)]
        section = opt + self.u16(pe + 20)
        self.sections = []
        for i in range(count):
            pos = section + i * 40
            virtual_size, rva, raw_size, raw = struct.unpack_from('<IIII', data, pos + 8)
            self.sections.append((rva, max(virtual_size, raw_size), raw))

    def u16(self, pos):
        return struct.unpack_from('<H', self.data, pos)[0]

    def u32(self, pos):
        return struct.unpack_from('<I', self.data, pos)[0]

    def offset(self, rva):
        for start, size, raw in self.sections:
            if start <= rva < start + size:
                return raw + rva - start
        if rva < min((row[0] for row in self.sections), default=0):
            return rva
        raise ValueError(f'Unmapped RVA {rva:#x}')

    def string(self, pos):
        return self.data[pos:self.data.index(b'\0', pos)].decode('utf-8')

    def imports(self, delay=False):
        rva, size = self.directories[13 if delay else 1]
        if not rva:
            return []
        start = self.offset(rva)
        stride = 32 if delay else 20
        result = []
        for pos in range(start, start + size, stride):
            values = struct.unpack_from('<' + 'I' * (stride // 4), self.data, pos)
            if not any(values):
                break
            name = values[1] if delay else values[3]
            if delay and not values[0] & 1:
                name -= self.base
            result.append(self.string(self.offset(name)))
        return sorted(set(result), key=str.lower)

    def clr(self):
        rva, _ = self.directories[14]
        if not rva:
            return None
        header = self.offset(rva)
        metadata = self.offset(self.u32(header + 8))
        assert self.data[metadata:metadata + 4] == b'BSJB', 'Invalid CLR metadata'
        version_size = self.u32(metadata + 12)
        version = self.data[metadata + 16:metadata + 16 + version_size].rstrip(b'\0').decode()
        pos = (metadata + 16 + version_size + 3) & ~3
        count = self.u16(pos + 2)
        pos += 4
        streams = {}
        for _ in range(count):
            offset, size = struct.unpack_from('<II', self.data, pos)
            name = self.string(pos + 8)
            streams[name] = self.data[metadata + offset:metadata + offset + size]
            pos = (pos + 8 + len(name) + 1 + 3) & ~3
        table = streams.get('#~', streams.get('#-'))
        assert table is not None, 'Missing CLR tables'
        heaps = table[6]
        valid = struct.unpack_from('<Q', table, 8)[0]
        rows, pos = {}, 24
        for index in range(64):
            if valid & (1 << index):
                rows[index] = struct.unpack_from('<I', table, pos)[0]
                pos += 4
        # ECMA-335 II.22 table layouts through AssemblyRef. Coded indices encode
        # tag width and participating tables; ordinary table indices use tN.
        coded = {
            'resolution': (2, [0, 26, 35, 1]), 'type': (2, [2, 1, 27]),
            'constant': (2, [4, 8, 23]),
            'attribute': (5, [6, 4, 1, 2, 8, 9, 10, 0, 14, 23, 20, 17, 26, 27, 32, 35, 38, 39, 40, 42, 44, 43]),
            'attrtype': (3, [6, 10]), 'marshal': (1, [4, 8]), 'security': (2, [2, 6, 32]),
            'member': (3, [2, 1, 26, 6, 27]), 'semantics': (1, [20, 23]),
            'method': (1, [6, 10]), 'forwarded': (1, [4, 6]),
        }
        layouts = [
            '2 s g g g', 'resolution s s', '4 s s type t4 t6', 't4', '2 s b', 't6',
            '4 2 2 s b t8', 't8', '2 2 s', 't2 type', 'member s b', '2 constant b',
            'attribute attrtype b', 'marshal b', '2 security b', '2 4 t2', '4 t4', 'b',
            't2 t20', 't20', '2 s type', 't2 t23', 't23', '2 s b',
            '2 t6 semantics', 't2 method method', 's', 'b', '2 forwarded s t26',
            '4 t4', '4 4', '4', '4 2 2 2 2 4 b s s', '4', '4 4 4',
            '2 2 2 2 4 b s s b',
        ]

        def width(token):
            if token in ('2', '4'):
                return int(token)
            if token in ('s', 'g', 'b'):
                return 4 if heaps & {'s': 1, 'g': 2, 'b': 4}[token] else 2
            if token.startswith('t') and token[1:].isdigit():
                return 4 if rows.get(int(token[1:]), 0) >= 65536 else 2
            bits, targets = coded[token]
            return 4 if max(rows.get(t, 0) for t in targets) >= (1 << (16 - bits)) else 2

        locations = {}
        for index, layout in enumerate(layouts):
            tokens = layout.split()
            stride = sum(width(token) for token in tokens)
            locations[index] = (pos, stride, tokens)
            pos += rows.get(index, 0) * stride

        def read_row(index, number):
            start, stride, tokens = locations[index]
            cursor = start + number * stride
            values = []
            for token in tokens:
                size = width(token)
                values.append(int.from_bytes(table[cursor:cursor + size], 'little'))
                cursor += size
            return values

        def string(index):
            data = streams['#Strings']
            return data[index:data.index(b'\0', index)].decode()

        assemblies = []
        for number in range(rows.get(35, 0)):
            row = read_row(35, number)
            assemblies.append({'name': string(row[6]), 'version': '.'.join(map(str, row[:4]))})
        modules = [string(read_row(26, number)[0]) for number in range(rows.get(26, 0))]
        pinvoke = []
        for number in range(rows.get(28, 0)):
            row = read_row(28, number)
            pinvoke.append({'module': modules[row[3] - 1], 'entryPoint': string(row[2])})
        frameworks = re.findall(rb'\.NET(?:Framework|CoreApp|Standard),Version=v[0-9.]+', streams.get('#Blob', b''))
        return {'metadataVersion': version, 'flags': self.u32(header + 16),
                'assemblyReferences': assemblies, 'pinvoke': pinvoke,
                'targetFrameworkStrings': [value.decode() for value in frameworks],
                'minimumFrameworkVersion': 'Not declared; CLR metadata version is not a product minimum version'}


def pe_self_test():
    for magic in (0x10b, 0x20b):
        data = bytearray(0x1000)
        data[:2] = b'MZ'
        struct.pack_into('<I', data, 0x3c, 0x80)
        data[0x80:0x84] = b'PE\0\0'
        opt = 0x98
        size = 240 if magic == 0x20b else 224
        struct.pack_into('<HH', data, 0x84, 0x8664 if magic == 0x20b else 0x14c, 1)
        struct.pack_into('<H', data, 0x94, size)
        struct.pack_into('<H', data, opt, magic)
        struct.pack_into('<Q' if magic == 0x20b else '<I', data,
                         opt + (24 if magic == 0x20b else 28), 0x400000)
        struct.pack_into('<IIII', data, opt + size + 8, 0x1000, 0x1000, 0xc00, 0x400)
        directory = opt + (112 if magic == 0x20b else 96)
        struct.pack_into('<II', data, directory + 8, 0x1020, 40)
        struct.pack_into('<IIIII', data, 0x420, 0, 0, 0, 0x1080, 0)
        data[0x480:0x480 + 13] = b'KERNEL32.dll\0'
        struct.pack_into('<II', data, directory + 13 * 8, 0x1100, 64)
        struct.pack_into('<IIIIIIII', data, 0x500, 1, 0x1180, 0, 0, 0, 0, 0, 0)
        data[0x580:0x580 + 13] = b'VCOMP140.dll\0'
        pe = PE(bytes(data))
        assert pe.imports() == ['KERNEL32.dll']
        assert pe.imports(True) == ['VCOMP140.dll']
        assert pe.offset(0x1080) == 0x480
        assert pe.clr() is None
        # Legacy PE32 delay descriptors can store virtual addresses, not RVAs.
        struct.pack_into('<II', data, 0x500, 0, 0x401180)
        assert PE(bytes(data)).imports(True) == ['VCOMP140.dll']
    try:
        PE(b'not a PE')
    except (AssertionError, struct.error):
        pass
    else:
        raise AssertionError('Invalid image accepted')
