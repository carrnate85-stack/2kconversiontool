'use strict';
const {readZipArchive,buildZipArchive}=require('./jukebox-manager');
const {sha}=require('./scoreboard-layout-patch');
const plan=require('../data/highschool-digit-probe.json');
// Static asset-only probe. Never reads scores, touches a process, or changes
// any other material. Deliberately not presented as a live scoreboard fix.
function buildHighschoolDigitProbe(source){
 const entries=readZipArchive(source),raw=entries.get(plan.member);
 if(!raw||sha(raw)!==plan.sourceSha256)throw Error('High school scene changed; digit probe stopped.');
 const scene=JSON.parse('{'+raw.toString('utf8')+'}'),root=scene.scorebug_highschool;
 const material=root?.Material?.[plan.material],effect=root?.Effect?.[material?.Effect];
 if(material?.Effect!=='unlit.fx#d8218efd5c3ec061'||!effect?.Parameter?.[plan.parameter]||Object.hasOwn(material.Parameter,plan.parameter))throw Error('High school digit material differs.');
 const changed=structuredClone(scene);changed.scorebug_highschool.Material[plan.material].Parameter[plan.parameter]=plan.value;
 const encoded=JSON.stringify(changed,null,2).slice(1,-1),roundtrip=JSON.parse('{'+encoded+'}');
 delete roundtrip.scorebug_highschool.Material[plan.material].Parameter[plan.parameter];
 if(JSON.stringify(roundtrip)!==JSON.stringify(scene))throw Error('Unrelated High school scene data changed.');
 const replacements=new Map(entries);replacements.set(plan.member,Buffer.from(encoded));
 const output=buildZipArchive([...replacements].map(([name,data])=>({name,data}))),verified=readZipArchive(output);
 if(verified.size!==entries.size)throw Error('High school archive membership changed.');
 for(const [name,bytes]of replacements)if(!verified.get(name)?.equals(bytes))throw Error('High school probe archive verification failed.');
 return output;
}
function createHighschoolDigitProbe(options){
 const {ScoreboardLayoutSession}=require('./scoreboard-layout-session');
 return new ScoreboardLayoutSession({...options,layouts:[{...plan,id:'highschool-static-seven-v1'}],targets:[],patch:buildHighschoolDigitProbe});
}
module.exports={buildHighschoolDigitProbe,createHighschoolDigitProbe};
