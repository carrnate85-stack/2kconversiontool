// Display metadata from the retained, hash-pinned WPF shoe-style identities.
// Does not infer models from numeric ranges or modify wardrobe asset IDs.
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const reference = path.resolve(root, '../nba2k27-launcher-wpf/release/quality-pass-qa-20261010/App/Reference/src');
const require = createRequire(import.meta.url);
const { humanizeModel } = require(path.join(reference, 'core/shoe-style-names.js'));
const source = fs.readFileSync(path.join(reference, 'data/shoe-model-names.json'));
const pins = JSON.parse(fs.readFileSync(path.join(root, 'tools/fit-preview/pins.json')));
if (createHash('sha256').update(source).digest('hex') !== pins['data/shoe-model-names.json']) throw Error('WPF shoe model source changed');
const brands = { nike:'Nike', adidas:'Adidas', jordan:'Jordan', reebok:'Reebok', converse:'Converse', underarmour:'Under Armour', lining:'Li-Ning', newbalance:'New Balance', puma:'Puma', anta:'Anta', threesixone:'361°', and1:'AND1', peak:'Peak', qiaodan:'Qiaodan', rigorer:'Rigorer', sevenfourone:'Seven Four One', fila:'Fila', skechers:'Skechers', jcole:'J. Cole', special:'Special' };
const display = { '0000': { name:'Default shoes', brand:'2K' } };
for (const [id, model] of Object.entries(JSON.parse(source).models)) {
  const prefix = model.slug.split('_')[0];
  const brand = prefix === 'coach' ? 'Coach' : brands[prefix];
  if (!brand) throw Error(`Unknown brand prefix: ${model.slug}`);
  display[id] = { name:humanizeModel(model.slug), brand };
}
fs.writeFileSync(path.join(root, 'assets/shoe-model-display.json'), JSON.stringify(display, null, 2)+'\n');
console.log(`Prepared ${Object.keys(display).length} shoe display identities.`);
