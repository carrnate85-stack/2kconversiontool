const fsp = require('node:fs/promises');

const EXACT_GAME_SHA256 = 'a870c4920985ee9bf9f3dba996d1bf41e5d65748274aadc00861bcf11b3b9ae5';

const BOUNDARY_SPEC = Object.freeze({
  wrapperExistsCall: Object.freeze({ rva: 0x4b48399, bytes: 'ff9000010000' }),
  resourceExistsById: Object.freeze({ rva: 0x4b480e0, bytes: '554157415641545657534881ecf00000' }),
  baseExists: Object.freeze({ rva: 0x4c48f60, bytes: '5657534881ec600100004889d64885c9' }),
  baseExistsOpenCall: Object.freeze({ rva: 0x4c48fb5, bytes: 'ff9008010000817c242ca8214550' }),
  openResultConstructor: Object.freeze({ rva: 0x4c49410, bytes: '48c70100000000c5f857c0c5fc114118' }),
  managerGetter: Object.freeze({ rva: 0x4b446d0, bytes: '55564883ec58488d6c245048c74500fe' }),
  managerSelect: Object.freeze({ rva: 0x4b45750, bytes: '4156565755534883ec20' }),
  managerRegister: Object.freeze({ rva: 0x4b44f90, bytes: '5541574156415541545657534881ec98000000' }),
  managerOpen: Object.freeze({ rva: 0x4b49270, bytes: '5541574156415541545657534881ec68' }),
  managerOpenDeviceCall: Object.freeze({ rva: 0x4b49b9e, bytes: '488b801801000044896c24308b' }),
  fileHandleConstructor: Object.freeze({ rva: 0x4b40070, bytes: '564883ec30c5f8297424204889ce48c7' }),
  fileHandleInitialize: Object.freeze({ rva: 0x4b409a0, bytes: '41565657534883ec284c89ce4c89c349' }),
  fileHandleRead: Object.freeze({ rva: 0x4b40c90, bytes: '4157415641554154565755534883ec58' }),
  fileHandleDestructor: Object.freeze({ rva: 0x4b40520, bytes: '55564883ec38488d6c243048c74500fe' }),
  win32StreamDevice: Object.freeze({
    vtableRva: 0x80b2f18,
    actualOpenSlotOffset: 0x118,
    actualOpenRva: 0x4ceb290,
    closeSlotOffset: 0x150,
    closeRva: 0x4ceb960,
    transferSlotOffset: 0x158,
    transferRva: 0x4cebbe0,
    alternateTransferSlotOffset: 0x160,
    alternateTransferRva: 0x4cebd10,
  }),
  lifecycle: Object.freeze([
    Object.freeze({ name: 'baseConstructor', rva: 0x4c48da0, bytes: '564883ec204889ce488d05d9812d0348' }),
    Object.freeze({ name: 'setDeviceId', rva: 0x4c49320, bytes: '4889d04585c0740c44894140' }),
    Object.freeze({ name: 'managerUnregister', rva: 0x4b453c0, bytes: '55564883ec38488d6c243048c74500fe' }),
    Object.freeze({ name: 'baseDestructor', rva: 0x4c49340, bytes: '5556574883ec30488d6c243048c745f8' }),
    Object.freeze({ name: 'baseNonDeletingDestructor', rva: 0x4c48ff0, bytes: '55564883ec28488d6c242048c74500fe' }),
  ]),
  rejectedStreamCall: Object.freeze({
    rva: 0x4d16c90,
    bytes: 'ff90a0000000',
    className: 'VCMEMORYSTREAM',
    vtableRva: 0x5cd3f78,
    slotOffset: 0xa0,
    targetRva: 0x4b7af20,
  }),
  devices: Object.freeze([
    Object.freeze({ name: 'VCPREFIXFILEDEVICE', vtableRva: 0x65fce08, existsRva: 0x4c48f60, openRva: 0x51cac90 }),
    Object.freeze({ name: 'VCFILEDEVICE', vtableRva: 0x7f20f88, existsRva: 0x4c48f60, openRva: 0x4c48f10 }),
    Object.freeze({ name: 'VCFAST_LOAD_FILE_DEVICE', vtableRva: 0x80aaf98, existsRva: 0x51a1e70, openRva: 0x51a2010 }),
    Object.freeze({ name: 'VCFAST_LOAD_GET_FILE_INFO_FILE_DEVICE', vtableRva: 0x80ab288, existsRva: 0x51a2f60, openRva: 0x51a2f90 }),
    Object.freeze({ name: 'VCDLC_FILE_DEVICE', vtableRva: 0x80b2ce8, existsRva: 0x4c48f60, openRva: 0x4ceaaf0 }),
    Object.freeze({ name: 'VCWIN32FILEDEVICE', vtableRva: 0x80b2f18, existsRva: 0x4c48f60, openRva: 0x4ceaaf0 }),
  ]),
});

