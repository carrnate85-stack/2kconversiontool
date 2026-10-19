const HEADER_BYTES = 0x10;
const RECORD_BYTES = 0x10;

function pointerTarget(fieldOffset, relative) {
  if (relative === 0n) return null;
  const target = BigInt(fieldOffset) + relative - 1n;
  if (target < 0n || target > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error('Signature-config pointer is outside the safe file range.');
  }
  return Number(target);
}

function relativePointer(fieldOffset, targetOffset) {
  const value = BigInt(targetOffset) - BigInt(fieldOffset) + 1n;
  if (value <= 0n) throw new Error('Signature-config pointers must remain forward-relative.');
  return value;
}

function align16(value) {
  return (value + 0x0f) & ~0x0f;
}

function readHeader(source) {
  if (!Buffer.isBuffer(source) || source.length < HEADER_BYTES) {
    throw new Error('Signature-config table is shorter than its header.');
  }
  const count = source.readUInt32LE(0);
  const maximumConfigs = source.readUInt32LE(8);
  const recordsEnd = HEADER_BYTES + count * RECORD_BYTES;
  if (count < 1 || recordsEnd > source.length || maximumConfigs < 1 || maximumConfigs > 64) {
    throw new Error('Signature-config table header is invalid.');
  }
  return { count, maximumConfigs, recordsEnd };
}

function readRecords(source) {
  const header = readHeader(source);
  const records = [];
  for (let index = 0; index < header.count; index += 1) {
    const offset = HEADER_BYTES + index * RECORD_BYTES;
    records.push({
      id: source.readUInt32LE(offset),
      configCount: source.readUInt32LE(offset + 4),
      relative: source.readBigUInt64LE(offset + 8),
      offset,
    });
  }
  for (let index = 1; index < records.length; index += 1) {
    if (records[index - 1].id >= records[index].id) {
      throw new Error('Signature-config table IDs are not strictly sorted.');
    }
  }
  return { ...header, records };
}

function recordMap(source) {
  const parsed = readRecords(source);
  return new Map(parsed.records.map((record) => [record.id, record]));
}

// Config strings are uppercase CRC32 values behind relative pointers.
// Append private lists/payloads; never overwrite shared stock string storage.
function replaceConfigLists(source, replacements) {
  const { crc32 } = require('./jukebox-manager');
  const records = recordMap(source);
  const output = Buffer.alloc(align16(source.length) + replacements.reduce((n, r) => n + r.names.length * 32, 0));
  source.copy(output);
  let cursor = align16(source.length);
  const seen = new Set();
  for (const { id, names } of replacements) {
    const record = records.get(id);
    if (!record || seen.has(id) || !names.length || names.length > 9) throw new Error('Invalid config replacement.');
    seen.add(id);
    const normalized = names.map(name => String(name).toUpperCase());
    if (normalized.some(name => !/^[A-Z0-9_-]+$/.test(name))) throw new Error('Invalid config name.');
    const hashes = normalized.map(name => crc32(Buffer.from(name)));
    if (new Set(hashes).size !== names.length) throw new Error('Duplicate config names or hash collision.');
    const list = cursor, payload = list + names.length * 16;
    hashes.forEach((hash, i) => {
      const item = list + i * 16, target = payload + i * 16;
      output.writeBigUInt64LE(relativePointer(item, target), item);
      output.writeUInt32LE(hash, target);
    });
    output.writeUInt32LE(names.length, record.offset + 4);
    output.writeBigUInt64LE(relativePointer(record.offset + 8, list), record.offset + 8);
    cursor = payload + names.length * 16;
  }
  return replacements.length ? output.subarray(0, cursor) : Buffer.from(source);
}

