const fs = require('fs');
const path = require('path');
const ts = require('./build-tools/node_modules/typescript');
const { parse } = require('./build-tools/node_modules/parse5');
const root = __dirname;
const html = fs.readFileSync(path.join(root, '../character-mod-tool-webview2/Reference/src/renderer/index.html'), 'utf8');
const types = { input: 'HTMLInputElement', select: 'HTMLSelectElement', button: 'HTMLButtonElement', dialog: 'HTMLDialogElement', canvas: 'HTMLCanvasElement', form: 'HTMLFormElement', table: 'HTMLTableElement', tbody: 'HTMLTableSectionElement', textarea: 'HTMLTextAreaElement' };
const lines = ['// Generated from the production HTML. Do not edit.', 'interface UIElements {'];
function visit(node) {
  const id = node.attrs?.find(attribute => attribute.name === 'id')?.value;
  if (id) lines.push(`  ${JSON.stringify(id)}: ${types[node.tagName] || 'HTMLElement'};`);
  for (const child of node.childNodes || []) visit(child);
}
visit(parse(html));
lines.push('}', '');
fs.writeFileSync(path.join(root, 'web/ui/dom.generated.d.ts'), lines.join('\n'));
let errors = 0;
for (const configName of ['tsconfig.json', 'tsconfig.ui.json']) {
  const file = path.join(root, configName);
  const config = ts.readConfigFile(file, ts.sys.readFile);
  const parsed = ts.parseJsonConfigFileContent(config.config, ts.sys, root);
  const program = ts.createProgram(parsed.fileNames, parsed.options);
  const diagnostics = [...parsed.errors, ...ts.getPreEmitDiagnostics(program)];
  errors += diagnostics.length;
  if (diagnostics.length) process.stdout.write(ts.formatDiagnostics(diagnostics, { getCurrentDirectory: () => root, getCanonicalFileName: x => x, getNewLine: () => '\n' }));
}
if (errors) process.exitCode = 1;
else console.log('TypeScript IPC and application UI type checks passed.');
