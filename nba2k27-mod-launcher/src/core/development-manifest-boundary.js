const fsp = require('node:fs/promises');
const {
  EXACT_GAME_SHA256,
  readPeLayout,
  rvaToFileOffset,
} = require('./resource-device-boundary');

const DEVELOPMENT_ROUTE_SPEC = Object.freeze({
  signatures: Object.freeze({
    fileManagerEnvironmentInit: Object.freeze({ rva: 0x4d15d70, bytes: '56574883ec284889ce488d542424' }),
    consumerDeviceSetup: Object.freeze({ rva: 0x805e0, bytes: '5556574881ecb0010000488dac2480000000' }),
    consumerEnvironmentWrite: Object.freeze({ rva: 0x4d15dd0, bytes: 'c780a00b00006bab68b6' }),
    developmentEnvironmentWrite: Object.freeze({ rva: 0x4d15de1, bytes: 'c780a00b0000c2b3d54d' }),
    developmentDeviceSetup: Object.freeze({ rva: 0x524a9e0, bytes: '555657534883ec58488d6c2450' }),
    developmentManifestGate: Object.freeze({ rva: 0x518dbb5, bytes: '81b8a00b0000c2b3d54d' }),
    developmentManifestConstructor: Object.freeze({ rva: 0x51a0420, bytes: '564883ec204889cee863170000' }),
    developmentManifestIndexBuilder: Object.freeze({ rva: 0x51a0a00, bytes: '4157415641554154565755534883ec48' }),
  }),
  strings: Object.freeze({
    consumerArchive: Object.freeze({ rva: 0x65fa8ec, text: '/0A' }),
    tempPrefix: Object.freeze({ rva: 0x65fa8f0, text: 'TMP:' }),
    vcnfsPrefix: Object.freeze({ rva: 0x80a803c, text: 'vcnfs:' }),
    gameDebugRoot: Object.freeze({ rva: 0x80c944c, text: 'GAME:../DEBUG/' }),
    vcnfsDebugRoot: Object.freeze({ rva: 0x80c945b, text: 'vcnfs:../DEBUG/' }),
    mirrorManifestConfig: Object.freeze({
      rva: 0x80a91cf,
      text: '"DEVELOPMENT_MIRROR_MANIFEST_NAME":"vcfast_load.manifest",',
    }),
    vcnfsManifestConfig: Object.freeze({
      rva: 0x80a920b,
      text: '"DEVELOPMENT_VCNFS_MANIFEST_NAME":"vcnfs.manifest",',
    }),
    maxHandlesConfig: Object.freeze({
      rva: 0x80a95a8,
      text: '"DEVELOPMENT_MANIFEST_MAX_OPEN_HANDLES":0,',
    }),
  }),
});

async function readAtRva(handle, layout, rva, length) {
  const output = Buffer.alloc(length);
  const result = await handle.read(output, 0, output.length, rvaToFileOffset(layout, rva));
  if (result.bytesRead !== output.length) throw new Error(`Could not read RVA 0x${rva.toString(16)}.`);
  return output;
}

async function verifyRecord(handle, layout, record, encoding = 'hex') {
  const expected = encoding === 'hex' ? Buffer.from(record.bytes, 'hex') : Buffer.from(record.text, 'ascii');
  const actual = await readAtRva(handle, layout, record.rva, expected.length);
  return {
    rva: `0x${record.rva.toString(16)}`,
    expected: encoding === 'hex' ? record.bytes : record.text,
    actual: encoding === 'hex' ? actual.toString('hex') : actual.toString('ascii'),
    matched: actual.equals(expected),
  };
}

async function inspectDevelopmentManifestBoundary(filePath, sha256, spec = DEVELOPMENT_ROUTE_SPEC) {
  if (String(sha256 || '').toLowerCase() !== EXACT_GAME_SHA256) {
    return {
      supported: false,
      verified: false,
      reason: 'The development-manifest map is available only for the exact verified game build.',
    };
  }

  const layout = await readPeLayout(filePath);
  const handle = await fsp.open(filePath, 'r');
  try {
    const signatures = {};
    for (const [name, record] of Object.entries(spec.signatures)) {
      signatures[name] = await verifyRecord(handle, layout, record);
    }
    const strings = {};
    for (const [name, record] of Object.entries(spec.strings)) {
      strings[name] = await verifyRecord(handle, layout, record, 'ascii');
    }
    const verified = Object.values(signatures).every((item) => item.matched)
      && Object.values(strings).every((item) => item.matched);

    return {
      supported: true,
      verified,
      mode: 'offline-read-only',
      signatures,
      strings,
      operatingEnvironment: {
        fieldOffset: '0xba0',
        consumer: { name: 'CONSUMER', crc32: '0xb668ab6b' },
        development: { name: 'DEVELOPMENT', crc32: '0x4dd5b3c2' },
        tool: { name: 'TOOL', crc32: '0x1666d365' },
        selection: 'The startup routine writes CONSUMER after the shipped 0A device mounts. It writes DEVELOPMENT only when that setup fails.',
        independentLaunchOrConfigSetterFound: false,
      },
      developmentProvider: {
        builtIntoRetailExecutable: true,
        activation: 'consumer-device-failure-fallback',
        roots: ['GAME:../DEBUG/', 'vcnfs:../DEBUG/'],
        prefix: 'vcnfs:',
        manifestNames: ['vcfast_load.manifest', 'vcnfs.manifest'],
        lookupModel: 'manifest-indexed',
        dynamicFolderScan: false,
      },
      verdict: {
        usableAsSafeLooseModSwitch: false,
        reason: 'The exact retail build exposes no independent environment setter; forcing this path requires breaking the normal 0A mount and enters the debug/network-file fallback.',
      },
      claims: {
        hookInstalled: false,
        processAttached: false,
        gameFilesModified: false,
      },
    };
  } finally {
    await handle.close();
  }
}

module.exports = {
  DEVELOPMENT_ROUTE_SPEC,
  inspectDevelopmentManifestBoundary,
};
