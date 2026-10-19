// Executes the preserved WPF planning body against owned synthetic archives.
// This never launches a game, reads a live process or edits the user's library.
import fs from "node:fs";
import fsp from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import crypto from "node:crypto";
import zlib from "node:zlib";
import { fileURLToPath } from "node:url";
import { createRequire } from "node:module";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), ".."),
  source = path.resolve(root, "../nba2k27-mod-launcher/src/core"),
  require = createRequire(import.meta.url),
  manager = fs.readFileSync(path.join(source, "multi-config-manager.js"), "utf8"),
  { scanModConfigs, suffixKey, readReplacementDefinition, buildRenderRegistry } = require(path.join(source, "multi-config-manager.js")),
  { readRecords, buildTableWithClones, replaceConfigLists, relativePointer } = require(path.join(source, "sigconfig-table.js")),
  { buildZipArchive, readZipArchive, crc32 } = require(path.join(source, "jukebox-manager.js"));
const start = manager.indexOf("    const allRecords = new Map"), end = manager.indexOf("    const generationKey =", start);
if (start < 0 || end < start) throw Error("WPF generation planner boundaries changed");
const plan = new (Object.getPrototypeOf(async function(){}).constructor)("mods", "stock", "allSource", "withoutSource", "withSource", "readRecords", "suffixKey", "readReplacementDefinition", manager.slice(start, end) + "\nreturn {eligible, skipped, replacements};");
function table(records) {
  const rows = records.toSorted((a,b) => a.id-b.id), header = 16 + 16*rows.length;
  const output = Buffer.alloc(header + rows.reduce((n,r) => n+r.names.length*32, 0));
  output.writeUInt32LE(rows.length,0); output.writeUInt32LE(9,8);
  let cursor=header;
  rows.forEach((row,i) => {
    const offset=16+i*16, payload=cursor+row.names.length*16;
    output.writeUInt32LE(row.id,offset); output.writeUInt32LE(row.names.length,offset+4);
    output.writeBigUInt64LE(relativePointer(offset+8,cursor),offset+8);
    row.names.forEach((name,j) => {const item=cursor+j*16,target=payload+j*16;output.writeBigUInt64LE(relativePointer(item,target),item);output.writeUInt32LE(crc32(Buffer.from(name.toUpperCase())),target);});
    cursor=payload+row.names.length*16;
  });
  return output;
}
const all=table([{id:21,names:["DEFAULT"]},{id:4090,names:["AFRO","SHORT","HEADBAND_FURY"]}]),
  without=table([{id:21,names:["DEFAULT"]},{id:4090,names:["AFRO","SHORT"]}]),
  withBand=table([{id:21,names:["DEFAULT"]},{id:4090,names:["AFRO","HEADBAND_FURY"]}]),
  registry=buildZipArchive([{name:"fpplayer_global_table.json",data:Buffer.from('{ { "id":"00021" }, { "id":"04090" } }')},...[21,4090].map(id=>({name:`${String(id).padStart(5,"0")}_modified.json`,data:Buffer.from(JSON.stringify({appearance:{"skin color":"4"}, untouched:id}))}))]);
const stockGroups = [[4090,["afro","short","headband_fury"]]],
  sources=[buildZipArchive([{name:"All.SigConfigTable",data:all}]),buildZipArchive([{name:"EditPlayer_WithoutHeadband.SigConfigTable",data:without},{name:"EditPlayer_WithHeadband.SigConfigTable",data:withBand}]),registry];
