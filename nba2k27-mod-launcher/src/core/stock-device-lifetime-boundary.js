const fsp = require('node:fs/promises');
const {
  EXACT_GAME_SHA256,
  readPeLayout,
  rvaToFileOffset,
} = require('./resource-device-boundary');

const STOCK_VTABLE_TARGETS = Object.freeze([
  0x4c49340, 0x4c493e0, 0x164ff0, 0x4c493f0, 0x1580, 0x1550,
  0x1590, 0x15a0, 0x15c0, 0x1580, 0x1580, 0x15d0,
  0x15e0, 0x15c0, 0x15c0, 0x1580, 0x1580, 0x1580,
  0x15d0, 0x4c49310, 0x4c48df0, 0x4c49120, 0x4c49180, 0x4c49090,
  0x4c490d0, 0x4c48e00, 0x4c48e10, 0x4c48e20, 0x4c48e30, 0x4c48e40,
  0x4c49240, 0x4c48f00, 0x4c48f60, 0x4c48f10, 0x4c48f30, 0x4c48e50,
  0x4c48e60, 0x4c48e70, 0x4c48e80, 0x4c48e90, 0x4c48eb0, 0x4c48ed0,
  0x4c48fe0, 0x4c48f40, 0x4c48f50, 0x4c48ea0, 0x4c48ec0,
]);

const LIFETIME_SPEC = Object.freeze({
  baseObjectBytes: 0x48,
  baseVtableRva: 0x7f20f88,
  baseVtableTargets: STOCK_VTABLE_TARGETS,
  routines: Object.freeze([
    Object.freeze({
      name: 'staticPrefixDeviceConstruction',
      rva: 0x95a91,
      bytes: '488d0d3893160ae80333bb04488d05647356064889052593160a',
    }),
    Object.freeze({
      name: 'gameAllocator',
      rva: 0x4b1e5d0,
      bytes: '41574156565755534883ec484885d20f84a1000000',
    }),
    Object.freeze({
      name: 'gameDeallocator',
      rva: 0x4b1e840,
      bytes: '4885c9740a31d24531c0e9b1a01400c3',
    }),
    Object.freeze({
      name: 'baseNonDeletingDestructor',
      rva: 0x4c48ff0,
      bytes: '55564883ec28488d6c242048c74500feffffff4889ce',
    }),
    Object.freeze({
      name: 'developmentOwnerAllocation',
      rva: 0x524aa31,
      bytes: 'b9280200004531c041b9a6afb44fe88c3b8dff',
    }),
    Object.freeze({
      name: 'developmentPrimaryDeviceConstruction',
      rva: 0x524aa4d,
      bytes: '4889c14889c3488945f8e844e39fff',
    }),
    Object.freeze({
      name: 'developmentEmbeddedDeviceConstruction',
      rva: 0x524aa93,
      bytes: '488b4df84881c1d0000000e8fde29fff',
    }),
    Object.freeze({
      name: 'developmentDeletingDestructor',
      rva: 0x524ada0,
      bytes: '5556574883ec30488d6c243048c745f8feffffff89d74889ce',
    }),
  ]),
  storageExamples: Object.freeze({
    staticPrefixDeviceRva: 0xa1fed0,
    developmentOwnerBytes: 0x228,
    developmentEmbeddedDeviceOffset: 0xd0,
    gameAllocatorRva: 0x4b1e5d0,
    gameDeallocatorRva: 0x4b1e840,
  }),
});

async function readAtRva(handle, layout, rva, bytes) {
  const output = Buffer.alloc(bytes);
  const result = await handle.read(output, 0, bytes, rvaToFileOffset(layout, rva));
  if (result.bytesRead !== bytes) throw new Error(`Could not read RVA 0x${rva.toString(16)}.`);
  return output;
}

async function verifyBytes(handle, layout, record) {
  const expected = Buffer.from(record.bytes, 'hex');
  const actual = await readAtRva(handle, layout, record.rva, expected.length);
  return {
    name: record.name,
    rva: `0x${record.rva.toString(16)}`,
    expected: record.bytes,
    actual: actual.toString('hex'),
    matched: actual.equals(expected),
  };
}

