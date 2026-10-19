const path = require('node:path');
const fsp = require('node:fs/promises');
const { loadIff, parseBinaryTree, parseScneText, readZipEntry } = require('./iff');

const DEFAULT_MAX_TRIANGLES = 250000;
const DEFAULT_MAX_MODELS = 300;
const MAX_TEXTURE_BYTES = 16 * 1024 * 1024;
const MAX_GALLERY_TEXTURE_BYTES = 96 * 1024 * 1024;
const MAX_GALLERY_TEXTURES = 256;

function createArenaTextureBatch(requests, batchIndex = 0, batchSize = 64) {
  const textureNames = Array.isArray(requests) ? requests : [];
  const size = Math.max(1, Math.min(256, Math.floor(Number(batchSize) || 64)));
  const index = Math.max(0, Math.floor(Number(batchIndex) || 0));
  const total = textureNames.length;
  const batchCount = Math.ceil(total / size);
  const start = index * size;
  const end = Math.min(total, start + size);
  return {
    index,
    size,
    start,
    end,
    total,
    batchCount,
    nextBatch: end < total ? index + 1 : null,
    textureNames: start < total ? textureNames.slice(start, end) : [],
  };
}

function textureGalleryDisplayOptions() {
  return {
    // Use the same 2D viewing orientation for every DDS, not just portraits.
    // Only gallery sampling changes; DDS exports and model UVs stay untouched.
    flipY: true,
  };
}

function sceneNameVariants(value) {
  const text = String(value || '');
  const values = new Set([text, path.posix.basename(text.replaceAll('\\', '/'))]);
  if (text.includes(':')) values.add(text.slice(text.lastIndexOf(':') + 1));
  if (text.includes('@GameObjects@')) {
    const rest = text.split('@GameObjects@', 2)[1];
    values.add(rest);
    if (rest.includes(':')) {
      values.add(rest.split(':', 1)[0]);
      values.add(rest.slice(rest.lastIndexOf(':') + 1));
    }
  }
  return [...values].map((item) => String(item || '').trim().toLowerCase()).filter(Boolean);
}

function findSceneRoots(value, output = []) {
  if (!value || typeof value !== 'object') return output;
  if (!Array.isArray(value) && value.Object && value.Model
    && typeof value.Object === 'object' && typeof value.Model === 'object') output.push(value);
  for (const child of Object.values(value)) findSceneRoots(child, output);
  return output;
}

function addPlacement(lookup, variant, placement) {
  const bucket = lookup.get(variant) || [];
  if (!bucket.includes(placement)) bucket.push(placement);
  lookup.set(variant, bucket);
}

function cumulativeTransformTranslation(transformMap, transformName) {
  const entries = Object.entries(transformMap || {});
  let index = entries.findIndex(([name]) => name === transformName);
  const translation = [0, 0, 0];
  const visited = new Set();
  while (index >= 0 && index < entries.length && !visited.has(index)) {
    visited.add(index);
    const node = entries[index][1];
    if (!node || typeof node !== 'object') break;
    const value = Array.isArray(node.Translate) ? node.Translate : [];
    for (let axis = 0; axis < 3; axis += 1) translation[axis] += Number(value[axis] || 0);
    index = Number.isInteger(node.Parent) ? node.Parent : -1;
  }
  return translation;
}

function findTransformOffset(root, objectValue) {
  const objectTransform = String(objectValue?.Transform || '');
  if (!objectTransform.includes('|')) return [0, 0, 0];
  const pivotName = objectTransform.slice(objectTransform.lastIndexOf('|') + 1).toLowerCase();
  const assembly = objectTransform.split(':', 1)[0].toLowerCase();
  const candidates = [];
  for (const model of Object.values(root.Model || {})) {
    const transformMap = model?.Transform;
    if (!transformMap || typeof transformMap !== 'object') continue;
    for (const transformName of Object.keys(transformMap)) {
      if (transformName.slice(transformName.lastIndexOf('|') + 1).toLowerCase() !== pivotName) continue;
      const sameAssembly = transformName.split(':', 1)[0].toLowerCase() === assembly;
      const depth = transformName.split('|').length;
      const translation = cumulativeTransformTranslation(transformMap, transformName);
      const magnitude = translation.reduce((sum, value) => sum + Math.abs(value), 0);
      candidates.push({ sameAssembly, depth, magnitude, translation });
    }
  }
  candidates.sort((a, b) => Number(b.sameAssembly) - Number(a.sameAssembly)
    || b.depth - a.depth || b.magnitude - a.magnitude);
  return candidates[0]?.translation || [0, 0, 0];
}

function applyLocalTranslation(matrix, translation) {
  const [x, y, z] = translation;
  if (!x && !y && !z) return [...matrix];
  const result = [...matrix];
  result[12] = matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12];
  result[13] = matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13];
  result[14] = matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14];
  return result;
}

function buildSceneTransformLookup(scene) {
  const lookup = new Map();
  let objectMatrices = 0;
  for (const root of findSceneRoots(scene)) {
    for (const [objectKey, objectValue] of Object.entries(root.Object || {})) {
      const target = objectValue?.Target;
      const matrix = objectValue?.Matrix;
      if (!target || !Array.isArray(matrix) || matrix.length < 16) continue;
      const numericMatrix = matrix.slice(0, 16).map(Number);
      if (numericMatrix.some((value) => !Number.isFinite(value))) continue;
      const transformOffset = findTransformOffset(root, objectValue);
      const placement = {
        name: objectKey,
        target,
        matrix: applyLocalTranslation(numericMatrix, transformOffset),
        transform: String(objectValue.Transform || ''),
        transformOffset,
      };
      const variants = new Set([...sceneNameVariants(target), ...sceneNameVariants(objectKey)]);
      const model = root.Model?.[target];
      if (model && typeof model === 'object') {
        for (const primitive of model.Prim || []) {
          for (const key of ['Mesh', 'Material']) {
            for (const variant of sceneNameVariants(primitive?.[key])) variants.add(variant);
          }
        }
        for (const key of ['IndexBuffer', 'VertexStream']) {
          const values = Array.isArray(model[key]) ? model[key] : Object.values(model[key] || {});
          for (const value of values) {
            for (const variant of sceneNameVariants(value?.Binary)) variants.add(variant);
          }
        }
      }
      for (const variant of variants) addPlacement(lookup, variant, placement);
      objectMatrices += 1;
    }
  }
  return { lookup, objectMatrices };
}

function lookupScenePlacements(modelPath, model, lookup) {
  const candidates = new Set(sceneNameVariants(modelPath));
  for (const primitive of model.Prim || []) {
    for (const key of ['Mesh', 'Material']) {
      for (const variant of sceneNameVariants(primitive?.[key])) candidates.add(variant);
    }
  }
  for (const candidate of candidates) {
    const placements = lookup.get(candidate);
    if (placements?.length) return placements;
  }
  return [];
}

