import assert from 'node:assert/strict';
import { unpackPreviewPacket } from '../worker/packet.mjs';
function packet(metadata, body = [0, 255, 128, 3]) {
  const header = new TextEncoder().encode(JSON.stringify(metadata));
  const bytes = new Uint8Array(12 + header.length + body.length);
  bytes.set(new TextEncoder().encode('CMTBIN01'));
  new DataView(bytes.buffer).setUint32(8, header.length, true);
  bytes.set(header, 12); bytes.set(body, 12 + header.length);
  return bytes.buffer;
}
const metadata = { packetKind: 'archive', native: true, geometryKey: 'geometry', contentKey: 'content',
  warnings: ['warning'], members: [{ name: 'a.dds', offset: 0, size: 2 }, { name: 'b.bin', offset: 2, size: 2 }] };
const source = packet(metadata), moved = structuredClone(source, { transfer: [source] });
assert.equal(source.byteLength, 0);
const decoded = unpackPreviewPacket(moved);
assert.deepEqual([...decoded.members[0].data], [0, 255]);
assert.deepEqual([...decoded.members[1].data], [128, 3]);
assert.equal(decoded.members[0].data.buffer, moved, 'Packet members must be views, not copies');
assert.equal(decoded.geometryKey, 'geometry');
assert.deepEqual(decoded.warnings, ['warning']);
assert(!('packetKind' in decoded));
assert.deepEqual([...unpackPreviewPacket(packet({ packetKind: 'file', size: 4 })).data], [0, 255, 128, 3]);
assert.equal(unpackPreviewPacket(packet({ packetKind: 'archive', members: [] }, [])).members.length, 0);
for (const rows of [
  [{ name: 'bad', offset: 1, size: 4 }],
  [{ name: 'bad', offset: 0, size: 5 }],
  [{ name: 'bad', offset: 0, size: -1 }],
  [{ name: 'bad', offset: 0, size: 1.1 }],
  [{ name: 'bad', offset: 0, size: 2 }, { name: 'bad', offset: 2, size: 2 }],
  [{ name: 'bad', offset: 0, size: 2 }],
]) assert.throws(() => unpackPreviewPacket(packet({ packetKind: 'archive', members: rows })));
assert.throws(() => unpackPreviewPacket(new ArrayBuffer(11)));
const badVersion = packet(metadata); new Uint8Array(badVersion)[0] = 0;
assert.throws(() => unpackPreviewPacket(badVersion));
const badHeader = packet(metadata); new DataView(badHeader).setUint32(8, 0xffffffff, true);
assert.throws(() => unpackPreviewPacket(badHeader));
assert.throws(() => unpackPreviewPacket(packet({ packetKind: 'file', size: 5 })));
assert.throws(() => unpackPreviewPacket(packet({ packetKind: 'other' })));
const rawGeometry = new Uint8Array(48), values = new DataView(rawGeometry.buffer);
[-0, .3333333432674408, 1].forEach((value,i) => values.setFloat32(i*4,value,true));
values.setFloat32(12,.25,true); values.setFloat32(16,.75,true);
values.setUint32(20,2,true); values.setUint32(24,65537,true);
[1,2,65535].forEach((value,i) => values.setUint16(28+i*2,value,true));
[99,100,101].forEach((value,i) => values.setFloat32(36+i*4,value,true));
const geometry = { packetKind:'catalog-geometry', assetBytes:36, baseBytes:12, stats:{headCacheHit:true},
  asset:{meshes:[{name:'asset',positions:{kind:'f32',offset:0,size:12},uvs:{kind:'f32',offset:12,size:8},indices:{kind:'u32',offset:20,size:4},sourceVertexIds:{kind:'u32',offset:24,size:4},skinBones:{kind:'u16',offset:28,size:6}}]},
  base:{meshes:[{name:'head',positions:{kind:'f32',offset:0,size:12}}]} };
geometry.note = '';
geometry.asset.meshes[0].textureUvSemantic = 'TEXCOORD1';
geometry.asset.meshes[0].attachmentUvSemantic = 'TEXCOORD0';
while ((12 + new TextEncoder().encode(JSON.stringify(geometry)).length) % 4 === 0) geometry.note += 'x';
const geometryPacket = packet(geometry,rawGeometry), result = unpackPreviewPacket(geometryPacket);
const mesh = result.asset.meshes[0], head = result.base.meshes[0];
assert.equal(mesh.textureUvSemantic, 'TEXCOORD1');
assert.equal(mesh.attachmentUvSemantic, 'TEXCOORD0');
assert(Object.is(mesh.positions[0],-0)); assert.equal(mesh.positions[1],.3333333432674408);
assert.deepEqual([...mesh.sourceVertexIds],[65537]); assert.deepEqual([...mesh.skinBones],[1,2,65535]);
assert.equal(mesh.positions.buffer,mesh.uvs.buffer); assert.equal(mesh.positions.buffer.byteLength,36);
assert.notEqual(mesh.positions.buffer,head.positions.buffer); assert.equal(head.positions.buffer.byteLength,12);
assert.notEqual(mesh.positions.buffer,geometryPacket);
mesh.positions[0]=9; assert.equal(head.positions[0],99); assert(Object.is(unpackPreviewPacket(geometryPacket).asset.meshes[0].positions[0],-0));
assert.equal(unpackPreviewPacket(packet({...geometry,base:null,baseBytes:0},rawGeometry.subarray(0,36))).base,null);
assert.equal(unpackPreviewPacket(packet({packetKind:'catalog-geometry',assetBytes:0,baseBytes:0,asset:{meshes:[]},base:null,stats:{}},[])).asset.meshes.length,0);
for (const change of [
  m=>{m.assetBytes=-1;},m=>{m.assetBytes=1.5;},m=>{m.baseBytes=Number.MAX_SAFE_INTEGER;},m=>{m.base=null;},
  m=>{m.asset.meshes={};},m=>{m.asset.meshes=Array(1001).fill({});},m=>{m.stats=[];},
  m=>{m.asset.meshes[0].positions.kind='u32';},m=>{m.asset.meshes[0].positions.offset=1;},
  m=>{m.asset.meshes[0].positions.size=11;},m=>{m.asset.meshes[0].uvs.offset=0;},
  m=>{m.asset.meshes[0].indices.size=1000000000;},m=>{m.asset.meshes[0].uvs.data='AAAA';},
]) {
  const malformed=structuredClone(geometry); change(malformed);
  assert.throws(()=>unpackPreviewPacket(packet(malformed,rawGeometry)));
}
const badPadding=rawGeometry.slice();badPadding[34]=1;
assert.throws(()=>unpackPreviewPacket(packet(geometry,badPadding)));
assert.throws(()=>unpackPreviewPacket(packet(geometry,rawGeometry.subarray(0,47))));
console.log('Preview packets: archive/file preservation, exact typed geometry, separate donor/asset ownership, alignment, bounds, types and malformed packet guards passed.');