async function readVtableTargets(handle, layout, vtableRva, slotCount) {
  const bytes = await readAtRva(handle, layout, vtableRva, slotCount * 8);
  return Array.from({ length: slotCount }, (_, index) => {
    const virtualAddress = bytes.readBigUInt64LE(index * 8);
    const rva = virtualAddress - layout.imageBase;
    if (rva < 0n || rva > 0xffffffffn) throw new Error(`Vtable slot ${index} points outside the image.`);
    return Number(rva);
  });
}

async function inspectStockDeviceLifetimeBoundary(filePath, sha256, spec = LIFETIME_SPEC) {
  if (String(sha256 || '').toLowerCase() !== EXACT_GAME_SHA256) {
    return {
      supported: false,
      verified: false,
      reason: 'The stock lifetime map is available only for the exact verified game build.',
    };
  }

  const layout = await readPeLayout(filePath);
  const handle = await fsp.open(filePath, 'r');
  try {
    const routines = [];
    for (const record of spec.routines) routines.push(await verifyBytes(handle, layout, record));
    const actualTargets = await readVtableTargets(
      handle,
      layout,
      spec.baseVtableRva,
      spec.baseVtableTargets.length,
    );
    const slots = actualTargets.map((targetRva, index) => ({
      index,
      offset: `0x${(index * 8).toString(16)}`,
      targetRva: `0x${targetRva.toString(16)}`,
      expectedRva: `0x${spec.baseVtableTargets[index].toString(16)}`,
      matched: targetRva === spec.baseVtableTargets[index],
    }));
    const routinesMatched = routines.every((item) => item.matched);
    const allSlotsMatched = slots.every((item) => item.matched);
    const verified = routinesMatched && allSlotsMatched && slots.length === 47;
    return {
      supported: true,
      verified,
      mode: 'offline-read-only',
      routines,
      stockVtable: {
        rva: `0x${spec.baseVtableRva.toString(16)}`,
        slotCount: slots.length,
        allSlotsMatched,
        slots,
      },
      ownership: {
        baseObjectBytes: spec.baseObjectBytes,
        storageModel: 'mixed',
        staticPrefixDeviceRva: `0x${spec.storageExamples.staticPrefixDeviceRva.toString(16)}`,
        developmentOwnerBytes: spec.storageExamples.developmentOwnerBytes,
        developmentEmbeddedDeviceOffset: spec.storageExamples.developmentEmbeddedDeviceOffset,
        gameAllocatorRva: `0x${spec.storageExamples.gameAllocatorRva.toString(16)}`,
        gameDeallocatorRva: `0x${spec.storageExamples.gameDeallocatorRva.toString(16)}`,
        conclusion: 'Stock devices may be static, embedded, or game-heap-owned; allocation provenance must survive until destruction.',
      },
      candidateClonePolicy: {
        purpose: 'offline minimum override policy for a launcher-owned delegating object',
        overrideSlots: [
          { index: 0, offset: '0x0', purpose: 'allocator-aware deleting destructor' },
          { index: 19, offset: '0x98', purpose: 'delegating-handle cleanup' },
          { index: 32, offset: '0x100', purpose: 'mod-first existence query' },
          { index: 33, offset: '0x108', purpose: 'mod-first open with stock fallback' },
        ],
        inheritedSlots: 43,
        baseIdAccessorInherited: true,
        stockDeletingDestructorAllowedForLauncherMemory: false,
        productionSafe: false,
      },
      claims: {
        stockVtableFullyMapped: allSlotsMatched && slots.length === 47,
        allocationBoundaryMapped: routinesMatched,
        foreignLifetimeProvenSafe: false,
        liveRegistrationEnabled: false,
      },
      safety: {
        gameOpened: false,
        processAttached: false,
        hooksInstalled: false,
        gameFilesModified: false,
      },
    };
  } finally {
    await handle.close();
  }
}

module.exports = {
  LIFETIME_SPEC,
  STOCK_VTABLE_TARGETS,
  inspectStockDeviceLifetimeBoundary,
  readVtableTargets,
};