async function readPeLayout(filePath) {
  const handle = await fsp.open(filePath, 'r');
  try {
    const dos = Buffer.alloc(64);
    if ((await handle.read(dos, 0, dos.length, 0)).bytesRead !== dos.length || dos.toString('ascii', 0, 2) !== 'MZ') {
      throw new Error('NBA2K27.exe is not a valid PE executable.');
    }
    const peOffset = dos.readUInt32LE(0x3c);
    const fileHeader = Buffer.alloc(24);
    if ((await handle.read(fileHeader, 0, fileHeader.length, peOffset)).bytesRead !== fileHeader.length
      || fileHeader.toString('ascii', 0, 4) !== 'PE\0\0') {
      throw new Error('NBA2K27.exe has an invalid PE header.');
    }
    const sectionCount = fileHeader.readUInt16LE(6);
    const optionalSize = fileHeader.readUInt16LE(20);
    const optional = Buffer.alloc(optionalSize);
    if ((await handle.read(optional, 0, optional.length, peOffset + 24)).bytesRead !== optional.length
      || optional.readUInt16LE(0) !== 0x20b) {
      throw new Error('NBA2K27.exe is not a supported 64-bit PE image.');
    }
    const imageBase = optional.readBigUInt64LE(24);
    const sectionTable = Buffer.alloc(sectionCount * 40);
    if ((await handle.read(sectionTable, 0, sectionTable.length, peOffset + 24 + optionalSize)).bytesRead !== sectionTable.length) {
      throw new Error('NBA2K27.exe has a truncated PE section table.');
    }
    const sections = [];
    for (let index = 0; index < sectionCount; index += 1) {
      const offset = index * 40;
      sections.push({
        name: sectionTable.toString('ascii', offset, offset + 8).replaceAll('\0', ''),
        virtualSize: sectionTable.readUInt32LE(offset + 8),
        virtualAddress: sectionTable.readUInt32LE(offset + 12),
        rawSize: sectionTable.readUInt32LE(offset + 16),
        rawOffset: sectionTable.readUInt32LE(offset + 20),
      });
    }
    return { imageBase, sections };
  } finally {
    await handle.close();
  }
}

function rvaToFileOffset(layout, rva) {
  const section = layout.sections.find((item) => (
    rva >= item.virtualAddress
    && rva < item.virtualAddress + Math.max(item.virtualSize, item.rawSize)
  ));
  if (!section) throw new Error(`RVA 0x${rva.toString(16)} is outside the PE sections.`);
  const relative = rva - section.virtualAddress;
  if (relative >= section.rawSize) throw new Error(`RVA 0x${rva.toString(16)} has no bytes in the PE file.`);
  return section.rawOffset + relative;
}

async function readAtRva(handle, layout, rva, bytes) {
  const output = Buffer.alloc(bytes);
  const result = await handle.read(output, 0, output.length, rvaToFileOffset(layout, rva));
  if (result.bytesRead !== output.length) throw new Error(`Could not read RVA 0x${rva.toString(16)}.`);
  return output;
}

async function verifyBytes(handle, layout, record) {
  const expected = Buffer.from(record.bytes, 'hex');
  const actual = await readAtRva(handle, layout, record.rva, expected.length);
  return {
    rva: `0x${record.rva.toString(16)}`,
    expected: record.bytes,
    actual: actual.toString('hex'),
    matched: actual.equals(expected),
  };
}