function gameMatrixToPreviewMatrix(matrix) {
  const m = matrix.slice(0, 16).map(Number);
  // B * M * B^-1, where B maps game XYZ to the preview's X,-Z,Y basis.
  // Three.js matrices are serialized column-major.
  return [
    m[0], -m[2], m[1], 0,
    -m[8], m[10], -m[9], 0,
    m[4], -m[6], m[5], 0,
    m[12], -m[14], m[13], 1,
  ];
}

function visit(value, pathName, callback) {
  if (!value || typeof value !== 'object') return;
  callback(value, pathName);
  if (Array.isArray(value)) {
    value.forEach((child, index) => visit(child, `${pathName}[${index}]`, callback));
  } else {
    for (const [name, child] of Object.entries(value)) visit(child, pathName ? `${pathName}/${name}` : name, callback);
  }
}

function halfToFloat(value) {
  const sign = (value & 0x8000) ? -1 : 1;
  const exponent = (value >>> 10) & 0x1f;
  const fraction = value & 0x3ff;
  if (exponent === 0) return sign * 2 ** -14 * (fraction / 1024);
  if (exponent === 31) return fraction ? Number.NaN : sign * Number.POSITIVE_INFINITY;
  return sign * 2 ** (exponent - 15) * (1 + fraction / 1024);
}

function formatInfo(format) {
  const normalized = String(format || '').toUpperCase();
  const match = /^R(\d+)(?:G(\d+))?(?:B(\d+))?(?:A(\d+))?_(UNORM|SNORM|FLOAT)$/.exec(normalized);
  if (!match) return null;
  const bits = match.slice(1, 5).filter(Boolean).map(Number);
  if (!bits.length || bits.some((value) => ![16, 32].includes(value))) return null;
  return { bits, type: match[5], bytes: bits.reduce((sum, value) => sum + value, 0) / 8 };
}

function readAttributeComponent(buffer, byteOffset, bits, type) {
  if (bits === 16 && type === 'UNORM') return buffer.readUInt16LE(byteOffset) / 65535;
  if (bits === 16 && type === 'SNORM') return Math.max(-1, buffer.readInt16LE(byteOffset) / 32767);
  if (bits === 16 && type === 'FLOAT') return halfToFloat(buffer.readUInt16LE(byteOffset));
  if (bits === 32 && type === 'FLOAT') return buffer.readFloatLE(byteOffset);
  if (bits === 32 && type === 'UNORM') return buffer.readUInt32LE(byteOffset) / 0xffffffff;
  if (bits === 32 && type === 'SNORM') return Math.max(-1, buffer.readInt32LE(byteOffset) / 0x7fffffff);
  return Number.NaN;
}

function decodeAttribute(buffer, stride, vertex, descriptor, components) {
  const normalizedFormat = String(descriptor.Format || '').toUpperCase();
  const base = vertex * stride + Number(descriptor.ByteOffset || 0);
  const offset = Array.isArray(descriptor.Offset) ? descriptor.Offset : [];
  const scale = Array.isArray(descriptor.Scale) ? descriptor.Scale : [];
  if (normalizedFormat === 'R21G21B22_UINT' && components === 3) {
    if (base < 0 || base + 8 > buffer.length) return null;
    const packed = buffer.readBigUInt64LE(base);
    const values = [
      Number(packed & 0x1fffffn),
      Number((packed >> 21n) & 0x1fffffn),
      Number((packed >> 42n) & 0x3fffffn),
    ];
    return values.map((value, index) => value * (Number.isFinite(scale[index]) ? scale[index] : 1)
      + (Number.isFinite(offset[index]) ? offset[index] : 0));
  }
  const info = formatInfo(descriptor.Format);
  if (!info || info.bits.length < components) return null;
  if (base < 0 || base + info.bytes > buffer.length) return null;
  const result = [];
  let cursor = base;
  for (let index = 0; index < components; index += 1) {
    let value = readAttributeComponent(buffer, cursor, info.bits[index], info.type);
    if (!Number.isFinite(value)) return null;
    value = value * (Number.isFinite(scale[index]) ? scale[index] : 1) + (Number.isFinite(offset[index]) ? offset[index] : 0);
    result.push(value);
    cursor += info.bits[index] / 8;
  }
  return result;
}

function vertexStreams(model) {
  const streams = model.VertexStream;
  if (Array.isArray(streams)) return streams;
  if (!streams || typeof streams !== 'object') return [];
  if (streams.VertexBuffer && typeof streams.VertexBuffer === 'object') return [streams.VertexBuffer];
  return Object.values(streams).filter((stream) => stream && typeof stream === 'object' && stream.Binary);
}

function selectPrimitiveRange(primitive, preferHighestDetail = false) {
  if (Array.isArray(primitive.LodList)) {
    const start = preferHighestDetail ? 0 : primitive.LodList.length - 1;
    const end = preferHighestDetail ? primitive.LodList.length : -1;
    const step = preferHighestDetail ? 1 : -1;
    for (let index = start; index !== end; index += step) {
      const lod = primitive.LodList[index];
      if (Number.isInteger(lod?.Start) && Number.isInteger(lod?.Count) && lod.Count >= 3) return lod;
    }
  }
  if (Number.isInteger(primitive.Start) && Number.isInteger(primitive.Count) && primitive.Count >= 3) {
    return { Start: primitive.Start, Count: primitive.Count };
  }
  // Some compact props (including gameplay balls) omit Start when their only
  // primitive begins at the first index. The game treats that missing field as 0.
  if (primitive.Start === undefined && Number.isInteger(primitive.Count) && primitive.Count >= 3) {
    return { Start: 0, Count: primitive.Count };
  }
  return null;
}

function readIndices(buffer, format) {
  const normalized = String(format || '').toUpperCase();
  if (normalized === 'R16_UINT') {
    const output = new Uint32Array(Math.floor(buffer.length / 2));
    for (let index = 0; index < output.length; index += 1) output[index] = buffer.readUInt16LE(index * 2);
    return output;
  }
  if (normalized === 'R32_UINT') {
    const output = new Uint32Array(Math.floor(buffer.length / 4));
    for (let index = 0; index < output.length; index += 1) output[index] = buffer.readUInt32LE(index * 4);
    return output;
  }
  return null;
}

function memberFor(iff, name) {
  if (!name) return null;
  const normalized = String(name).replaceAll('\\', '/').toLowerCase();
  const direct = iff.lookup.get(normalized) || iff.lookup.get(path.posix.basename(normalized));
  if (direct || !normalized.endsWith('.gz')) return direct || null;
  const decoded = `${normalized.slice(0, -3)}.bin`;
  return iff.lookup.get(decoded) || iff.lookup.get(path.posix.basename(decoded)) || null;
}

