const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const { Rpc, root } = require('./rpc-client.cjs');
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
(async () => {
  const rpc = new Rpc({ env: { CMT_TEST_DATA: path.join(root, 'artifacts/native-dialog-data') } });
  const report = { version: JSON.parse(fs.readFileSync(path.join(root, 'dist/BUILD.json'), 'utf8').replace(/^\uFEFF/, '')).version, completed: false, cases: [] };
  try {
    const target = path.resolve(root, '../character-mod-tool-rust/artifacts/conversion-output/png06827.iff');
    const native = await rpc.call('native-preview-resources', { target, includeJersey: true });
    const fallback = await rpc.call('preview-resources', { target, includeJersey: true });
    const python = path.resolve(root, '../character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Backend/python_runtime/python.exe');
    for (const garment of native.options.companionPaths.filter(item => item.kind === 'preview-jersey')) {
      const original = fallback.options.companionPaths.find(item => item.name === garment.name);
      if (!original) throw new Error('Original reader omitted garment ' + garment.name);
      const archive = await rpc.call('read-archive', { path: garment.path });
      const textures = new Map();
      const visit = value => {
        if (!value || typeof value !== 'object') return;
        if (typeof value.Binary === 'string' && /\.(tld|dds)$/i.test(value.Binary)) {
          textures.set(value.Binary, { name: value.Binary, segments: value.Segments?.length || 0, format: value.Format, width: value.Width, height: value.Height });
        }
        for (const child of Object.values(value)) visit(child);
      };
      for (const member of archive.members.filter(item => /\.(scne|txtr)$/i.test(item.name))) visit(JSON.parse(Buffer.from(member.data, 'base64').toString('utf8')));
      const originalDds = JSON.parse(execFileSync(python, ['-B', '-c',
        "import sys,zipfile,json,hashlib; z=zipfile.ZipFile(sys.argv[1]); rows=[]\nfor i in z.infolist():\n if i.filename.lower().endswith(('.dds','.tld')):\n  b=z.read(i); rows.append({'name':i.filename,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest(),'width':int.from_bytes(b[16:20],'little'),'height':int.from_bytes(b[12:16],'little'),'mips':int.from_bytes(b[28:32],'little'),'dxgi':int.from_bytes(b[128:132],'little') if b[84:88]==b'DX10' else None,'arraySize':int.from_bytes(b[140:144],'little') if b[84:88]==b'DX10' else 1})\nprint(json.dumps(rows))",
        original.path], { windowsHide: true, encoding: 'utf8' }));
      const nativeDds = archive.members.filter(item => /\.dds$/i.test(item.name)).map(item => ({ name: item.name, bytes: item.size, sha256: hash(Buffer.from(item.data, 'base64')) }));
      report.cases.push({ name: garment.name, metadata: [...textures.values()], warnings: archive.warnings, originalDds, nativeDds,
        missingNative: originalDds.filter(item => /\.dds$/i.test(item.name) && !nativeDds.some(native => native.name === item.name)),
        changed: originalDds.filter(item => nativeDds.some(native => native.name === item.name && native.sha256 !== item.sha256)) });
    }
    assert.equal(report.cases.length, 2);
    for (const garment of report.cases) {
      assert.equal(garment.originalDds.length, 4);
      assert.deepEqual(garment.missingNative, [], 'Native decoder omitted original garment maps');
      assert.deepEqual(garment.changed, [], 'Native garment DDS differs from original reader');
      assert.deepEqual(garment.warnings, [], 'Native garment decoding still warns');
      assert(garment.originalDds.every(texture => texture.arraySize === 8));
    }
    report.completed = true;
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await rpc.close();
    fs.writeFileSync(path.join(root, 'artifacts/garment-parity-report.json'), JSON.stringify(report, null, 2));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
