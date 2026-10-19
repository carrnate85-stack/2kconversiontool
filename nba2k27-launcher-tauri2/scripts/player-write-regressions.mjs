// Build-time validation oracle: the unchanged WPF function, no manager scans.
import fs from 'node:fs';import path from 'node:path';import {createRequire} from 'node:module';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),require=createRequire(import.meta.url);
const {validatePlayerEditInput}=require(path.resolve(root,'../nba2k27-mod-launcher/src/core/player-research.js'));
const base={recordIndex:0,expectedFirstName:'QA',expectedLastName:'Source',expectedCyberfaceId:4090,firstName:'QA',lastName:'Source',cyberfaceId:4090,positionId:0,secondaryPositionId:1,jerseyNumberRaw:30,weightLbs:185.12,heightCm:198.75,wingspanCm:207.25,potential:91};
const cases=[];
const add=(value)=>{const input=JSON.parse(JSON.stringify(value));try{cases.push({input,ok:true,expected:validatePlayerEditInput(input)});}catch{cases.push({input,ok:false});}};
add(base);
for(const [key,min,max] of [['recordIndex',0,8191],['expectedCyberfaceId',0,65535],['cyberfaceId',0,65535],['positionId',0,5],['secondaryPositionId',0,5],['jerseyNumberRaw',0,100],['weightLbs',70,500],['heightCm',120,260],['wingspanCm',120,320],['potential',25,110]]){
 for(const value of [undefined,null,false,true,{},[],'',' \t\uFEFF ',min-1,min,min+.25,max,max+1,String(min),`\uFEFF${min}\uFEFF`,String(max),'0x1','0b10','0o11','1e2','NaN','Infinity'])add({...base,[key]:value});
}
for(const key of ['firstName','lastName','expectedFirstName','expectedLastName'])for(const value of [undefined,null,0,{},'',' \uFEFFQA\uFEFF ','A'.repeat(19),'A'.repeat(20),'🏀'.repeat(9),'🏀'.repeat(10),'QA\nName','\u0000','O’Neal','Željko'])add({...base,[key]:value});
for(const value of ['2000-02-29','1900-02-29','4095-12-31','4096-01-01','0000-01-01','1988-3-14',' 1988-03-14 ',null,0,''])add({...base,birthDate:value,expectedBirthYear:1988,expectedBirthMonth:3,expectedBirthDay:14});
for(const key of ['fromIndex','fullNameId','yearsPro'])for(const value of [undefined,null,false,'',-2,-1,0,1,31,32,4095,4096,32767,32768,'1',1.5])add({...base,[key]:value,expectedYearsPro:17});
for(const change of [null,{},'',[],[{index:0,expected:0,value:1}],[{index:0,expected:0,value:64}],[{index:0,expected:'0',value:1}],[{index:0,expected:0,value:'1'}],[{index:455,expected:0,value:1}],[{index:0,expected:0,value:1},{index:0,expected:0,value:2}]])add({...base,ratingEdits:change});
fs.writeFileSync(path.join(root,'assets/player-write-validation-regressions.json'),JSON.stringify(cases));
console.log(JSON.stringify({originalPlayerRequestValidationCases:cases.length,gameAccess:false}));