function textureBinding(name, source = '') {
  const value = path.posix.basename(String(name || '').replaceAll('\\', '/')).toLowerCase();
  const sourcePath = String(source || '').replaceAll('\\', '/').toLowerCase();
  const pcreateHairFins = /hair_underpaint_.+_colorhair_.+_fins_/.test(sourcePath);
  const pcreateHairUnderpaint = /(?:^|\/)hair_underpaint_/.test(sourcePath) && !pcreateHairFins;
  const pcreateFacialUnderpaint = /facialhair_.+_underpaint_/.test(sourcePath);
  const pcreateFacialHair = /(?:^|\/)(?:fh_|beard_|fullbeard_|fullgoatee_|full_scruff)/.test(sourcePath);
  const target = pcreateHairFins ? 'hair'
    : pcreateHairUnderpaint ? 'hair-underpaint'
      : pcreateFacialUnderpaint ? 'facialhair-underpaint'
        : pcreateFacialHair ? 'facialhair'
          : /eyewhite|iris_/.test(value) ? 'eye'
    : /(?:^|_)lash(?:_|\.)/.test(value) ? 'eyelash'
      : value.includes('facialhair') ? 'facialhair'
    : /(?:^|_)face_/.test(value) ? 'face'
      : /(?:^|_)hair_/.test(value) ? 'hair'
        : /(?:^|_)chest_/.test(value) ? 'chest'
          : /(?:^|_)torso_/.test(value) ? 'torso'
            : /(?:^|_)legs_/.test(value) ? 'legs'
              : value.startsWith('ball_') || /(?:^|\/)ball\/texture(?:\/|$)/.test(sourcePath) ? 'ball' : '';
  const sourceChannel = /_fins_color\.iff$|_(?:beard_[^/]+|fullbeard_[^/]+|fullgoatee_[^/]+)_color\.iff$/.test(sourcePath) ? 'color'
    : /_bump\.iff$|_underpaint_normal\.iff$/.test(sourcePath) ? 'normal'
      : /_fins_(?:tangent|lightmap|occlusion)\.iff$|_(?:bentnormal|coneangle|hairmask)\.iff$/.test(sourcePath) ? 'auxiliary' : '';
  const channel = sourceChannel || (/wrinkle|bentnormal|coneangle|hairmask|lightmap|tangent|occlusion/.test(value) ? 'auxiliary'
    : /normal/.test(value) ? 'normal'
      : /color|albedo|diffuse/.test(value) ? 'color' : 'auxiliary');
  return { target, channel };
}

function txtrRangeMetadata(metadata) {
  const min = Array.isArray(metadata?.Min) ? metadata.Min.slice(0, 4).map(Number) : [];
  const max = Array.isArray(metadata?.Max) ? metadata.Max.slice(0, 4).map(Number) : [];
  if (min.length < 4 || max.length < 4 || [...min, ...max].some((value) => !Number.isFinite(value))) return null;
  const rangeCompress = Number(metadata?.RangeCompress || 0);
  const mask = rangeCompress
    ? [0, 1, 2, 3].map((channel) => (rangeCompress & (1 << channel) ? 1 : 0))
    : [0, 1, 2, 3].map((channel) => (Math.abs(min[channel]) > 1e-7 || Math.abs(max[channel] - 1) > 1e-7 ? 1 : 0));
  return mask.some(Boolean) ? { min, max, mask } : null;
}

function textureTxtrMetadata(iff, textureName) {
  const leaf = path.posix.basename(String(textureName || '').replaceAll('\\', '/'));
  const logicalName = leaf.replace(/\.[0-9a-f]{16}\.dds$/i, '');
  if (logicalName === leaf) return null;
  const txtrEntry = memberFor(iff, `${logicalName}.TXTR`);
  if (!txtrEntry) return null;
  try {
    const tree = parseBinaryTree(readZipEntry(iff.buffer, txtrEntry, true));
    const metadata = tree && Object.values(tree)[0];
    const result = { format: String(metadata?.Format || '') };
    result.range = txtrRangeMetadata(metadata);
    return result;
  } catch {
    return null;
  }
}

function collectTextureMetadata(scene, output, container = 0) {
  visit(scene, '', (node, nodePath) => {
    if (typeof node.Binary !== 'string' || !/\.(?:dds|tld)$/i.test(node.Binary)) return;
    // mod.exe expands the TLD payload to DDS but intentionally leaves the SCNE
    // Binary reference unchanged. Normalize that declared name so themed maps
    // retain their SCNE source path and can be bound to the correct material.
    const decodedName = node.Binary.replace(/\.tld$/i, '.dds');
    const key = `${container}:${decodedName.toLowerCase()}`;
    if (output.has(key)) return;
    output.set(key, {
      name: decodedName,
      resourceName: node.Binary,
      source: nodePath,
      width: Number(node.Width || 0),
      height: Number(node.Height || 0),
      mips: Number(node.Mips || 0),
      format: String(node.Format || 'DDS'),
      binding: textureBinding(node.Binary, nodePath),
      container,
    });
  });
}

function collectTextureReferences(scene, output, container = 0) {
  visit(scene, '', (node) => {
    if (typeof node.Binary !== 'string' || !/\.(?:dds|tld)$/i.test(node.Binary)) return;
    output.add(`${container}:${node.Binary.replace(/\.tld$/i, '.dds').toLowerCase()}`);
  });
}

function arenaTextureChannel(resourcePath) {
  const value = String(resourcePath || '').toLowerCase();
  if (/albedo|diffuse|basecolou?r|colou?rmap/.test(value)) return 'color';
  if (/normal/.test(value)) return 'normal';
  if (/rough/.test(value)) return 'roughness';
  if (/metal/.test(value)) return 'metalness';
  if (/emissive/.test(value)) return 'emissive';
  return '';
}

function collectArenaMaterialTextures(scene, output, container = 0) {
  for (const root of findSceneRoots(scene)) {
    const textureLookup = new Map(Object.entries(root.Texture || {}).map(([name, texture]) => [name.toLowerCase(), texture]));
    for (const [material, definition] of Object.entries(root.Material || {})) {
      visit(definition, '', (node, nodePath) => {
        if (typeof node?.Pixelmap !== 'string') return;
        const channel = arenaTextureChannel(nodePath);
        if (!channel) return;
        const texture = textureLookup.get(node.Pixelmap.toLowerCase());
        if (!texture || typeof texture.Binary !== 'string' || !/\.(?:dds|tld)$/i.test(texture.Binary)) return;
        const decodedName = texture.Binary.replace(/\.tld$/i, '.dds');
        const key = `${container}:${decodedName.toLowerCase()}`;
        let item = output.get(key);
        if (!item) {
          item = {
            name: decodedName,
            resourceName: texture.Binary,
            source: node.Pixelmap,
            width: Number(texture.Width || 0),
            height: Number(texture.Height || 0),
            mips: Number(texture.Mips || 0),
            format: String(texture.Format || 'DDS'),
            binding: { target: 'arena', channel },
            assignments: [],
            container,
          };
          output.set(key, item);
        }
        if (!item.assignments.some((assignment) => assignment.material === material && assignment.channel === channel)) {
          item.assignments.push({ material, channel });
        }
      });
    }
  }
}