function buildTableWithClones(source, clones) {
  const parsed = readRecords(source);
  const requested = [...clones]
    .map(({ targetId, donorId }) => ({ targetId: Number(targetId), donorId: Number(donorId) }))
    .sort((a, b) => a.targetId - b.targetId);
  if (!requested.length) return { data: Buffer.from(source), added: [], maximumConfigs: parsed.maximumConfigs };

  const originalById = new Map(parsed.records.map((record) => [record.id, record]));
  const targetIds = new Set();
  for (const item of requested) {
    if (!Number.isInteger(item.targetId) || item.targetId < 0 || item.targetId > 99999) {
      throw new Error(`Invalid five-digit PNG ID: ${item.targetId}`);
    }
    if (targetIds.has(item.targetId) || originalById.has(item.targetId)) {
      throw new Error(`Signature-config ID ${item.targetId} already exists.`);
    }
    targetIds.add(item.targetId);
    const donor = originalById.get(item.donorId);
    if (!donor || donor.configCount < 1 || donor.configCount > parsed.maximumConfigs) {
      throw new Error(`Donor ${item.donorId} is not a valid config record.`);
    }
  }

  const merged = [
    ...parsed.records.map((record) => ({ kind: 'original', id: record.id, record })),
    ...requested.map((item) => ({ kind: 'clone', id: item.targetId, ...item })),
  ].sort((a, b) => a.id - b.id);
  const shift = requested.length * RECORD_BYTES;
  const newRecordsEnd = parsed.recordsEnd + shift;
  const donorCounts = requested.map((item) => originalById.get(item.donorId).configCount);
  const estimatedAppend = donorCounts.reduce((sum, count) => sum + count * 32 + 15, 0);
  const output = Buffer.alloc(source.length + shift + estimatedAppend);
  source.copy(output, newRecordsEnd, parsed.recordsEnd);
  source.copy(output, 0, 0, HEADER_BYTES);
  output.writeUInt32LE(merged.length, 0);

  const newOffsetById = new Map();
  merged.forEach((item, index) => {
    const newOffset = HEADER_BYTES + index * RECORD_BYTES;
    newOffsetById.set(item.id, newOffset);
    if (item.kind === 'clone') {
      output.writeUInt32LE(item.id, newOffset);
      return;
    }
    const old = item.record;
    const oldTarget = pointerTarget(old.offset + 8, old.relative);
    const newTarget = oldTarget + shift;
    output.writeUInt32LE(old.id, newOffset);
    output.writeUInt32LE(old.configCount, newOffset + 4);
    output.writeBigUInt64LE(relativePointer(newOffset + 8, newTarget), newOffset + 8);
  });

  let cursor = align16(source.length + shift);
  const added = [];
  for (const item of requested) {
    const donorOffset = newOffsetById.get(item.donorId);
    const targetOffset = newOffsetById.get(item.targetId);
    const configCount = output.readUInt32LE(donorOffset + 4);
    const donorList = pointerTarget(donorOffset + 8, output.readBigUInt64LE(donorOffset + 8));
    const privateList = align16(cursor);
    const privatePayloads = privateList + configCount * 16;
    for (let configIndex = 0; configIndex < configCount; configIndex += 1) {
      const donorItem = donorList + configIndex * 16;
      const donorRelative = output.readBigUInt64LE(donorItem);
      const donorExactTarget = pointerTarget(donorItem, donorRelative);
      const donorBlock = donorExactTarget & ~0x0f;
      const intraBlockOffset = donorExactTarget - donorBlock;
      const privateItem = privateList + configIndex * 16;
      const privateBlock = privatePayloads + configIndex * 16;
      output.writeBigUInt64LE(
        relativePointer(privateItem, privateBlock + intraBlockOffset),
        privateItem,
      );
      output.writeBigUInt64LE(output.readBigUInt64LE(donorItem + 8), privateItem + 8);
      output.copy(output, privateBlock, donorBlock, donorBlock + 16);
    }
    output.writeUInt32LE(item.targetId, targetOffset);
    output.writeUInt32LE(configCount, targetOffset + 4);
    output.writeBigUInt64LE(relativePointer(targetOffset + 8, privateList), targetOffset + 8);
    cursor = privatePayloads + configCount * 16;
    added.push({
      targetId: item.targetId,
      donorId: item.donorId,
      configCount,
      privateList,
    });
  }

  const data = output.subarray(0, cursor);
  const verified = readRecords(data);
  if (verified.count !== merged.length) throw new Error('Generated record count did not validate.');
  const listTargets = new Set();
  for (const record of verified.records) {
    const list = pointerTarget(record.offset + 8, record.relative);
    if (list === null || list < verified.recordsEnd || list + record.configCount * 16 > data.length) {
      throw new Error(`Generated list pointer for PNG ${record.id} is invalid.`);
    }
    if (listTargets.has(list)) throw new Error(`Generated list for PNG ${record.id} is not privately owned.`);
    listTargets.add(list);
  }
  return { data, added, maximumConfigs: parsed.maximumConfigs };
}

module.exports = {
  HEADER_BYTES,
  RECORD_BYTES,
  buildTableWithClones,
  pointerTarget,
  readHeader,
  readRecords,
  recordMap,
  replaceConfigLists,
  relativePointer,
};
