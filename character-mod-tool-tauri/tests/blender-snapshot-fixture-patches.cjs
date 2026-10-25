// Propose fixture-only protocol updates without editing tests owned by the parent.
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const replacements = {
  'bridge-lifecycle.cjs': [
    ["      if (command === 'runtime-info') return { smoke: false };",
     "      if (command === 'runtime-info') return { smoke: false };\n" +
     "      if (command === 'blender-snapshot-prepare') return { snapshotId: payload.handoffId };\n" +
     "      if (command === 'blender-snapshot-release') return true;"],
  ],
  'player-preview-freshness.cjs': [
    ["      if (command === 'runtime-info') return {};",
     "      if (command === 'runtime-info') return {};\n" +
     "      if (command === 'blender-snapshot-prepare') return { snapshotId: payload.handoffId };\n" +
     "      if (command === 'blender-snapshot-release') return true;"],
  ],
  'preview-resources.cjs': [
    ["      if (command === 'runtime-info') return {};",
     "      if (command === 'runtime-info') return {};\n" +
     "      if (command === 'blender-snapshot-prepare') { const value = resources(true); return { ...value, snapshotId: value.resourceLease }; }\n" +
     "      if (command === 'blender-snapshot-fallback') return { ...resources(false), snapshotId: payload.snapshotId };\n" +
     "      if (command === 'blender-snapshot-release') { if (!live.delete(payload.snapshotId)) duplicate.push(payload.snapshotId); return true; }"],
  ],
  'preview-scopes.cjs': [
    ["      if (command === 'cancel-preview') return true;",
     "      if (command === 'cancel-preview' || command === 'blender-snapshot-release') return true;"],
    ["independent.calls[0].resolve({ native: false, filePath: 'C:/test/player.iff', options: {} });",
     "independent.calls[0].resolve({ native: false, snapshotId: 'scope-lease', filePath: 'C:/test/player.iff', options: {} });"],
  ],
  'preview-barriers.cjs': [
    ["      if (command === 'reset') return true;",
     "      if (command === 'reset' || command === 'blender-snapshot-release') return true;\n" +
     "      if (command === 'blender-snapshot-prepare') return { ...resources(Boolean(options.native)), snapshotId: 'barrier-lease' };"],
    [": call.command === 'native-preview-resources' && call.payload._cmtPreviewKind === kind));",
     ": call.command === (kind === 'independent' ? 'blender-snapshot-prepare' : 'native-preview-resources') && call.payload._cmtPreviewKind === kind));"],
  ],
};
if (require.main === module) {
  const name = process.argv[2];
  if (!replacements[name]) throw Error('Pass one of: '+Object.keys(replacements).join(', '));
  const file = path.join(__dirname,name);
  let source = fs.readFileSync(file,'utf8');
  for (const [oldText,newText] of replacements[name]) {
    if (source.includes(newText)) continue;
    if (source.split(oldText).length!==2) throw Error('Fixture context changed: '+name);
    source=source.replace(oldText,newText);
  }
  const test = new Module(file,module);
  test.filename=file;
  test.paths=Module._nodeModulePaths(__dirname);
  test._compile(source,file);
}
module.exports = replacements;