function firstStringValue(value, depth = 0) {
  if (typeof value === 'string') return value;
  if (!value || typeof value !== 'object' || depth > 3) return '';
  for (const child of Object.values(value)) {
    const result = firstStringValue(child, depth + 1);
    if (result) return result;
  }
  return '';
}

function firstMaterialShader(value, depth = 0) {
  if (!value || typeof value !== 'object' || depth > 6) return '';
  for (const [key, child] of Object.entries(value)) {
    if (/shader|technique|effect/i.test(key)) {
      const result = firstStringValue(child);
      if (result) return result;
    }
  }
  for (const child of Object.values(value)) {
    const result = firstMaterialShader(child, depth + 1);
    if (result) return result;
  }
  return '';
}

function collectMaterialMetadata(scene, output, sourceName = '') {
  for (const root of findSceneRoots(scene)) {
    for (const [name, definition] of Object.entries(root.Material || {})) {
      const key = name.toLowerCase();
      if (output.has(key)) continue;
      output.set(key, {
        name,
        shader: firstMaterialShader(definition),
        source: sourceName,
      });
    }
  }
}

function previewTextureScore(item) {
  const channels = item.assignments?.map((assignment) => assignment.channel) || [item.binding?.channel];
  const channelScore = channels.includes('color') ? 10 : channels.includes('normal') ? 5 : 1;
  const courtScore = /court|floor|hardwood|basket|scoreboard|scorecube/i.test(`${item.name} ${item.source}`) ? 8 : 0;
  return channelScore + courtScore;
}

function comparePreviewTextures(a, b) {
  return previewTextureScore(b) - previewTextureScore(a)
    || (b.assignments?.length || 0) - (a.assignments?.length || 0)
    || (a.width * a.height) - (b.width * b.height);
}

function primitiveMaterial(primitive, inherited = 'Unassigned') {
  const declared = String(primitive?.Material || '').trim();
  return declared || inherited || 'Unassigned';
}

function buildMesh(iff, model, modelPath, remainingTriangles, materialUsage, preferHighestDetail = false) {
  const position = model.VertexFormat?.POSITION0;
  const tangentSpaceAttachment = String(position?.Format || '').toUpperCase() === 'R16G16B16A16_UINT'
    && Boolean(model.VertexFormat?.POSITION1 && model.VertexFormat?.TEXCOORD0 && model.VertexFormat?.TEXCOORD1);
  const streams = vertexStreams(model);
  const positionStream = streams[Number(position?.Stream || 0)];
  const indexEntry = memberFor(iff, model.IndexBuffer?.Binary);
  const positionEntry = memberFor(iff, positionStream?.Binary);
  if (!position || !positionStream || !indexEntry || !positionEntry) return { reason: 'missing position or index buffer' };
  const positionFormat = formatInfo(position.Format);
  const stride = Number(positionStream.Stride || 0);
  if ((!positionFormat || positionFormat.bits.length < 3)
    && String(position.Format).toUpperCase() !== 'R21G21B22_UINT'
    && !tangentSpaceAttachment) {
    return { reason: `unsupported position format ${position.Format || '(none)'}` };
  }
  if (stride <= 0) return { reason: 'invalid position-stream stride' };

  const positionBytes = readZipEntry(iff.buffer, positionEntry, true);
  const indexBytes = readZipEntry(iff.buffer, indexEntry, true);
  const sourceIndices = readIndices(indexBytes, model.IndexBuffer.Format);
  if (!sourceIndices) return { reason: `unsupported index format ${model.IndexBuffer.Format || '(none)'}` };
  const vertexCount = Math.floor(positionBytes.length / stride);
  const uvDescriptor = tangentSpaceAttachment ? model.VertexFormat?.TEXCOORD1 : model.VertexFormat?.TEXCOORD0;
  const uvStream = uvDescriptor ? streams[Number(uvDescriptor.Stream || 0)] : null;
  const uvEntry = memberFor(iff, uvStream?.Binary);
  const uvBytes = uvEntry ? readZipEntry(iff.buffer, uvEntry, true) : null;
  const uvStride = Number(uvStream?.Stride || 0);
  const attachmentDescriptor = tangentSpaceAttachment ? model.VertexFormat?.TEXCOORD0 : null;
  const attachmentStream = attachmentDescriptor ? streams[Number(attachmentDescriptor.Stream || 0)] : null;
  const attachmentEntry = memberFor(iff, attachmentStream?.Binary);
  const attachmentBytes = attachmentEntry ? readZipEntry(iff.buffer, attachmentEntry, true) : null;
  const attachmentStride = Number(attachmentStream?.Stride || 0);
  const primitives = Array.isArray(model.Prim) ? model.Prim : [];
  const remap = new Map();
  const positions = [];
  const sourceVertexIds = [];
  const uvs = [];
  const attachmentUvs = [];
  const tangentOffsets = [];
  const tangentAnchorIds = [];
  const indices = [];
  const groups = [];
  let triangles = 0;
  let inheritedMaterial = 'Unassigned';

  for (const primitive of primitives) {
    inheritedMaterial = primitiveMaterial(primitive, inheritedMaterial);
    const range = selectPrimitiveRange(primitive, preferHighestDetail);
    if (!range || triangles >= remainingTriangles) continue;
    const available = Math.min(range.Count, sourceIndices.length - range.Start);
    const count = Math.min(available - (available % 3), (remainingTriangles - triangles) * 3);
    if (count < 3 || range.Start < 0) continue;
    const groupStart = indices.length;
    for (let cursor = 0; cursor < count; cursor += 3) {
      const triangle = [sourceIndices[range.Start + cursor], sourceIndices[range.Start + cursor + 1], sourceIndices[range.Start + cursor + 2]];
      if (triangle.some((vertex) => vertex >= vertexCount)) continue;
      for (const sourceVertex of triangle) {
        let targetVertex = remap.get(sourceVertex);
        if (targetVertex === undefined) {
          const decodedPosition = tangentSpaceAttachment
            ? [0, 0, 0]
            : decodeAttribute(positionBytes, stride, sourceVertex, position, 3);
          if (!decodedPosition) return { reason: `could not decode ${position.Format}` };
          targetVertex = remap.size;
          remap.set(sourceVertex, targetVertex);
          sourceVertexIds.push(sourceVertex);
          // The Blender importer in this workspace establishes the game-to-editor axis mapping.
          positions.push(decodedPosition[0], -decodedPosition[2], decodedPosition[1]);
          const decodedUv = uvBytes && uvStride > 0 ? decodeAttribute(uvBytes, uvStride, sourceVertex, uvDescriptor, 2) : null;
          // DDS compressed textures are uploaded without a WebGL Y flip. Keep the
          // game's native UV orientation here; applying Blender's import flip a
          // second time turns face atlases upside down on the player mesh.
          uvs.push(decodedUv?.[0] || 0, decodedUv?.[1] || 0);
          const decodedAttachment = attachmentBytes && attachmentStride > 0
            ? decodeAttribute(attachmentBytes, attachmentStride, sourceVertex, attachmentDescriptor, 2)
            : null;
          attachmentUvs.push(decodedAttachment?.[0] || 0, decodedAttachment?.[1] || 0);
          if (tangentSpaceAttachment) {
            const packedBase = sourceVertex * stride + Number(position.ByteOffset || 0);
            tangentOffsets.push(
              halfToFloat(positionBytes.readUInt16LE(packedBase)),
              halfToFloat(positionBytes.readUInt16LE(packedBase + 2)),
              halfToFloat(positionBytes.readUInt16LE(packedBase + 4)),
            );
            tangentAnchorIds.push(positionBytes.readUInt16LE(packedBase + 6));
          }
        }
        indices.push(targetVertex);
      }
    }
    const groupCount = indices.length - groupStart;
    if (groupCount) {
      const material = inheritedMaterial;
      groups.push({ start: groupStart, count: groupCount, material, mesh: String(primitive.Mesh || '') });
      materialUsage.set(material, (materialUsage.get(material) || 0) + 1);
      triangles += groupCount / 3;
    }
  }
  if (!indices.length) return { reason: 'no triangle-list primitive at the selected LOD' };
  return {
    mesh: {
      name: modelPath.split('/').at(-1) || modelPath,
      positions: new Float32Array(positions),
      sourceVertexIds: new Uint32Array(sourceVertexIds),
      uvs: new Float32Array(uvs),
      attachmentUvs: tangentSpaceAttachment ? new Float32Array(attachmentUvs) : null,
      tangentOffsets: tangentSpaceAttachment ? new Float32Array(tangentOffsets) : null,
      tangentAnchorIds: tangentSpaceAttachment ? new Uint32Array(tangentAnchorIds) : null,
      indices: new Uint32Array(indices),
      groups,
      bounds: { min: model.Min || null, max: model.Max || null, center: model.Center || null },
    },
    triangles,
  };
}

