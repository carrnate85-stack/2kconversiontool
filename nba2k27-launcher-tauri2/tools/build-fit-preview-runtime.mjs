// Builds frontend/public/fit-preview/ from the frozen WPF Reference:
//  - index.html, fast-viewer.js and three.js/DDSLoader vendor files
//    (renderer/fit-preview), shown in the mannequin dialog's iframe;
//  - fit-runtime.js: a classic Web Worker bundling WPF's
//    core/fast-clothing-preview-worker.js and its helpers verbatim, plus
//    browser shims. Only three hash-pinned worker regions (decoder process,
//    manifest/cache key, disk cache) are swapped for Rust (fit_assets.rs)
//    and IndexedDB equivalents.
// Usage: node tools/build-fit-preview-runtime.mjs
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { dirname, join, posix, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const project = resolve(here, '..');
const reference = resolve(project, '../nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src');
const out = join(project, 'frontend/public/fit-preview');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const provenance = { source: 'nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src', generator: 'tools/build-fit-preview-runtime.mjs', files: {} };

// Viewer: retained source with explicit, guarded presentation-only adaptations.
for (const file of ['index.html', 'fast-viewer.js', 'vendor/three.module.min.js', 'vendor/three.core.min.js', 'vendor/DDSLoader.js']) {
  const source = join(reference, 'renderer/fit-preview', file), target = join(out, file);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(source, target);
  provenance.files[file] = { from: `renderer/fit-preview/${file}`, sha256: sha(readFileSync(target)) };
}
function adaptViewer(file, replacements) {
  const target = join(out, file);
  let source = readFileSync(target, 'utf8');
  for (const [before, after] of replacements) {
    if (source.split(before).length !== 2) throw Error(`Viewer adaptation no longer unique: ${file}: ${before}`);
    source = source.replace(before, after);
  }
  writeFileSync(target, source);
  provenance.files[file].sourceSha256 = provenance.files[file].sha256;
  provenance.files[file].sha256 = sha(readFileSync(target));
  provenance.files[file].adaptations = ['QA113 lightweight studio lighting and static CSS backdrop.', 'QA114 camera/backdrop controls, view preservation and narrow-view refit.', 'QA115 procedural contact shadow: one two-triangle plane; no textures, shadow maps or animation loop.'];
}
adaptViewer('fast-viewer.js', [
  ['if(palette?.diffuse)palette=null;', 'const opaqueColor=palette?.opaqueColor===true;\n if(palette?.diffuse)palette=null;'],
  ['if(!palette&&!range&&!active.length&&!emission)return;', 'if(!palette&&!range&&!active.length&&!emission&&!opaqueColor)return;'],
  ['diffuseColor*=sampledDiffuseColor;', '${opaqueColor?\'sampledDiffuseColor.a=1.0;\':\'\'}diffuseColor*=sampledDiffuseColor;'],
  ['function colorShader(material,palette,range,artworks,emission=null){', readFileSync(join(here, 'fit-preview/artwork-color.js'), 'utf8')+'\nfunction colorShader(material,palette,range,artworks,emission=null){'],
  ['active.forEach(({texture,placement},index)=>{', 'active.forEach(({texture,placement,palette},index)=>{'],
  ['const prefix=`previewArt${index}`,artRange=texture.userData.range;', 'const prefix=`previewArt${index}`,artRange=texture.userData.range;\n   const colorize=configureArtworkPalette(shader,prefix,palette,declarations);'],
  ['artwork=mix(artwork,decodedArtwork,${prefix}Mask);sampledDiffuseColor.rgb=', 'artwork=mix(artwork,decodedArtwork,${prefix}Mask);${colorize}sampledDiffuseColor.rgb='],
  ['const colorize=configureArtworkPalette(shader,prefix,palette,declarations);', 'const colorize=(palette&&!artRange?\'artwork.rgb=previewLinearToSrgb(artwork.rgb);\':\'\')+configureArtworkPalette(shader,prefix,palette,declarations);'],
  ["${palette?'sampledDiffuseColor.rgb=sampledDiffuseColor.r*previewPrimary", "${palette?.fullRangeMask&&!range?'sampledDiffuseColor.rgb=previewLinearToSrgb(sampledDiffuseColor.rgb);':''}\\n${palette?'sampledDiffuseColor.rgb=sampledDiffuseColor.r*previewPrimary"],
  ['vec3 previewSrgbToLinear(vec3 c){', 'vec3 previewLinearToSrgb(vec3 c){return mix(c*12.92,1.055*pow(max(c,vec3(0.0)),vec3(1.0/2.4))-0.055,step(vec3(0.0031308),c));}vec3 previewSrgbToLinear(vec3 c){'],
  ['${active.length}-${Boolean(emission)}`;', '${active.length}-${Boolean(emission)}-${active.map(a=>Boolean(a.palette)).join(\'\')}`;'],
  ["active.map(a=>Boolean(a.palette)).join('')", "active.map(a=>`${Boolean(a.palette)}:${Boolean(a.texture.userData.range)}`).join('|')"],
  ['clothing-preview-${Boolean(palette)}-', 'clothing-preview-${Boolean(palette)}-${Boolean(palette?.fullRangeMask)}-'],
  ['clothing-preview-${Boolean(palette)}-', 'clothing-preview-${Boolean(opaqueColor)}-${Boolean(palette)}-'],
  ['function makeMeshes(key,payload,palettes,texture,neutral=false,overlays=[],placements=[],shoeTextures=null,materialTextures=null,emission=null){', 'function makeMeshes(key,payload,palettes,texture,neutral=false,overlays=[],placements=[],shoeTextures=null,materialTextures=null,emission=null,overlayPalettes=[]){'],
  ['({texture:art,placement:placements[index]?.[i]?.[j]})', '({texture:art,placement:placements[index]?.[i]?.[j],palette:overlayPalettes[index]})'],
  ['${JSON.stringify(part.palettes)}`;', '${JSON.stringify(part.palettes)}:${JSON.stringify(part.overlayPalettes||[])}`;'],
  ['false,overlays,placements,shoeTextures,materialTextures,emission);', 'false,overlays,placements,shoeTextures,materialTextures,emission,part.overlayPalettes||[]);'],
  ['scene.background=new THREE.Color(0x10151b);', 'scene.background=null;\n' + readFileSync(join(here, 'fit-preview/contact-shadow.js'), 'utf8')],
  ["{canvas,antialias:false,powerPreference:'high-performance'}", "{canvas,alpha:true,antialias:false,powerPreference:'high-performance'}"],
  ['new THREE.AmbientLight(0xffffff,0.7)', 'new THREE.HemisphereLight(0xf4f7ff,0x687887,0.95)'],
  ['new THREE.DirectionalLight(0xffffff,1.1)', 'new THREE.DirectionalLight(0xffffff,1.4)'],
  ['new THREE.DirectionalLight(0xd8e8ff,0.35)', 'new THREE.DirectionalLight(0xe1edff,0.65)'],
  ["let bodyKey='',yaw=", "let viewFramed=false;\nlet bodyKey='',yaw="],
  ['   fit();', '   if(!viewFramed){fit();viewFramed=true;}'],
  ['*1.25;\n draw();', '*1.25*Math.max(1,canvas.clientHeight/Math.max(1,canvas.clientWidth));\n positionContactShadow(bounds);\n const depthBounds=bounds.clone().expandByObject(contactShadow);\n clipRadius=depthBounds.getSize(new THREE.Vector3()).length()/2+depthBounds.getCenter(new THREE.Vector3()).distanceTo(target);\n draw();'],
  ['  const clipBounds=new THREE.Box3().setFromObject(model);', '  const clipBounds=new THREE.Box3().setFromObject(model);\n  positionContactShadow(clipBounds);clipBounds.expandByObject(contactShadow);'],
  ['renderer.dispose();', 'contactShadow.geometry.dispose();contactShadow.material.dispose();renderer.dispose();'],
  ["parent.postMessage({type:'fit-preview:ready'},'*');draw();", readFileSync(join(here, 'fit-preview/viewer-controls.js'), 'utf8') + "\nparent.postMessage({type:'fit-preview:ready'},'*');draw();"],
]);
provenance.files['fast-viewer.js'].adaptations.push('QA120 source LogoN RGB-region palettes, cache identity and part reuse updates; no extra rendering pass.');
provenance.files['fast-viewer.js'].adaptations.push('QA121 recover full-range RGB mask weights after sRGB sampling; distinguish ranged and ordinary artwork shader programs.');
provenance.files['fast-viewer.js'].adaptations.push('QA122 exact-source WNBA sleeve palettes follow acc_nologo opaque color output; alpha policy included in shader cache identity.');
provenance.files['fast-viewer.js#artwork-color'] = {from:'tools/fit-preview/artwork-color.js',sha256:sha(readFileSync(join(here,'fit-preview/artwork-color.js')))};
adaptViewer('index.html', [
  ['</head>', '<link rel="stylesheet" href="studio.css"></head>'],
  ['<body>', `<body><nav class="studio-controls" aria-label="Mannequin view">
<div role="group" aria-label="Camera"><button type="button" data-view="front">Front</button><button type="button" data-view="back">Back</button><button type="button" data-view="reset" title="Reset angle, zoom and pan">Reset</button></div>
<div role="group" aria-label="Backdrop"><button type="button" data-backdrop="dark" aria-pressed="true">Dark</button><button type="button" data-backdrop="light" aria-pressed="false">Light</button></div></nav>`],
  ['W / S to pan</small>', 'W / S to pan · Double-click to fit</small>'],
]);
copyFileSync(join(here, 'fit-preview/studio.css'), join(out, 'studio.css'));
provenance.files['studio.css'] = { from: 'tools/fit-preview/studio.css', sha256: sha(readFileSync(join(out, 'studio.css'))) };

// Worker modules (ids are Reference-relative paths).
const modules = [
  'core/fast-clothing-preview-worker.js',
  'vendor/archive-preview/core/iff.js',
  'vendor/archive-preview/core/preview.js',
  'core/clothing-item-table.js',
  'core/clothing-color-bindings.js',
  'shared/accessory-slots.js',
  'shared/accessory-preview-support.js',
  'core/accessory-rig-fit.js',
  'core/shoe-colorway-preview.js',
  'core/clothing-components.js',
  'data/shoe-model-names.json',
];
// WPF worker regions replaced by tools/fit-preview/worker-overrides.js.
const regions = [
  { name: 'decode', start: 'async function decode(args,signal){', end: 'function remember(key,value){', sha256: null },
  { name: 'manifest', start: 'async function manifest(gameRoot){', end: 'async function cached(gameRoot,name,kind,build,signal){', sha256: null },
  { name: 'loadCached', start: 'async function loadCached(gameRoot,source,kind,key,build,signal){', end: 'async function clothingTables(gameRoot,signal){', sha256: null },
];
const pins = JSON.parse(readFileSync(join(here, 'fit-preview/pins.json'), 'utf8'));
const overrides = Object.fromEntries(readFileSync(join(here, 'fit-preview/worker-overrides.js'), 'utf8')
  .split(/^\/\/@@/m).slice(1).map(part => { const newline = part.indexOf('\n'); return [part.slice(0, newline).trim(), part.slice(newline + 1)]; }));

let bundle = `// GENERATED by tools/build-fit-preview-runtime.mjs from the WPF Reference. Do not edit.\n'use strict';\n(() => {\n`;
bundle += readFileSync(join(here, 'fit-preview/shims.js'), 'utf8').replace(/^'use strict';\n/m, '');
bundle += `\nconst factories = Object.create(null), loaded = Object.create(null);\n`;
bundle += `function resolveId(from, spec) {\n  if (spec.startsWith('node:')) return spec;\n  let id = posix.normalize(posix.join(posix.dirname(from), spec));\n  if (!/\\.(js|json)$/.test(id)) id += '.js';\n  return id;\n}\n`;
bundle += `const posix = { normalize(p) { const parts = []; for (const part of p.split('/')) { if (!part || part === '.') continue; if (part === '..') parts.pop(); else parts.push(part); } return parts.join('/'); }, join: (...p) => p.join('/'), dirname: p => p.split('/').slice(0, -1).join('/') };\n`;
bundle += `function load(id) {\n  if (builtins[id]) return builtins[id];\n  if (loaded[id]) return loaded[id].exports;\n  const factory = factories[id];\n  if (!factory) throw Error('Preview runtime module missing: ' + id);\n  const module = { exports: {} };\n  loaded[id] = module;\n  factory(module, module.exports, spec => load(resolveId(id, spec)));\n  return module.exports;\n}\n`;
for (const id of modules) {
  let source = readFileSync(join(reference, id), 'utf8');
  const digest = sha(source);
  if (pins[id] && pins[id] !== digest) throw Error(`WPF source changed since the runtime was pinned: ${id}`);
  pins[id] = digest;
  provenance.files[`fit-runtime.js#${id}`] = { from: id, sha256: digest };
  if (id === 'core/fast-clothing-preview-worker.js') {
    // QA111: an empty visual draft still renders the body. Retain the bounded
    // array/asset checks; this does not change game outfit validation.
    const emptyGuard = 'assets.length<1||assets.length>7';
    if (source.split(emptyGuard).length !== 2) throw Error('WPF preview asset guard changed');
    source = source.replace(emptyGuard, 'assets.length>7');
    provenance.files[`fit-runtime.js#${id}`].adaptations = ['Allow an empty visual draft to display the mannequin; retain max-seven and asset-name validation.'];
    for (const region of regions) {
      const start = source.indexOf(region.start), end = source.indexOf(region.end, start);
      if (start < 0 || end <= start) throw Error(`WPF worker region not found: ${region.name}`);
      const text = source.slice(start, end), key = `region:${region.name}`;
      if (pins[key] && pins[key] !== sha(text)) throw Error(`WPF worker region changed: ${region.name}`);
      pins[key] = sha(text);
      source = source.slice(0, start) + overrides[region.name] + source.slice(end);
    }
    const adaptations = [
      ['||available.find(dep=>!/normal|coneangle|rdmo|region|mask|wrinkle/.test(dep.name));', `||available.find(dep=>!/normal|coneangle|rdmo|region|mask|wrinkle/.test(dep.name))
  // Historic and some current uniforms bundle diffuse, normal and RDMO maps
  // inside a dependency whose container name ends in *_rdmo_tex. The gallery
  // still exposes the embedded *_color DDS, so retain the first stock
  // dependency as a final candidate after every preferred-name check.
  ||available[0];`],
      ["const hatArtwork=slot==='headwear';", "const hatArtwork=slot==='headwear'||Boolean(accessoryMaterialMap.catalogArtwork?.[asset]);"],
      ['artworkDependencies=resources.filter(Boolean);', 'if(textureDependency)hatArtworkResources.set(textureDependency.name,tex);\n   const mappedArtwork=verifiedHatArtwork(asset,itemBindings,item.dependencies,hatArtworkResources);\n   if((accessoryMaterialMap.boundHats?.[asset]||accessoryMaterialMap.catalogArtwork?.[asset])&&!mappedArtwork)return unavailable("The accessory artwork changed and needs to be verified again.");\n   artworkDependencies=mappedArtwork||resources.filter(Boolean).flatMap(d=>expandVerifiedHatArtwork(d,hatArtworkResources.get(d.name).value.textures||[]));'],
      ['const artStates=artworkDependencies.map((dependency,slot)=>{', 'const artStates=artworkDependencies.map((dependency,slot)=>{\n   if(!dependency)return null;'],
      ['const state={slot,settled:false,promise:null};', 'const state={slot,textureName:dependency.textureName,palette:dependency.palette||null,textureSpec:dependency.textureSpec||null,settled:false,promise:null};'],
      ['const selectedArtwork=colorArtwork.length===1?colorArtwork[0]:null;', 'const namedArtwork=job.state.textureName?artworkTextures.filter(t=>t.name===job.state.textureName):colorArtwork;\n  const selectedArtwork=namedArtwork.length===1?namedArtwork[0]:null;'],
      ['job.part[`overlayKey${suffix}`]=art.value.key;\n  job.part[`overlay${suffix}`]=knownTextures.has(art.value.key)?null:selectedArtwork;', '(job.part.overlayPalettes??=Array(6).fill(null))[job.state.slot]=job.state.palette;\n  const artworkKey=job.state.textureName?crypto.createHash(\'sha256\').update(`${art.value.key}:${job.state.textureName}`).digest(\'hex\'):art.value.key;\n  job.part[`overlayKey${suffix}`]=artworkKey;\n  job.part[`overlay${suffix}`]=knownTextures.has(artworkKey)?null:selectedArtwork;'],
      ['slot?accessorySupport.palette(itemBindings,crc32(Buffer.from(g.material)),materialTextures?.[i]?.[j]?.texture.range||selectedTexture?.range)', 'slot?(accessorySupport.palette(itemBindings,crc32(Buffer.from(g.material)),materialTextures?.[i]?.[j]?.texture.range||selectedTexture?.range)||verifiedAccessoryPalette(asset,itemBindings,crc32(Buffer.from(g.material)),selectedTexture))'],
    ];
    for (const [before, after] of adaptations) {
      if (source.split(before).length !== 2) throw Error('Accessory worker adaptation changed: ' + before);
      source = source.replace(before, after);
    }
    // Mapped arm/headband artwork is as required as hat artwork. Never retain
    // a partly rendered mapped accessory if its exact resource check fails.
    const artworkFailureGuard="job.part.accessorySlot==='headwear'";
    if(source.split(artworkFailureGuard).length!==3)throw Error('Mapped artwork failure guards changed');
    source=source.replaceAll(artworkFailureGuard,"(job.part.accessorySlot==='headwear'||Boolean(accessoryMaterialMap.catalogArtwork?.[job.asset]))");
    const selectedArtworkGuard='if(!selectedArtwork){';
    if(source.split(selectedArtworkGuard).length!==2)throw Error('Artwork texture validation changed');
    source=source.replace(selectedArtworkGuard,'if(!selectedArtwork||(job.state.textureSpec&&!verifiedTextureSpec(selectedArtwork,job.state.textureSpec))){');
    const materialMap = readFileSync(join(here, 'fit-preview/accessory-material-map.json'), 'utf8');
    source += '\nconst accessoryMaterialMap = ' + JSON.stringify(JSON.parse(materialMap)) + ';\n' + readFileSync(join(here, 'fit-preview/accessory-materials.js'), 'utf8');
    provenance.files['fit-runtime.js#accessory-material-map'] = { from: 'tools/fit-preview/accessory-material-map.json', sha256: sha(materialMap) };
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA118 audited source-diffuse sleeve/headband variants and numbered hat artwork bundles; exact texture/material allowlist.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA119 exact CRC32 of source TXTR paths maps hat slots, repeated decals and base-bundle artwork; guarded baked-color hats and static clover sleeves.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA120 source clothing_default LogoNColorizeR/G/B and current identity defaults; exact per-asset guarded palettes.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA121 verified full-range and opaque clothing_default hats with exact format, dimensions and range guards; sleeves remain guarded.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA122 twelve source-bound WNBA sleeves: acc_nologo RGB-only texture sampling and opaque output, verified retained DXIL and current identity defaults.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA123 exact-source catalog RGB palettes and ordered accessory logo bindings; required mapped artwork fails closed on resource changes.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA124 combined uniform texture containers remain eligible after preferred dependency checks; embedded color DDS selection is unchanged.');
    provenance.files[`fit-runtime.js#${id}`].adaptations.push('QA117 verified Vandy numbered artwork slots with distinct cache identities and Pro-Am S1 source-color sleeve.');
    provenance.files['fit-runtime.js#accessory-materials'] = { from: 'tools/fit-preview/accessory-materials.js', sha256: sha(readFileSync(join(here, 'fit-preview/accessory-materials.js'))) };
  }
  if (id.endsWith('.json')) bundle += `factories[${JSON.stringify(id)}] = module => { module.exports = ${source.trim()}; };\n`;
  else bundle += `factories[${JSON.stringify(id)}] = function (module, exports, require) {\n${source}\n};\n`;
}
bundle += readFileSync(join(here, 'fit-preview/entry.js'), 'utf8');
bundle += `\n})();\n`;
writeFileSync(join(out, 'fit-runtime.js'), bundle);
provenance.files['fit-runtime.js'] = { sha256: sha(readFileSync(join(out, 'fit-runtime.js'))) };
writeFileSync(join(here, 'fit-preview/pins.json'), JSON.stringify(pins, null, 2) + '\n');
writeFileSync(join(out, 'PROVENANCE.json'), JSON.stringify(provenance, null, 2) + '\n');
console.log(`fit-preview runtime: ${modules.length} WPF modules, ${(bundle.length / 1024).toFixed(0)} KiB`);
