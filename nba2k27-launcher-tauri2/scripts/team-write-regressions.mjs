// Original WPF request normalizer only. No game scan or helper execution.
import fs from 'node:fs';import path from 'node:path';import {createRequire} from 'node:module';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),require=createRequire(import.meta.url);
const {validateTeamEditInput}=require(path.resolve(root,'../nba2k27-mod-launcher/src/core/player-research.js'));
const base={recordIndex:0,teamId:42,expectedName:'QA Team',name:'QA Team',cityName:'Philadelphia',cityShort:'PHI',stateShort:'PA',mainLogo:'logo_qa',rosterTypeId:0,historicYearId:0,arenaId:0,floorFile:'rucker_floor'};
const cases=[];function add(v){const input=JSON.parse(JSON.stringify(v));try{cases.push({input,ok:true,expected:validateTeamEditInput(input)});}catch{cases.push({input,ok:false});}}
add(base);
for(const [key,max] of [['recordIndex',1023],['teamId',65535],['rosterTypeId',31],['historicYearId',127],['arenaId',65535]])for(const value of [undefined,null,true,false,{},[],[1],[null],[[1]],'', ' \uFEFF ', -1,0,1.5,max,max+1,String(max),'0x1','0b10','0o11','1e2','Infinity'])add({...base,[key]:value});
for(const [key,max] of [['expectedName',23],['name',23],['cityName',17],['cityShort',6],['stateShort',6],['mainLogo',35],['floorFile',39]])for(const value of [undefined,null,true,0,12.5,{},[],['A','B'],'', ' \uFEFF QA \uFEFF ','A'.repeat(max),'A'.repeat(max+1),'🏀'.repeat(Math.floor(max/2)),'QA\0Name',1e21,1e-7,1e-6,1e20,1.234e30,1.23e-10])add({...base,[key]:value});
for(const key of ['name','cityName','mainLogo'])for(const value of [{toString:null},{toString:'x'},[{toString:0}],{valueOf:'ignored'}])add({...base,[key]:value});
fs.writeFileSync(path.join(root,'assets/team-write-validation-regressions.json'),JSON.stringify(cases));
console.log(JSON.stringify({originalTeamRequestCases:cases.length,gameAccess:false}));