function buildFacialHairFitHead(mesh) {
  const headPattern = /face|head|(?:^|_)ears?(?:_|$)/i;
  const accessoryPattern = /hair|beard|headband/i;
  const remap = new Map();
  const positions = [];
  const sourceVertexIds = [];
  const uvs = [];
  const indices = [];
  const groups = [];

  const appendVertex = (sourceVertex) => {
    const existing = remap.get(sourceVertex);
    if (existing !== undefined) return existing;
    const targetVertex = remap.size;
    remap.set(sourceVertex, targetVertex);
    positions.push(
      mesh.positions[sourceVertex * 3],
      mesh.positions[sourceVertex * 3 + 1],
      mesh.positions[sourceVertex * 3 + 2],
    );
    sourceVertexIds.push(mesh.sourceVertexIds?.[sourceVertex] ?? sourceVertex);
    uvs.push(mesh.uvs[sourceVertex * 2] || 0, mesh.uvs[sourceVertex * 2 + 1] || 0);
    return targetVertex;
  };

  for (const group of mesh.groups || []) {
    if (!headPattern.test(group.material) || accessoryPattern.test(group.material)) continue;
    const groupStart = indices.length;
    for (let cursor = group.start; cursor < group.start + group.count; cursor += 1) {
      indices.push(appendVertex(mesh.indices[cursor]));
    }
    const groupCount = indices.length - groupStart;
    if (groupCount) groups.push({ ...group, start: groupStart, count: groupCount });
  }
  if (!indices.length) return null;
  return {
    ...mesh,
    positions: new Float32Array(positions),
    sourceVertexIds: new Uint32Array(sourceVertexIds),
    uvs: new Float32Array(uvs),
    indices: new Uint32Array(indices),
    groups,
    sourceKind: 'facialhair-fit-base',
  };
}

