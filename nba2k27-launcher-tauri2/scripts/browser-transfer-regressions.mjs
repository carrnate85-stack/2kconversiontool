// Development-only compatibility oracle. Original WPF functions; no game I/O.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url),root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const source=path.resolve(root,'../nba2k27-mod-launcher/src');
const dna=require(path.join(source,'core/player-dna.js'));
const schema=require(path.join(source,'data/player-field-schema.json'));
const {maximum}=require(path.join(source,'core/player-field-edits.js'));
const {Player23Catalog}=require(path.join(source,'core/player23-catalog.js'));
const originalPresets=require(path.join(source,'core/editor-presets.js'));
const {controls}=require(path.join(source,'core/team-staff.js'));
const base={source:'live-roster',recordIndex:7,recordAddress:'0xDEADBEEF',pid:42,teamName:'QA team',
 firstName:'QA',lastName:'Player',cyberfaceId:4117,positionId:0,secondaryPositionId:5,jerseyNumberRaw:100,jerseyNumber:'00',weightLbs:185.125,heightCm:190.5,wingspanCm:205.75,potential:88,
 audio:{fullNameId:-1},collegeName:'Davidson',bioLayoutVerified:true,birthDate:'2000-02-29',birthYear:2000,birthMonth:2,birthDay:29,yearsPro:3,ratingsRaw:schema.map(()=>0)};
const copies=[];
function add(input){let expected=null;try{expected=dna.fromPlayer(input);}catch{}copies.push({input,expected});}
add(base);
for(const [key,values] of Object.entries({source:['layout-preview','cataloged-roster',null],ratingsRaw:[null,[],schema.map(f=>maximum(f)),schema.map(f=>maximum(f)+1),schema.map(()=>0.5)],
firstName:['','🏀 Player','x'.repeat(20),'x\0'],lastName:['','x'.repeat(20)],collegeName:['','\ufeff','\u0085','\ufeffColumbia\ufeff','x'.repeat(48),null],
bioLayoutVerified:[false,null],birthDate:['1900-02-29','2000-03-01',null],birthYear:[2001,null],yearsPro:[0,31,32,-1,0.5,null],
weightLbs:[70,500,69,501,null],heightCm:[120,260,119,261],wingspanCm:[120,320,119,321],potential:[25,110,24,111],positionId:[0,5,6,null],
jerseyNumberRaw:[null,-1,101,0.5],audio:[null,{fullNameId:-2},{fullNameId:32767},{fullNameId:32768}],cyberfaceId:[0,65535,65536,null]}))for(const value of values)add({...base,[key]:value});
for(const jersey of ['00','0','09','99','100','x',100,0.5,null])add({...base,jerseyNumberRaw:null,jerseyNumber:jersey});
const catalog=new Player23Catalog(root);await catalog.load();
const status=catalog.summary(),selections=[];
for(const playerId of [512,0,8191]){
 let player;try{player=await catalog.player({catalogId:status.catalogId,playerId});}catch{continue;}
 for(const sections of [['info'],['attributes','tendencies'],['appearance','personality','hotzones'],['seasons'],dna.groups,[],['invalid'],Array(21).fill('info')]) {
  for(const includeAwards of [true,false]){
   const input={action:'selection',catalogId:status.catalogId,playerId,sections,seasonIds:player.seasons.slice(0,2).map(s=>s.id),includeAwards};
   let expected=null;try{expected=await catalog.selection(input);}catch{}selections.push({input,expected});
  }
 }
 if(player.seasons.length)for(const seasonIds of [[],[player.seasons[0].id,player.seasons[0].id],['bad'],[-1],[65535],null,Array(26).fill(player.seasons[0].id)]){
  const input={action:'selection',catalogId:status.catalogId,playerId,sections:['info','seasons'],seasonIds};
  let expected=null;try{expected=await catalog.selection(input);}catch{}selections.push({input,expected});
 }
}
const presets=[];
function preset(input){let expected=null;try{expected=originalPresets.validate(input);}catch{}presets.push({input,expected});}
for(const kind of ['signature','equipment'])for(const f of schema.filter(f=>f.group===kind))for(const value of [-1,0,maximum(f),maximum(f)+1,0.5,'0'])preset({format:'2k-editor-preset',version:1,kind,name:'  QA preset  ',values:{[f.key]:value}});
for(const f of controls)for(const value of [-1,0,f.max??2**f.bits-1,(f.max??2**f.bits-1)+1])preset({format:'2k-editor-preset',version:1,kind:'coaching',name:'QA coaching',values:{[f.key]:value}});
const signatureKey=schema.find(f=>f.group==='signature'&&maximum(f)>=0).key;
for(const name of ['', ' ', '\ufeff', '\u0085','\ufeffQA\ufeff','x'.repeat(60),'x'.repeat(61),'🏀'.repeat(31),'x\0','x\x7f'])preset({format:'2k-editor-preset',version:1,kind:'signature',name,values:{[signatureKey]:0}});
fs.writeFileSync(path.join(root,'assets/browser-transfer-regressions.json'),JSON.stringify({copies,selections,presets}));
console.log(JSON.stringify({loadedPlayerCopies:copies.length,catalogSelections:selections.length,presetValidationCases:presets.length,gameAccess:false}));
