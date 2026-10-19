// Build-time only: authoritative WPF definitions and request normalization.
import fs from 'node:fs';import path from 'node:path';import crypto from 'node:crypto';import {createRequire}from'node:module';import{fileURLToPath}from'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),require=createRequire(import.meta.url);
const source=path.resolve(root,'../nba2k27-mod-launcher/src/core/player-research.js');
const {validateTeamUniformEditInput,TEAM_UNIFORM_COLOR_FIELDS:colors,TEAM_UNIFORM_PACKED_FIELDS:packed}=require(source);
const read=JSON.parse(fs.readFileSync(path.join(root,'assets/roster-layout.json'),'utf8'));
const fields=[];
for(const [request,argument,maximum,readKey]of [...colors.map(([key,arg])=>[key,arg,0xFFFFFFFF,key]),...packed.map(([key,arg,min,max])=>{if(min!==0)throw Error('Unexpected uniform minimum');return[key,arg,max,key];}),['home','home',7,'headbandColorHome'],['away','away',7,'headbandColorAway'],['logo','logo',3,'headbandLogoTypeId']]){
 const f=read.uniforms.find(f=>f.key===readKey);if(!f||f.bytes!==4&&!['typeId'].includes(request))throw Error('Missing uniform layout '+readKey);
 fields.push({request,readKey,argument,maximum,offset:f.offset,shift:f.shift??0,mask:f.kind==='signed'?255:f.kind==='uint'?0xFFFFFFFF:f.mask,color:colors.some(([key])=>key===request)});
}
if(fields.length!==52||new Set(fields.map(f=>f.request)).size!==52)throw Error('Uniform field coverage changed');
const layout={schema:'nba2k27-uniform-write-layout-v1',sourceSha256:crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex'),readerSha256:read.sourceSha256,fields};
fs.writeFileSync(path.join(root,'assets/uniform-write-layout.json'),JSON.stringify(layout));
const base={recordIndex:0,teamId:42,expectedTeamName:'QA Team',slot:0,uniformId:77,expectedBaseFilename:'uniform_qa',expectedHome:0,expectedAway:0,expectedLogo:0,home:0,away:0,logo:0};
for(const f of fields){base[f.request]=0;if(f.color)base['expected'+f.request[0].toUpperCase()+f.request.slice(1)]=0;}
for(const s of['160','164','168','16C'])base['expectedFlags'+s]=0;
const maxima={recordIndex:1023,teamId:65535,slot:31,uniformId:65535,expectedHome:7,expectedAway:7,expectedLogo:3,...Object.fromEntries(fields.map(f=>[f.request,f.maximum]))};
for(const f of fields.filter(f=>f.color))maxima['expected'+f.request[0].toUpperCase()+f.request.slice(1)]=0xFFFFFFFF;
for(const s of['160','164','168','16C'])maxima['expectedFlags'+s]=0xFFFFFFFF;
const cases=[];const add=v=>{const input=JSON.parse(JSON.stringify(v));try{cases.push({input,ok:true,expected:validateTeamUniformEditInput(input)});}catch{cases.push({input,ok:false});}};
add(base);for(const[key,max]of Object.entries(maxima))for(const value of[undefined,null,true,{},'',-1,1.5,max,max+1,String(max)])add({...base,[key]:value});
for(const key of['recordIndex','uniformPrimaryColor','typeId','expectedFlags16C'])for(const value of[false,[],[1],[null],[[1]],'0x1','0o1','0b1','1e0','🏀'])add({...base,[key]:value});
for(const[key,max]of[['expectedTeamName',23],['expectedBaseFilename',63]])for(const value of[undefined,null,0,true,{},[],['A','B'],'',' \uFEFF QA \uFEFF ','A'.repeat(max),'A'.repeat(max+1),'QA\0bad',1e21,1e-7,{toString:null}])add({...base,[key]:value});
fs.writeFileSync(path.join(root,'assets/uniform-validation-regressions.json'),JSON.stringify(cases));
console.log(JSON.stringify({uniformControls:fields.length,uniformValidationCases:cases.length,gameAccess:false}));