function projectTangentSpaceMeshes(preview, basePreview) {
  const baseMesh = (basePreview?.meshes || []).find((mesh) => (
    !mesh.attachmentUvs?.length
    && (mesh.groups || []).some((group) => /face|head|(?:^|_)ears?(?:_|$)/i.test(group.material))
  ));
  if (!baseMesh?.positions?.length || !baseMesh?.uvs?.length) return 0;

  const faceVertices = new Set();
  const normals = new Float32Array(baseMesh.positions.length);
  const tangents = new Float32Array(baseMesh.positions.length);
  for (const group of baseMesh.groups || []) {
    if (!/face|head|(?:^|_)ears?(?:_|$)/i.test(group.material)) continue;
    for (let cursor = group.start; cursor < group.start + group.count; cursor += 3) {
      const triangle = [baseMesh.indices[cursor], baseMesh.indices[cursor + 1], baseMesh.indices[cursor + 2]];
      triangle.forEach((vertex) => faceVertices.add(vertex));
      const [a, b, c] = triangle.map((vertex) => [
        baseMesh.positions[vertex * 3],
        baseMesh.positions[vertex * 3 + 1],
        baseMesh.positions[vertex * 3 + 2],
      ]);
      const ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
      const normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
      ];
      const [uvA, uvB, uvC] = triangle.map((vertex) => [
        baseMesh.uvs[vertex * 2],
        baseMesh.uvs[vertex * 2 + 1],
      ]);
      const duvAB = [uvB[0] - uvA[0], uvB[1] - uvA[1]];
      const duvAC = [uvC[0] - uvA[0], uvC[1] - uvA[1]];
      const divisor = duvAB[0] * duvAC[1] - duvAB[1] * duvAC[0];
      const tangent = Math.abs(divisor) > 1e-10
        ? [
          (ab[0] * duvAC[1] - ac[0] * duvAB[1]) / divisor,
          (ab[1] * duvAC[1] - ac[1] * duvAB[1]) / divisor,
          (ab[2] * duvAC[1] - ac[2] * duvAB[1]) / divisor,
        ]
        : ab;
      for (const vertex of triangle) {
        normals[vertex * 3] += normal[0];
        normals[vertex * 3 + 1] += normal[1];
        normals[vertex * 3 + 2] += normal[2];
        tangents[vertex * 3] += tangent[0];
        tangents[vertex * 3 + 1] += tangent[1];
        tangents[vertex * 3 + 2] += tangent[2];
      }
    }
  }
  if (!faceVertices.size) return 0;
  for (const vertex of faceVertices) {
    const offset = vertex * 3;
    const length = Math.hypot(normals[offset], normals[offset + 1], normals[offset + 2]) || 1;
    normals[offset] /= length;
    normals[offset + 1] /= length;
    normals[offset + 2] /= length;
    const tangentLength = Math.hypot(tangents[offset], tangents[offset + 1], tangents[offset + 2]) || 1;
    tangents[offset] /= tangentLength;
    tangents[offset + 1] /= tangentLength;
    tangents[offset + 2] /= tangentLength;
  }
  const sourceVertexLookup = new Map();
  baseMesh.sourceVertexIds?.forEach((sourceVertex, vertex) => sourceVertexLookup.set(sourceVertex, vertex));

  let projected = 0;
  for (const mesh of preview?.meshes || []) {
    if (!mesh.tangentOffsets?.length || !mesh.tangentAnchorIds?.length) continue;
    const positions = new Float32Array(mesh.tangentOffsets.length);
    let matched = 0;
    for (let vertex = 0; vertex < mesh.tangentAnchorIds.length; vertex += 1) {
      const match = sourceVertexLookup.get(mesh.tangentAnchorIds[vertex]);
      if (match === undefined) continue;
      const offset = vertex * 3;
      const anchor = match * 3;
      const localX = mesh.tangentOffsets[offset] * 64;
      const localY = mesh.tangentOffsets[offset + 1] * 64;
      const localZ = mesh.tangentOffsets[offset + 2] * 64;
      const tangent = [tangents[anchor], tangents[anchor + 1], tangents[anchor + 2]];
      const normal = [normals[anchor], normals[anchor + 1], normals[anchor + 2]];
      const bitangent = [
        normal[1] * tangent[2] - normal[2] * tangent[1],
        normal[2] * tangent[0] - normal[0] * tangent[2],
        normal[0] * tangent[1] - normal[1] * tangent[0],
      ];
      const bitangentLength = Math.hypot(...bitangent) || 1;
      bitangent[0] /= bitangentLength;
      bitangent[1] /= bitangentLength;
      bitangent[2] /= bitangentLength;
      for (let axis = 0; axis < 3; axis += 1) {
        positions[offset + axis] = baseMesh.positions[anchor + axis]
          + tangent[axis] * localX
          + bitangent[axis] * localY
          + normal[axis] * localZ;
      }
      matched += 1;
    }
    if (!matched) continue;
    mesh.positions = positions;
    mesh.tangentProjected = true;
    mesh.tangentProjectionMode = 'anchor-tangent-frame';
    projected += matched;
  }
  return projected;
}

function modelGeometrySignature(model) {
  let inheritedMaterial = 'Unassigned';
  const primitives = (Array.isArray(model.node?.Prim) ? model.node.Prim : []).map((primitive) => {
    inheritedMaterial = primitiveMaterial(primitive, inheritedMaterial);
    const range = selectPrimitiveRange(primitive, model.preferHighestDetail);
    return [inheritedMaterial, range?.Start ?? null, range?.Count ?? null];
  });
  const streams = (Array.isArray(model.node?.VertexStream) ? model.node.VertexStream : [model.node?.VertexStream])
    .filter(Boolean)
    .map((stream) => [String(stream.Binary || '').toLowerCase(), Number(stream.Stride || 0)]);
  const attribute = (descriptor) => descriptor
    ? [String(descriptor.Format || '').toUpperCase(), Number(descriptor.Stream || 0), Number(descriptor.ByteOffset || 0)]
    : null;
  return JSON.stringify([
    model.container,
    String(model.node?.IndexBuffer?.Binary || '').toLowerCase(),
    String(model.node?.IndexBuffer?.Format || '').toUpperCase(),
    streams,
    attribute(model.node?.VertexFormat?.POSITION0),
    attribute(model.node?.VertexFormat?.POSITION1),
    attribute(model.node?.VertexFormat?.TEXCOORD0),
    attribute(model.node?.VertexFormat?.TEXCOORD1),
    primitives,
    Boolean(model.preferHighestDetail),
  ]);
}

function deduplicateModels(models) {
  const unique = [];
  const byGeometry = new Map();
  for (const model of models) {
    const signature = modelGeometrySignature(model);
    const existing = byGeometry.get(signature);
    if (!existing) {
      const copy = { ...model, placements: [...model.placements], duplicateDefinitions: 0 };
      copy.placementKeys = new Set(copy.placements.map((placement) => JSON.stringify([
        placement.name,
        placement.target,
        placement.matrix,
      ])));
      byGeometry.set(signature, copy);
      unique.push(copy);
      continue;
    }
    existing.duplicateDefinitions += 1;
    for (const placement of model.placements) {
      const key = JSON.stringify([placement.name, placement.target, placement.matrix]);
      if (existing.placementKeys.has(key)) continue;
      existing.placementKeys.add(key);
      existing.placements.push(placement);
    }
  }
  unique.forEach((model) => { delete model.placementKeys; });
  return unique;
}

function shouldHideArenaModel(model, arenaLike = true) {
  return Boolean(arenaLike && /debug_helper_sphere/i.test(String(model?.path || '')));
}

