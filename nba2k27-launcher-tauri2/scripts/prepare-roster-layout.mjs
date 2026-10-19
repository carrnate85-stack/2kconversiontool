// Build-time extraction from the original verified WPF reader. Nothing from
// C#/.NET is shipped; the runtime consumes only the bounded layout document.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const research=path.resolve(root,'../nba2k27-roster-research/src');
const program=fs.readFileSync(path.join(research,'Program.cs'),'utf8');
const ratings=fs.readFileSync(path.join(research,'RatingSchema.cs'),'utf8');
const require=createRequire(import.meta.url);
const {maximum}=require(path.resolve(root,'../nba2k27-mod-launcher/src/core/player-field-edits.js'));
const fields=[...ratings.matchAll(/\{(\d+),(\d+),(\d+)\}, \/\/ (\w+)/g)].map(m=>({offset:+m[1],shift:+m[2],bits:+m[3],key:m[4]}));
const original=JSON.parse(fs.readFileSync(path.resolve(root,'../nba2k27-mod-launcher/src/data/player-field-schema.json')));
if(fields.length!==original.length||!fields.every((f,i)=>Object.keys(f).every(k=>f[k]===original[i][k])))throw Error('C# and original browser rating layouts differ');
const editMax=ratings.match(/EditMax = new int\[\] \{([^}]+)\}/)?.[1].split(',').map(Number);
if(!editMax||editMax.length!==fields.length||!editMax.every((max,i)=>max===maximum(original[i])))throw Error('C# and original browser editable-field limits differ');
fields.forEach((f,i)=>f.maximum=editMax[i]);
const propertyKeys=new Map();
for(const line of program.split('\n')){
 const property=line.match(/\.Append\((?:Json\()?uniform\.(\w+)/)?.[1];
 const key=line.match(/\\"([A-Za-z0-9]+)\\"/)?.[1];
 if(property&&key)propertyKeys.set(property,key);
}
const parse=(property,expression)=>{
 let m;
 if(m=expression.match(/^BitConverter\.ToUInt(16|32|64)\(record, (0x[0-9A-Fa-f]+)\)$/))return {kind:'uint',offset:+m[2],bytes:+m[1]/8};
 if(m=expression.match(/^DecodeAscii\(record, (0x[0-9A-Fa-f]+), (0x[0-9A-Fa-f]+)\)$/))return {kind:'ascii',offset:+m[1],bytes:+m[2]};
 if(m=expression.match(/^unchecked\(\(sbyte\)record\[(0x[0-9A-Fa-f]+)\]\)$/))return {kind:'signed',offset:+m[1],bytes:1};
 if(m=expression.match(/^flags(160|164|168|16C)$/))return {kind:'uint',offset:parseInt(m[1],16),bytes:4};
 if(m=expression.match(/flags(160|164|168|16C)(?: >> (\d+))?\)? & (0x[0-9A-Fa-f]+|\d+)/))return {kind:expression.endsWith('!= 0')?'bool':'masked',offset:parseInt(m[1],16),bytes:4,shift:+(m[2]||0),mask:+m[3]};
 throw Error(`Unsupported uniform expression ${property}: ${expression}`);
};
const body=program.match(/TeamUniform uniform = new TeamUniform \{([\s\S]*?)\n                \};/)?.[1];
if(!body)throw Error('Uniform reader not found');
const uniforms=[];
for(const line of body.trim().split('\n')){
 const m=line.trim().match(/^(\w+) = (.*),$/);if(!m)throw Error('Unexpected uniform field: '+line);
 const [,property,expression]=m;if(['Index','Address'].includes(property))continue;
 const key=propertyKeys.get(property);
 if(!key&&!/^SockOption[1-5]$/.test(property))throw Error('Missing original JSON key for '+property);
 uniforms.push({property,key:key??null,...parse(property,expression)});
}
if(uniforms.length!==85)throw Error('Uniform field count changed: '+uniforms.length);
const layout={schema:'nba2k27-native-roster-layout-v1',sourceSha256:crypto.createHash('sha256').update(program).digest('hex'),serializerSha256:ratings.match(/Fingerprint = "([a-f0-9]+)"/)[1],serializerRva:+ratings.match(/Start = (\d+)/)[1],serializerBytes:+ratings.match(/Length = (\d+)/)[1],fields,uniforms};
const output=path.join(root,'assets/roster-layout.json');const encoded=JSON.stringify(layout);
if(!fs.existsSync(output)||fs.readFileSync(output,'utf8')!==encoded)fs.writeFileSync(output,encoded);
console.log(JSON.stringify({ratingFields:fields.length,uniformFields:uniforms.length,sourceSha256:layout.sourceSha256}));
