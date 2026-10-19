'use strict';
const {loadIff,readZipEntry,parseScneText}=require('../vendor/archive-preview/core/iff');
function translation(rig,name){
 const entries=Object.entries(rig),seen=new Set();let index=entries.findIndex(([key])=>key===name);const result=[0,0,0];
 if(index<0)throw Error('Missing attachment bone.');
 while(index>=0){
  if(index>=entries.length||seen.has(index))throw Error('Invalid attachment hierarchy.');seen.add(index);
  const bone=entries[index][1];
  if(bone.Rotate||bone.Scale||bone.Matrix)throw Error('Non-translation bind pose is unsupported.');
  const offset=bone.Translate||[0,0,0];if(!Array.isArray(offset)||offset.length!==3||!offset.every(Number.isFinite))throw Error('Invalid bind translation.');
  for(let axis=0;axis<3;axis++)result[axis]+=offset[axis];index=bone.Parent??-1;
 }
 return result;
}
async function readHeadRig(file){
 const iff=await loadIff(file),rigs=[];
 const visit=node=>{if(!node||typeof node!=='object')return;if(node.Transform?.def_head&&node.Transform?.def_spine2)rigs.push(node.Transform);for(const value of Object.values(node))visit(value);};
 for(const entry of iff.entries.filter(e=>/\.scne$/i.test(e.name)))visit(parseScneText(readZipEntry(iff.buffer,entry,true)));
 if(rigs.length!==1)throw Error('Ambiguous head attachment skeleton.');
 return {head:translation(rigs[0],'def_head'),spine:translation(rigs[0],'def_spine2')};
}
function headOffset(source,target){
 const delta=target.head.map((v,i)=>v-source.head[i]),spineDelta=target.spine.map((v,i)=>v-source.spine[i]);
 if(!delta.every(Number.isFinite)||delta.some((v,i)=>Math.abs(v-spineDelta[i])>0.001)||Math.hypot(...delta)>100)throw Error('Head bind poses are incompatible.');
 // Game XYZ -> preview X,-Z,Y. No guessed scale or face/bone offsets.
 return [delta[0],-delta[2],delta[1]];
}
function translateMeshes(meshes,offset){return meshes.map(mesh=>{const positions=Float32Array.from(mesh.positions);for(let i=0;i<positions.length;i++)positions[i]+=offset[i%3];return {...mesh,positions};});}
module.exports={readHeadRig,headOffset,translateMeshes,translation};