async function createIffPreview(filePath, options = {}) {
  const iff = await loadIff(filePath);
  const companionSources = await Promise.all((options.companionPaths || []).map(async (companion) => ({
    name: companion.name || path.basename(companion.path),
    kind: companion.kind || 'companion',
    iff: await loadIff(companion.path),
  })));
  const textureSources = [{ name: path.basename(filePath), kind: 'main', iff }, ...companionSources];
  const maxTriangles = Math.max(1000, Math.min(1000000, options.maxTriangles || DEFAULT_MAX_TRIANGLES));
  const maxModels = Math.max(1, Math.min(1000, options.maxModels || DEFAULT_MAX_MODELS));
  const scenes = [];
  const models = [];
  const textures = new Map();
  const arenaTextureCandidates = new Map();
  const materialMetadata = new Map();
  const textureReferences = new Set();
  // Cached targeted-texture previews use an id/size filename, so arena identity
  // must not depend solely on the temporary file's basename.
  const arenaByName = Boolean(options.arena) || /^arena_.+_int\.iff$/i.test(path.basename(filePath));
  const includeArenaTextures = arenaByName && Boolean(options.includeArenaTextures);
  let objectMatrices = 0;
  let morphTargets = 0;
  let highDetailModels = 0;
  for (const [container, source] of textureSources.entries()) {
    if (source.kind !== 'main' && source.kind !== 'geometry') continue;
    for (const entry of source.iff.entries.filter((candidate) => /\.scne$/i.test(candidate.name))) {
      try {
        const scene = parseScneText(readZipEntry(source.iff.buffer, entry, true));
        scenes.push(container ? `${source.name}: ${entry.name}` : entry.name);
        collectMaterialMetadata(scene, materialMetadata, source.name);
        if (arenaByName) collectTextureReferences(scene, textureReferences, container);
        if (includeArenaTextures) collectArenaMaterialTextures(scene, textures, container);
        else if (arenaByName) collectArenaMaterialTextures(scene, arenaTextureCandidates, container);
        else collectTextureMetadata(scene, textures, container);
        const transforms = buildSceneTransformLookup(scene);
        objectMatrices += transforms.objectMatrices;
        visit(scene, entry.name, (node, nodePath) => {
          if (node?.IndexBuffer?.Binary && node.VertexStream && node?.VertexFormat?.POSITION0 && Array.isArray(node.Prim)) {
            const placements = lookupScenePlacements(nodePath, node, transforms.lookup);
            const modelMorphTargets = Object.keys(node.Morph || {}).length;
            const preferHighestDetail = Boolean(options.preferHighestDetail)
              || modelMorphTargets > 0 || /(?:hihead|blend_head|basebody)/i.test(nodePath);
            morphTargets += modelMorphTargets;
            if (preferHighestDetail) highDetailModels += 1;
            models.push({ container, iff: source.iff, node, path: nodePath, placements, preferHighestDetail });
          }
        });
      } catch {
        // Validation reports malformed SCNE files. A single scene should not suppress other previewable scenes.
      }
    }
  }

  const arenaTextureLeafKeys = new Map();
  if (includeArenaTextures) {
    for (const [key, texture] of textures) {
      arenaTextureLeafKeys.set(`${texture.container}:${path.posix.basename(texture.name).toLowerCase()}`, key);
    }
  }
  textureSources.forEach((source, container) => {
    for (const entry of source.iff.entries.filter((candidate) => /\.dds$/i.test(candidate.name))) {
      if (arenaByName && !includeArenaTextures) {
        textureReferences.add(`${container}:${entry.name.toLowerCase()}`);
        continue;
      }
      const key = `${container}:${entry.name.toLowerCase()}`;
      if (includeArenaTextures) {
        const mappedKey = textures.has(key)
          ? key
          : arenaTextureLeafKeys.get(`${container}:${path.posix.basename(entry.name).toLowerCase()}`);
        if (!mappedKey || textures.has(mappedKey)) continue;
      }
      if (textures.has(key)) continue;
      textures.set(key, {
        name: entry.name,
        resourceName: entry.name,
        source: `${source.kind}: ${source.name}`,
        width: 0,
        height: 0,
        mips: 0,
        format: 'DDS',
        binding: textureBinding(entry.name, source.name),
        container,
      });
    }
  });
  for (const [index, texture] of (options.externalTextures || []).entries()) {
    if (!texture?.path || !texture?.name) continue;
    textures.set(`external:${index}:${texture.name.toLowerCase()}`, {
      name: texture.name,
      resourceName: texture.resourceName || texture.name,
      source: texture.source || 'shared texture cache',
      width: 0,
      height: 0,
      mips: 0,
      format: texture.format || 'DDS',
      range: texture.range || null,
      binding: texture.binding || textureBinding(texture.name),
      externalPath: texture.path,
      container: null,
    });
  }

  const meshes = [];
  const skipped = [];
  const materialUsage = new Map();
  let triangles = 0;
  const arenaLike = arenaByName || objectMatrices >= 20;
  const visibleModels = models.filter((model) => !shouldHideArenaModel(model, arenaLike));
  const hiddenModels = models.length - visibleModels.length;
  const uniqueModels = deduplicateModels(visibleModels);
  for (const model of uniqueModels.slice(0, maxModels)) {
    if (triangles >= maxTriangles) break;
    try {
      const result = buildMesh(
        model.iff,
        model.node,
        model.path,
        maxTriangles - triangles,
        materialUsage,
        model.preferHighestDetail,
      );
      if (result.mesh) {
        if (options.headShaderOnly) {
          result.mesh = buildFacialHairFitHead(result.mesh);
          result.triangles = result.mesh ? result.mesh.indices.length / 3 : 0;
        }
        if (!result.mesh) continue;
        if (arenaLike && model.placements.length) {
          result.mesh.instanceMatrices = new Float32Array(model.placements.length * 16);
          result.mesh.instanceNames = new Array(model.placements.length);
          result.mesh.instanceTargets = new Array(model.placements.length);
          model.placements.forEach((placement, index) => {
            result.mesh.instanceMatrices.set(gameMatrixToPreviewMatrix(placement.matrix), index * 16);
            result.mesh.instanceNames[index] = placement.name;
            result.mesh.instanceTargets[index] = placement.target;
          });
        } else {
          result.mesh.instances = model.placements.map((placement) => ({
            name: placement.name,
            target: placement.target,
            matrix: gameMatrixToPreviewMatrix(placement.matrix),
          }));
        }
        meshes.push(result.mesh);
        triangles += result.triangles;
      } else if (skipped.length < 30) skipped.push(`${model.path}: ${result.reason}`);
    } catch (error) {
      if (skipped.length < 30) skipped.push(`${model.path}: ${error.message}`);
    }
  }

  const textureList = [...textures.values()];
  for (const texture of textureList) {
    const metadata = texture.container === null
      ? null
      : textureTxtrMetadata(textureSources[texture.container].iff, texture.name);
    if (metadata?.format) texture.format = metadata.format;
    texture.range = metadata?.range || texture.range || null;
  }
  const prioritized = [...textureList].sort(comparePreviewTextures);
  const texturePayloads = [];
  let textureBytes = 0;
  const maxTextureBytes = includeArenaTextures ? 64 * 1024 * 1024 : MAX_TEXTURE_BYTES;
  const maxTextureCount = includeArenaTextures ? 128 : 24;
  for (const texture of (options.skipTextures ? [] : prioritized)) {
    try {
      let bytes;
      if (texture.externalPath) {
        bytes = await fsp.readFile(texture.externalPath);
      } else {
        const source = textureSources[texture.container];
        const entry = memberFor(source.iff, texture.name);
        if (!entry || entry.size > 8 * 1024 * 1024) continue;
        bytes = readZipEntry(source.iff.buffer, entry, true);
      }
      if (bytes.length > 8 * 1024 * 1024 || textureBytes + bytes.length > maxTextureBytes) continue;
      const { container, externalPath, ...publicTexture } = texture;
      texturePayloads.push({ ...publicTexture, bytes: bytes.toString('base64') });
      textureBytes += bytes.length;
      if (texturePayloads.length >= maxTextureCount) break;
    } catch {
      // Invalid DDS payloads are already included in validation.
    }
  }

  const instanceCount = meshes.reduce((count, mesh) => {
    const packed = Math.floor((mesh.instanceMatrices?.length || 0) / 16);
    return count + Math.max(1, packed || mesh.instances?.length || 0);
  }, 0);
  if (morphTargets > 0) {
    skipped.unshift(`${morphTargets} facial morph target(s) detected. The preview currently shows the neutral base mesh; NBA 2K's math-node morph evaluation is not applied yet.`);
  }
  const arenaTextureRequests = arenaByName && !includeArenaTextures
    ? [...arenaTextureCandidates.values()]
      .sort(comparePreviewTextures)
      .map((texture) => texture.name.replace(/\.dds$/i, '.tld'))
    : [];
  const materialTextureCandidateMaterials = new Set(
    [...(includeArenaTextures ? textures : arenaTextureCandidates).values()]
      .flatMap((texture) => texture.assignments || [])
      .map((assignment) => String(assignment.material || '').toLowerCase())
      .filter(Boolean),
  ).size;
  const texturedMaterials = new Set(texturePayloads.flatMap((texture) => (
    texture.assignments || []
  ).map((assignment) => String(assignment.material || '').toLowerCase())).filter(Boolean)).size;
  const dependencyTextures = arenaByName && !includeArenaTextures
    ? [...arenaTextureCandidates.values()]
    : textureList;
  const loadedTextureNames = new Set(texturePayloads.map((texture) => String(texture.name || '').toLowerCase()));
  const textureDependencies = dependencyTextures.map((texture) => {
    const { container, externalPath, ...publicTexture } = texture;
    return {
      ...publicTexture,
      sourceIff: container === null
        ? String(texture.source || 'shared texture cache')
        : textureSources[container]?.name || path.basename(filePath),
      loaded: loadedTextureNames.has(String(texture.name || '').toLowerCase()),
    };
  });

  return {
    file: filePath,
    scenes,
    meshes,
    stats: {
      modelsFound: models.length,
      modelsHidden: hiddenModels,
      modelsUnique: uniqueModels.length,
      modelsDeduplicated: visibleModels.length - uniqueModels.length,
      modelsDecoded: meshes.length,
      instances: instanceCount,
      objectMatrices,
      triangles,
      morphTargets,
      highDetailModels,
      materials: materialUsage.size,
      textures: arenaByName
        ? (includeArenaTextures ? textureList.length : textureReferences.size)
        : textureList.length,
      previewTextures: texturePayloads.length,
      texturedMaterials,
      materialTextureCandidates: includeArenaTextures ? textureList.length : arenaTextureCandidates.size,
      materialTextureCandidateMaterials,
      requestedArenaTextures: arenaTextureRequests.length,
      companions: companionSources.length,
      externalTextures: (options.externalTextures || []).length,
      skippedModels: Math.max(0, uniqueModels.length - meshes.length),
      truncated: uniqueModels.length > maxModels || triangles >= maxTriangles,
      arena: arenaLike,
    },
    materials: [...materialUsage].map(([name, count]) => ({
      name,
      primitives: count,
      shader: materialMetadata.get(name.toLowerCase())?.shader || '',
      source: materialMetadata.get(name.toLowerCase())?.source || '',
    })),
    textures: texturePayloads,
    textureDependencies,
    arenaTextureRequests,
    skipped,
  };
}