async function readVtableTarget(handle, layout, vtableRva, slotOffset) {
  const pointer = await readAtRva(handle, layout, vtableRva + slotOffset, 8);
  const virtualAddress = pointer.readBigUInt64LE(0);
  const rva = virtualAddress - layout.imageBase;
  if (rva < 0n || rva > 0xffffffffn) throw new Error('Vtable target is outside the supported image range.');
  return Number(rva);
}

async function inspectResourceDeviceBoundary(filePath, sha256, spec = BOUNDARY_SPEC) {
  if (String(sha256 || '').toLowerCase() !== EXACT_GAME_SHA256) {
    return {
      supported: false,
      verified: false,
      reason: 'The resource-device map is available only for the exact verified game build.',
    };
  }

  const layout = await readPeLayout(filePath);
  const handle = await fsp.open(filePath, 'r');
  try {
    const signatures = {
      wrapperExistsCall: await verifyBytes(handle, layout, spec.wrapperExistsCall),
      resourceExistsById: await verifyBytes(handle, layout, spec.resourceExistsById),
      baseExists: await verifyBytes(handle, layout, spec.baseExists),
      baseExistsOpenCall: await verifyBytes(handle, layout, spec.baseExistsOpenCall),
      openResultConstructor: await verifyBytes(handle, layout, spec.openResultConstructor),
      managerGetter: await verifyBytes(handle, layout, spec.managerGetter),
      managerSelect: await verifyBytes(handle, layout, spec.managerSelect),
      managerRegister: await verifyBytes(handle, layout, spec.managerRegister),
      ...(spec.managerOpen ? { managerOpen: await verifyBytes(handle, layout, spec.managerOpen) } : {}),
      ...(spec.managerOpenDeviceCall ? { managerOpenDeviceCall: await verifyBytes(handle, layout, spec.managerOpenDeviceCall) } : {}),
      ...(spec.fileHandleConstructor ? { fileHandleConstructor: await verifyBytes(handle, layout, spec.fileHandleConstructor) } : {}),
      ...(spec.fileHandleInitialize ? { fileHandleInitialize: await verifyBytes(handle, layout, spec.fileHandleInitialize) } : {}),
      ...(spec.fileHandleRead ? { fileHandleRead: await verifyBytes(handle, layout, spec.fileHandleRead) } : {}),
      ...(spec.fileHandleDestructor ? { fileHandleDestructor: await verifyBytes(handle, layout, spec.fileHandleDestructor) } : {}),
      rejectedStreamCall: await verifyBytes(handle, layout, spec.rejectedStreamCall),
    };
    const lifecycle = [];
    for (const record of spec.lifecycle || []) {
      lifecycle.push({ name: record.name, ...await verifyBytes(handle, layout, record) });
    }
    const rejectedTarget = await readVtableTarget(
      handle,
      layout,
      spec.rejectedStreamCall.vtableRva,
      spec.rejectedStreamCall.slotOffset,
    );
    const devices = [];
    for (const device of spec.devices) {
      const existsRva = await readVtableTarget(handle, layout, device.vtableRva, 0x100);
      const openRva = await readVtableTarget(handle, layout, device.vtableRva, 0x108);
      devices.push({
        name: device.name,
        vtableRva: `0x${device.vtableRva.toString(16)}`,
        existsRva: `0x${existsRva.toString(16)}`,
        openRva: `0x${openRva.toString(16)}`,
        matched: existsRva === device.existsRva && openRva === device.openRva,
      });
    }
    let managerOwnedStream = null;
    if (spec.win32StreamDevice) {
      const actualOpenRva = await readVtableTarget(
        handle, layout, spec.win32StreamDevice.vtableRva,
        spec.win32StreamDevice.actualOpenSlotOffset,
      );
      const closeRva = await readVtableTarget(
        handle, layout, spec.win32StreamDevice.vtableRva,
        spec.win32StreamDevice.closeSlotOffset,
      );
      const transferRva = await readVtableTarget(
        handle, layout, spec.win32StreamDevice.vtableRva,
        spec.win32StreamDevice.transferSlotOffset,
      );
      const alternateTransferRva = await readVtableTarget(
        handle, layout, spec.win32StreamDevice.vtableRva,
        spec.win32StreamDevice.alternateTransferSlotOffset,
      );
      managerOwnedStream = {
        device: 'VCWIN32FILEDEVICE',
        actualOpenSlot: `0x${spec.win32StreamDevice.actualOpenSlotOffset.toString(16)}`,
        actualOpenRva: `0x${actualOpenRva.toString(16)}`,
        closeSlot: `0x${spec.win32StreamDevice.closeSlotOffset.toString(16)}`,
        closeRva: `0x${closeRva.toString(16)}`,
        transferSlot: `0x${spec.win32StreamDevice.transferSlotOffset.toString(16)}`,
        transferRva: `0x${transferRva.toString(16)}`,
        alternateTransferSlot: `0x${spec.win32StreamDevice.alternateTransferSlotOffset.toString(16)}`,
        alternateTransferRva: `0x${alternateTransferRva.toString(16)}`,
        matched: actualOpenRva === spec.win32StreamDevice.actualOpenRva
          && closeRva === spec.win32StreamDevice.closeRva
          && transferRva === spec.win32StreamDevice.transferRva
          && alternateTransferRva === spec.win32StreamDevice.alternateTransferRva,
      };
    }
    const signatureValues = Object.values(signatures);
    const verified = signatureValues.every((item) => item.matched)
      && rejectedTarget === spec.rejectedStreamCall.targetRva
      && devices.every((item) => item.matched)
      && (!spec.win32StreamDevice || managerOwnedStream?.matched)
      && lifecycle.every((item) => item.matched);
    return {
      supported: true,
      verified,
      mode: 'offline-read-only',
      signatures,
      rejectedCandidate: {
        callRva: `0x${spec.rejectedStreamCall.rva.toString(16)}`,
        className: spec.rejectedStreamCall.className,
        reason: 'The +0xA0 target is a memory-stream serialization method, not resource dispatch.',
        targetRva: `0x${rejectedTarget.toString(16)}`,
        matched: rejectedTarget === spec.rejectedStreamCall.targetRva,
      },
      deviceDispatch: {
        existsSlot: '0x100',
        fileInfoSlot: '0x108',
        openSlot: '0x108',
        explicitIdWrapperRva: `0x${spec.resourceExistsById.rva.toString(16)}`,
        baseExistsRva: `0x${spec.baseExists.rva.toString(16)}`,
        baseExistsOpenCallRva: `0x${spec.baseExistsOpenCall.rva.toString(16)}`,
        openResultConstructorRva: `0x${spec.openResultConstructor.rva.toString(16)}`,
        openResultBytes: 0x130,
        managerSelectRva: `0x${spec.managerSelect.rva.toString(16)}`,
        managerRegisterRva: `0x${spec.managerRegister.rva.toString(16)}`,
        managerGetterRva: `0x${spec.managerGetter.rva.toString(16)}`,
        devices,
        managerOwnedStream,
      },
      registrationLifecycle: {
        verified: lifecycle.length > 0 && lifecycle.every((item) => item.matched),
        routines: lifecycle,
        deviceIdOffset: '0x40',
        intrusiveLinks: ['0x8', '0x10'],
        invariants: [
          'Device ID must be nonzero.',
          'A device must be self-linked before registration.',
          'Duplicate device IDs are rejected.',
          'The base destructor unregisters the device before final cleanup.',
        ],
        selectionPolicy: 'Unprefixed paths select the manager default device; registration alone does not create overlay precedence.',
      },
      claims: {
        managerLifecycleMapped: lifecycle.length > 0 && lifecycle.every((item) => item.matched),
        registrationProvenSafe: false,
        hookInstalled: false,
        resourceReplacementEnabled: false,
      },
    };
  } finally {
    await handle.close();
  }
}

module.exports = {
  BOUNDARY_SPEC,
  EXACT_GAME_SHA256,
  inspectResourceDeviceBoundary,
  readPeLayout,
  rvaToFileOffset,
};
