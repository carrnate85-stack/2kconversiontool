'use strict';
const {readZipArchive,buildZipArchive}=require('./jukebox-manager');
const {patchScoreboardMember}=require('./scoreboard-layout-patch');
const {resolve}=require('./camera-localization');

// Existing stock expressions only; never edit the shared localization archive.
function verifyScoreboardTargets(english,targets){
 const text=readZipArchive(english).get('TEXT.VCLOCALIZEDATA');
 if(!text||!Array.isArray(targets)||!targets.length)throw Error('Missing scoreboard localization references.');
 for(const target of targets){
  if(!/^[a-fA-F0-9]{1,16}$/.test(target.key)||typeof target.text!=='string'||resolve(text,target.key).value!==target.text)throw Error('Stock scoreboard localization changed; adaptation stopped.');
 }
 return true;
}
function buildScoreboardArchive(source,layout){
 if(!layout||!Array.isArray(layout.plans)||!layout.plans.length)throw Error('Missing scoreboard archive plan.');
 const original=readZipArchive(source),entries=new Map(original),seen=new Set();
 for(const plan of layout.plans){
  if(typeof plan.member!=='string'||!/\.VCUIELEMENT$/i.test(plan.member)||seen.has(plan.member)||!entries.has(plan.member))throw Error('Invalid scoreboard archive member.');
  seen.add(plan.member);
  entries.set(plan.member,patchScoreboardMember(original.get(plan.member),plan));
 }
 const built=buildZipArchive([...entries].map(([name,data])=>({name,data}))),verified=readZipArchive(built);
 if(verified.size!==original.size)throw Error('Scoreboard archive membership changed.');
 for(const [name,expected]of entries)if(!verified.get(name)?.equals(expected))throw Error('Scoreboard archive verification failed.');
 return built;
}
module.exports={buildScoreboardArchive,verifyScoreboardTargets};