function ddsHeaderMetadata(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 128 || bytes.toString('ascii', 0, 4) !== 'DDS ') return null;
  const height = bytes.readUInt32LE(12);
  const width = bytes.readUInt32LE(16);
  const mips = Math.max(1, bytes.readUInt32LE(28));
  const fourCC = bytes.toString('ascii', 84, 88).replace(/\0/g, '');
  let format = fourCC || 'DDS';
  if (fourCC === 'DX10' && bytes.length >= 148) {
    const dxgi = bytes.readUInt32LE(128);
    format = ({
      28: 'RGBA8_UNORM', 29: 'RGBA8_SRGB',
      71: 'BC1_UNORM', 72: 'BC1_SRGB',
      74: 'BC2_UNORM', 75: 'BC2_SRGB',
      77: 'BC3_UNORM', 78: 'BC3_SRGB',
      80: 'BC4_UNORM', 81: 'BC4_SNORM',
      83: 'BC5_UNORM', 84: 'BC5_SNORM',
      95: 'BC6H_UF16', 96: 'BC6H_SF16',
      98: 'BC7_UNORM', 99: 'BC7_SRGB',
    })[dxgi] || `DXGI_${dxgi}`;
  }
  return { width, height, mips, format };
}

async function createIffTextureGallery(filePath, options = {}) {
  const iff = await loadIff(filePath);
  const maxTextures = Math.max(1, Math.min(1000, options.maxTextures || MAX_GALLERY_TEXTURES));
  const maxBytes = Math.max(1024 * 1024, Math.min(512 * 1024 * 1024, options.maxBytes || MAX_GALLERY_TEXTURE_BYTES));
  const textures = [];
  let textureBytes = 0;
  let skippedOversize = 0;
  const entries = iff.entries.filter((entry) => /\.dds$/i.test(entry.name));
  for (const entry of entries) {
    if (textures.length >= maxTextures) break;
    if (entry.size > 16 * 1024 * 1024 || textureBytes + entry.size > maxBytes) {
      skippedOversize += 1;
      continue;
    }
    try {
      const bytes = readZipEntry(iff.buffer, entry, true);
      const header = ddsHeaderMetadata(bytes);
      if (!header) continue;
      const txtr = textureTxtrMetadata(iff, entry.name);
      textures.push({
        name: entry.name,
        width: header.width,
        height: header.height,
        mips: header.mips,
        format: txtr?.format || header.format,
        range: txtr?.range || null,
        bytes: bytes.toString('base64'),
      });
      textureBytes += bytes.length;
    } catch {
      // Keep browsing other textures when one member is malformed.
    }
  }
  return {
    file: filePath,
    textures,
    stats: {
      found: entries.length,
      loaded: textures.length,
      bytes: textureBytes,
      skipped: Math.max(0, entries.length - textures.length),
      skippedOversize,
      truncated: textures.length < entries.length,
    },
  };
}

module.exports = {
  createArenaTextureBatch,
  textureGalleryDisplayOptions,
  buildFacialHairFitHead,
  projectTangentSpaceMeshes,
  buildSceneTransformLookup,
  applyLocalTranslation,
  createIffPreview,
  createIffTextureGallery,
  ddsHeaderMetadata,
  decodeAttribute,
  formatInfo,
  findTransformOffset,
  gameMatrixToPreviewMatrix,
  halfToFloat,
  lookupScenePlacements,
  readIndices,
  sceneNameVariants,
  selectPrimitiveRange,
  textureBinding,
  collectTextureMetadata,
  collectTextureReferences,
  collectArenaMaterialTextures,
  previewTextureScore,
  comparePreviewTextures,
  deduplicateModels,
  modelGeometrySignature,
  primitiveMaterial,
  shouldHideArenaModel,
  textureTxtrMetadata,
  txtrRangeMetadata,
};
