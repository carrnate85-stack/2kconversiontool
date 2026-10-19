'use strict';
// Single native roster, distinct from mapped-fields JSON and encrypted backups.
// Pure codec only: no filesystem, network or import operation.
const crypto = require('node:crypto');
const bundleCodec = require('./native-roster-bundle');
const MAGIC = Buffer.from('2KRSTR2\n'), HEADER = 44, MAX_METADATA = 4096;
const MAX_BYTES = HEADER + MAX_METADATA + bundleCodec.MAX_INPUT;
const sha = data => crypto.createHash('sha256').update(data).digest();
function metadata(value) {
  if (!value || value.format !== '2k-native-roster' || value.version !== 2 || value.game !== 'NBA2K27' || value.saveType !== 1)
    throw Error('Unsupported native roster format.');
  const keys = ['format','version','game','saveType','name','gameBuildSha256','bundleBytes','bundleSha256'];
  if (Object.keys(value).length !== keys.length || Object.keys(value).some(key => !keys.includes(key)))
    throw Error('Unexpected roster metadata.');
  if (typeof value.name !== 'string' || !value.name.trim() || value.name.length > 63 || /[\x00-\x1f\x7f]/u.test(value.name))
    throw Error('Invalid roster name.');
  for (const key of ['gameBuildSha256','bundleSha256'])
    if (typeof value[key] !== 'string' || !/^[a-f0-9]{64}$/.test(value[key])) throw Error('Invalid roster fingerprint.');
  if (!Number.isInteger(value.bundleBytes) || value.bundleBytes < 4096 || value.bundleBytes > bundleCodec.MAX_INPUT)
    throw Error('Invalid native roster size.');
  return value;
}
function create({bundle, name, gameBuildSha256}) {
  const report = bundleCodec.inspect(bundle);
  const manifest = metadata({format:'2k-native-roster',version:2,game:'NBA2K27',saveType:1,
    name,gameBuildSha256,bundleBytes:bundle.length,bundleSha256:report.sha256});
  const raw = Buffer.from(JSON.stringify(manifest));
  if (raw.length > MAX_METADATA) throw Error('Roster metadata is too large.');
  const header = Buffer.alloc(HEADER); MAGIC.copy(header); header.writeUInt32LE(raw.length,8); sha(raw).copy(header,12);
  return Buffer.concat([header,raw,bundle]);
}
function inspect(bytes, expectedGameBuildSha256) {
  if (!Buffer.isBuffer(bytes) || bytes.length < HEADER || bytes.length > MAX_BYTES || !bytes.subarray(0,8).equals(MAGIC))
    throw Error('Not a native roster file. Mapped-fields exports and encrypted backups cannot be imported here.');
  const length = bytes.readUInt32LE(8);
  if (!length || length > MAX_METADATA || HEADER + length > bytes.length) throw Error('Invalid roster metadata length.');
  const raw = bytes.subarray(HEADER,HEADER+length);
  if (!sha(raw).equals(bytes.subarray(12,44))) throw Error('Roster metadata checksum failed.');
  let value; try {value = JSON.parse(raw.toString('utf8'));} catch {throw Error('Invalid roster metadata JSON.');}
  metadata(value);
  if (expectedGameBuildSha256 !== undefined && value.gameBuildSha256 !== expectedGameBuildSha256)
    throw Error('Roster was exported for a different game build.');
  const bundle = bytes.subarray(HEADER+length);
  if (bundle.length !== value.bundleBytes || sha(bundle).toString('hex') !== value.bundleSha256)
    throw Error('Roster bundle checksum or length failed.');
  const report = bundleCodec.inspect(bundle);
  return {metadata:{...value},bundle:Buffer.from(bundle),blockCount:report.blockCount,
    importEnabled:false,crossAccountVerified:false,createdPlayerDependenciesVerified:false};
}
module.exports = {create,inspect,MAX_BYTES};
