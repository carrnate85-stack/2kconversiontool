// Native numeric edit oracle; local WPF source only, no game reads or writes.
import fs from 'node:fs';import path from 'node:path';import {createRequire} from 'node:module';import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const math=createRequire(import.meta.url)(path.join(root,'../nba2k27-mod-launcher/src/renderer/player-season-math.js'));
const sample=()=>({gp:70,gs:70,points:1980,minutes:2661,fgm:694,fga:1501,threeMade:220,threeAttempted:599,ftm:372,fta:417,oreb:24,dreb:266,assists:461,steals:130,blocks:55,turnovers:171,fouls:151,doubleDoubles:7,tripleDoubles:0,plusMinus:113});
const cases=[];function check(before,key,value){let expected=null;try{expected=math.change(before,key,value);}catch{}cases.push({before,key,value,expected});}
const keys=[...Object.keys(math.limits),...Object.keys(math.averages),...Object.keys(math.percentages),'rpg','rebounds','unknown'];
for(const key of keys)for(const value of [-16385,-2.5,-.5,0,.5,1,2,30,50,99,101,127,100000])check(sample(),key,value);
for(let two=0;two<=5;two++)for(let three=0;three<=4;three++)for(let free=0;free<=3;free++)for(let target=0;target<=30;target++){
  const s={...sample(),gp:1,gs:1,fga:two+three,threeAttempted:three,fta:free,fgm:0,threeMade:0,ftm:0,points:0};check(s,'ppg',target);
}
for(const key of Object.keys(math.averages))check({...sample(),gp:0},key,30);
for(const [key,[made,attempt]]of Object.entries(math.percentages))check({...sample(),[attempt]:0,[made]:0},key,50);
check(sample(),'plusMinusAverage',-.15);check(sample(),'plusMinusAverage',-.05);
const output=JSON.stringify(cases),file=path.join(root,'assets/season-math-regressions.json');if(!fs.existsSync(file)||fs.readFileSync(file,'utf8')!==output)fs.writeFileSync(file,output);
console.log(JSON.stringify({seasonMathOracleCases:cases.length,source:'WPF',gameWrites:false}));