const directory=await fsp.mkdtemp(path.join(root,"artifacts/selector-plan-oracle-"));
const cases=[];
async function run(name, populate) {
  const modsRoot=path.join(directory,name);await fsp.mkdir(modsRoot);
  async function file(relative,content=Buffer.from("config")){const target=path.join(modsRoot,relative);await fsp.mkdir(path.dirname(target),{recursive:true});await fsp.writeFile(target,content);}
  async function player(id,names,{folder="char/sig",info}={}) {
    await file(`${folder}/png${String(id).padStart(5,"0")}.iff`,buildZipArchive([{name:"appearance_info.json",data:Buffer.from(JSON.stringify({accessory_items:info || {default_config:names[0],configurations:names.map(name=>({name}))}}))}]));
    for(const suffix of names)await file(`${folder}/png${String(id).padStart(5,"0")}_config_${suffix}.iff`);
  }
  await populate({file,player});
  const mods=await scanModConfigs(modsRoot), stock={baseIds:new Set([21,4090]),groups:new Map(stockGroups.map(([id,names])=>[id,new Set(names)]))};
  const expected=await plan(mods,stock,all,without,withBand,readRecords,suffixKey,readReplacementDefinition);
  const inputs={};
  for(const [id,suffixes]of mods.groups) {
    const files=mods.baseFiles.get(id)||[],bases=[];
    for(const filePath of files){try{const entries=readZipArchive(await fsp.readFile(filePath));const entry=[...entries].find(([n])=>n.toLowerCase()==="appearance_info.json");if(!entry)throw Error("No appearance_info.json");bases.push({path:path.relative(modsRoot,filePath).replaceAll("\\","/"),info:JSON.parse(entry[1].toString()).accessory_items,error:null});}catch(error){bases.push({path:path.relative(modsRoot,filePath).replaceAll("\\","/"),info:null,error:error.message});}}
    inputs[id]={suffixes:[...suffixes].sort(),basePaths:files.map(f=>path.relative(modsRoot,f).replaceAll("\\","/")).sort(),bases};
  }
  let output=null;
  if(expected.eligible.length||expected.replacements.length){
    const clones=expected.eligible.map(e=>({targetId:e.id,donorId:e.donorId}));
    const generated=[replaceConfigLists(buildTableWithClones(all,clones).data,expected.replacements),replaceConfigLists(buildTableWithClones(without,clones).data,expected.replacements.map(r=>({id:r.id,names:r.withoutHeadband}))),replaceConfigLists(buildTableWithClones(withBand,clones).data,expected.replacements.map(r=>({id:r.id,names:r.withHeadband})))];
    const rendered=readZipArchive(buildRenderRegistry(registry,expected.replacements));
    output={tables:generated.map(b=>[...b]),registry:[...rendered].map(([name,data])=>({name,bytes:[...data]}))};
  }
  cases.push({name,inputs,sources:sources.map(b=>[...b]),stock:{bases:[21,4090],configs:stockGroups},expected:{eligible:expected.eligible.map(e=>({id:e.id,donorId:e.donorId,configCount:e.configCount,suffixes:e.suffixes})),replacements:expected.replacements.map(r=>r.id),skipped:expected.skipped.map(r=>({id:r.id,reason:r.reason})),output}});
}
await run("empty",async()=>{});
await run("unique",async({player})=>player(10000,["AFRO","SHORT","HEADBAND_FURY"]));
await run("several-unique",async({player})=>{await player(10001,["headband_fury","short","afro"]);await player(10000,["AFRO","SHORT","HEADBAND_FURY"]);});
await run("replacement",async({player})=>player(4090,["CUSTOM","BAND"],{info:{default_config:"CUSTOM",items:[{name:"band",type:"headband"}],configurations:[{name:"CUSTOM"},{name:"BAND",items:["band"],fallback:"CUSTOM"}]}}));
await run("unique-and-replacement",async({player})=>{await player(10000,["AFRO","SHORT","HEADBAND_FURY"]);await player(4090,["CUSTOM","HEADBAND_CUSTOM"]);});
await run("missing-base",async({file})=>file("char/sig/png10000_config_AFRO.iff"));
await run("single-config",async({player})=>player(10000,["AFRO"]));
await run("no-donor",async({player})=>player(10000,["AFRO","CUSTOM"]));
await run("over-limit",async({player})=>player(10000,Array.from({length:10},(_,i)=>`STYLE_${i}`)));
await run("stock-no-base",async({file})=>file("char/sig/png04090_config_AFRO.iff"));
await run("duplicate-identical",async({player})=>{await player(4090,["CUSTOM","HEADBAND_CUSTOM"]);await player(4090,["CUSTOM","HEADBAND_CUSTOM"],{folder:"players/Test"});});
await run("duplicate-conflicting",async({player})=>{await player(4090,["CUSTOM","HEADBAND_CUSTOM"]);await player(4090,["CUSTOM","HEADBAND_CUSTOM"],{folder:"players/Test",info:{default_config:"HEADBAND_CUSTOM",configurations:[{name:"CUSTOM"},{name:"HEADBAND_CUSTOM"}]}});});
await run("missing-reference",async({player})=>player(4090,["CUSTOM"],{info:{configurations:[{name:"MISSING"}]}}));
await run("bad-default",async({player})=>player(4090,["CUSTOM"],{info:{default_config:"MISSING",configurations:[{name:"CUSTOM"}]}}));
await run("bad-base-archive",async({file})=>{await file("char/sig/png04090.iff",Buffer.from("not a zip"));await file("char/sig/png04090_config_AFRO.iff");});
const report={schema:"nba2k27-selector-generation-oracle-v1",sourceSha256:crypto.createHash("sha256").update(manager).digest("hex"),cases};
fs.writeFileSync(path.join(root,"assets/selector-generation-regressions.json.gz"),zlib.gzipSync(JSON.stringify(report),{level:9}));
console.log(JSON.stringify({cases:cases.length,sourceSha256:report.sourceSha256,fixture:directory,gameAccess:false,userModFilesChanged:false}));
