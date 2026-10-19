// Preserve the existing pure UI arithmetic. This is not a Node backend.
import fs from 'node:fs';import path from 'node:path';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const original=fs.readFileSync(path.join(root,'../nba2k27-mod-launcher/src/renderer/player-season-math.js'),'utf8');
const lines=original.split(/\r?\n/);const wrapper=lines.shift();
if(!wrapper?.startsWith('(function(root,factory)')||!wrapper.endsWith('()=>{'))throw Error('Season arithmetic wrapper changed; review before porting.');
while(lines.at(-1)==='')lines.pop();if(lines.pop()!=='});')throw Error('Season arithmetic footer changed.');
const body=lines.join('\n');if(/\b(?:require|window|module|document)\b/.test(body))throw Error('Unexpected runtime dependency in UI arithmetic.');
const data='// Generated from the WPF pure UI arithmetic; no Node runtime.\nexport const seasonMath = (() => {\n'+body+'\n})();\n';
const file=path.join(root,'frontend/season-ui-math.js');if(!fs.existsSync(file)||fs.readFileSync(file,'utf8')!==data)fs.writeFileSync(file,data);
console.log(JSON.stringify({seasonUiArithmetic:'WPF exact body',backend:'Rust',gameWrites:false}));
