const fs = require('fs');
const path = require('path');
const esbuild = require('./build-tools/node_modules/esbuild');
const { execFileSync } = require('child_process');
const { parse, parseFragment, serialize } = require('./build-tools/node_modules/parse5');
const root = __dirname;
execFileSync(process.execPath, [path.join(root, 'typecheck.cjs')], { stdio: 'inherit' });
const production = path.join(root, '../character-mod-tool-webview2/Reference/src');
const output = path.join(root, 'frontend');
fs.cpSync(path.join(production, 'renderer'), output, { recursive: true });
fs.copyFileSync(path.join(root, 'web/renderer/model-preview.js'), path.join(output, 'model-preview.js'));
fs.copyFileSync(path.join(root, 'web/renderer/body-preview-editor.mjs'), path.join(output, 'body-preview-editor.mjs'));
fs.copyFileSync(path.join(root, 'web/renderer/explorer-model-preview.js'), path.join(output, 'explorer/model-preview.js'));
fs.copyFileSync(path.join(root, 'web/renderer/strand-preview-fixes.mjs'), path.join(output, 'strand-preview-fixes.mjs'));
fs.copyFileSync(path.join(root, 'web/renderer/skin-response.mjs'), path.join(output, 'skin-response.mjs'));
fs.copyFileSync(path.join(root, 'web/renderer/portrait-skin.mjs'), path.join(output, 'portrait-skin.mjs'));
fs.copyFileSync(path.join(root, 'web/renderer/portrait-capture.mjs'), path.join(output, 'portrait-capture.mjs'));
fs.copyFileSync(path.join(root, 'web/renderer/studio-shading.mjs'), path.join(output, 'studio-shading.mjs'));
let html = fs.readFileSync(path.join(output, 'index.html'), 'utf8');
html = html.replace(/<meta[^>]*http-equiv="Content-Security-Policy"[^>]*>/i, '');
html = html.replace('<script src="lucide.min.js"></script>', '<script src="tauri-bridge.js"></script>\n<script src="lucide.min.js"></script>');
fs.copyFileSync(path.join(root, 'web/ui/preview-layout.css'), path.join(output, 'preview-layout.css'));
const document = parse(html);
const head = document.childNodes.find(node => node.tagName === 'html').childNodes.find(node => node.tagName === 'head');
const link = parseFragment('<link rel="stylesheet" href="preview-layout.css">').childNodes[0];
link.parentNode = head;
head.childNodes.push(link);
fs.writeFileSync(path.join(output, 'index.html'), serialize(document));
for (const name of ['app', 'configs-ui', 'sidebar-ui', 'virtual-table', 'viewer-tools', 'theme']) {
  const source = fs.readFileSync(path.join(root, `web/ui/${name}.ts`), 'utf8');
  const compiled = esbuild.transformSync(source, { loader: 'ts', target: 'es2022', sourcefile: `web/ui/${name}.ts` });
  fs.writeFileSync(path.join(output, `${name}.js`), compiled.code);
}
esbuild.buildSync({ entryPoints: [path.join(root, 'web/tauri-bridge.ts')], outfile: path.join(output, 'tauri-bridge.js'), bundle: true, platform: 'browser', format: 'iife', target: 'es2022' });
esbuild.build({
  entryPoints: [path.join(root, 'worker/index.js')],
  outfile: path.join(output, 'preview-worker.bundle.js'),
  bundle: true, platform: 'browser', format: 'iife', target: 'es2022',
  inject: [path.join(root, 'worker/globals.js')],
  plugins: [{ name: 'native-iff', setup(build) {
    build.onResolve({ filter: /^\.\/iff$/ }, () => ({ path: path.join(root, 'worker/iff.js') }));
    build.onLoad({ filter: /[\\/]core[\\/]preview\.js$/ }, args => {
      let source = fs.readFileSync(args.path, 'utf8');
      const mesh = 'result.mesh.sourceKind = model.sourceKind;';
      const payload = 'file: filePath,\n    scenes,';
      if (!source.includes(mesh) || !source.includes(payload)) throw new Error('Pinned preview geometry hooks changed.');
      source = source.replace(mesh, mesh + '\n        result.mesh.geometryKey = model.iff.geometryKey ? model.iff.geometryKey + ":" + model.path : null;');
      source = source.replace('const skipped = [];', 'const skipped = textureSources.flatMap(s => s.iff.warnings || []);');
      const projectionStart = source.indexOf('function projectTangentSpaceMeshes(preview, basePreview) {');
      const projectionEnd = source.indexOf('function projectTangentSpaceMeshesRigidReference(', projectionStart);
      if (projectionStart < 0 || projectionEnd < 0) throw new Error('Pinned tangent projection changed.');
      source = source.slice(0, projectionStart) + "const { projectTangentSpaceMeshes } = require('cmt-tangent-frame');\n\n" + source.slice(projectionEnd);
      const meshStart = 'const meshes = [];\n  const skipped';
      if (!source.includes(meshStart)) throw new Error('Pinned native mesh override hook changed.');
      source = source.replace(meshStart, 'const meshes = options.nativeGeometry?.meshes || [];\n  const skipped');
      source = source.replace('let triangles = 0;\n  const arenaLike', 'let triangles = options.nativeGeometry?.stats.triangles || 0;\n  meshes.forEach(mesh => (mesh.groups || []).forEach(group => materialUsage.set(group.material, (materialUsage.get(group.material) || 0) + 1)));\n  const arenaLike');
      source = source.replace('for (const model of uniqueModels.slice(0, maxModels))', 'for (const model of (options.nativeGeometry ? [] : uniqueModels.slice(0, maxModels)))');
      source = source.replace(payload, payload + '\n    geometryKey: textureSources.every(s => s.iff.geometryKey) ? textureSources.filter(s => ["main", "geometry", "preview-jersey"].includes(s.kind)).map(s => s.kind + ":" + s.iff.geometryKey).join("|") : null,\n    textureKey: textureSources.every(s => s.iff.contentKey) ? textureSources.map(s => s.iff.contentKey).join("|") + JSON.stringify(options.externalTextures || []) : null,');
      return { contents: source, loader: 'js' };
    });
  }}],
  alias: {
    'cmt-original-iff': path.join(production, 'core/iff.js'),
    'node:fs/promises': path.join(root, 'worker/fs.js'),
    'node:path': path.join(root, 'worker/path.js'),
    'node:zlib': path.join(root, 'worker/zlib.js'),
    'cmt-preview': path.join(production, 'core/preview.js'),
    'cmt-tangent-frame': path.join(root, 'worker/tangent-frame.js')
  }
}).then(() => {
  const bundle = path.join(output, 'preview-worker.bundle.js');
  fs.writeFileSync(bundle, require('./worker/bundle-patches.cjs').patchWorkerBundle(fs.readFileSync(bundle, 'utf8')));
  console.log('TypeScript application UI and native preview worker prepared for Tauri.');
})
  .catch(error => { console.error(error); process.exitCode = 1; });
