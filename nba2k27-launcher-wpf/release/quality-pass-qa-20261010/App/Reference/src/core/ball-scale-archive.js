'use strict';
const crypto=require('node:crypto');
const {readZipArchive,buildZipArchive}=require('./jukebox-manager');
const FACTOR=.97;
const hash=b=>crypto.createHash('sha256').update(b).digest('hex');
const numeric=n=>typeof n==='number'&&Number.isFinite(n);
const scaled=n=>Number((n*FACTOR).toPrecision(9));

function scene(source){
 if(!Buffer.isBuffer(source)||source.length>64*1024*1024)throw Error('Unsupported ball archive size.');
 const entries=readZipArchive(source),names=[...entries.keys()].filter(n=>/\.scne$/i.test(n));
 if(names.length!==1)throw Error('Expected one ball scene.');
 const name=names[0],text=entries.get(name).toString('utf8');
 if(text.length>4*1024*1024)throw Error('Ball scene too large.');
 // Current exported SCNE is a named JSON property without outer braces.
 const wrapped=!text.trimStart().startsWith('{');
 const doc=JSON.parse(wrapped?'{'+text+'}':text),roots=Object.values(doc);
 if(roots.length!==1||!roots[0]?.Model)throw Error('Unsupported ball scene root.');
 const root=roots[0],models=root.Model;
 if(!Object.keys(models).length||Object.keys(models).length>8||root.Skeleton||root.Transform)throw Error('Animated/transformed ball scene is unsupported.');
 for(const model of Object.values(models)){
  const pos=model.VertexFormat?.POSITION0;
  if(pos?.Format!=='R16G16B16A16_SNORM'||!Array.isArray(model.Prim)||!model.Prim.length||model.Prim.length>32)throw Error('Unsupported ball geometry.');
  for(const item of [model,...model.Prim]){
   if(!numeric(item.Radius)||item.Radius<=0||item.Radius>100)throw Error('Unsupported ball bounds.');
   for(const key of ['Center','Min','Max'])if(!Array.isArray(item[key])||item[key].length!==3||!item[key].every(numeric))throw Error('Incomplete ball bounds.');
  }
  for(const key of ['Scale','Offset'])if(!Array.isArray(pos[key])||pos[key].length!==4||!pos[key].every(numeric))throw Error('Unsupported ball position encoding.');
  if(pos.Scale[3]!==1||pos.Offset[3]!==0||pos.Scale.slice(0,3).some(n=>n<=0))throw Error('Unsupported ball position multiplier.');
  if(!model.Transform||Object.values(model.Transform).some(v=>v!==null))throw Error('Non-identity ball transform is unsupported.');
  if(!Array.isArray(model.VertexStream)||model.VertexStream.length>8)throw Error('Unsupported ball vertex streams.');
  for(const stream of [...model.VertexStream,model.IndexBuffer])if(!stream||typeof stream.Binary!=='string'||!entries.has(stream.Binary)||entries.get(stream.Binary).length!==stream.Size)throw Error('Incomplete ball geometry buffers.');
  const positionStream=model.VertexStream[pos.Stream||0];
  if(positionStream?.Stride!==8||positionStream.Size%8)throw Error('Unsupported ball position stream.');
 }
 return {entries,name,text,doc,root,models,wrapped};
}
function shapeIdentity(source){
 const s=scene(source);
 const buffers=[...s.entries].filter(([n])=>/^(?:VertexBuffer|IndexBuffer)\..*\.bin$/i.test(n)).sort(([a],[b])=>a.localeCompare(b));
 return hash(JSON.stringify(s.models)+buffers.map(([n,b])=>n+hash(b)).join('|'));
}
function buildScaledBall(source,{alreadyScaledShapes=[]}={}){
 const identity=shapeIdentity(source);
 if(alreadyScaledShapes.includes(identity))return {bytes:source,alreadyScaled:true,shape:identity,factor:1};
 const s=scene(source),models=structuredClone(s.models);
 for(const m of Object.values(models)){
  for(const item of [m,...m.Prim]){item.Radius=scaled(item.Radius);for(const key of ['Center','Min','Max'])item[key]=item[key].map(scaled);}
  for(const key of ['Scale','Offset'])m.VertexFormat.POSITION0[key]=m.VertexFormat.POSITION0[key].map((n,i)=>i<3?scaled(n):n);
 }
 // Replace only Model's JSON value, retaining every other scene byte.
 const matches=[...s.text.matchAll(/"Model"\s*:\s*\{/g)];
 if(matches.length!==1)throw Error('Ambiguous Model property.');
 const start=matches[0].index+matches[0][0].length-1;let depth=0,string=false,escape=false,end=-1;
 for(let i=start;i<s.text.length;i++){const c=s.text[i];if(string){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"')string=false;}else if(c==='"')string=true;else if(c==='{')depth++;else if(c==='}'&&!--depth){end=i+1;break;}}
 if(end<0||JSON.stringify(JSON.parse(s.text.slice(start,end)))!==JSON.stringify(s.models))throw Error('Ball Model span rejected.');
 const text=s.text.slice(0,start)+JSON.stringify(models,null,'\t')+s.text.slice(end);
 const verifiedDoc=JSON.parse(s.wrapped?'{'+text+'}':text);Object.values(verifiedDoc)[0].Model=s.models;
 if(JSON.stringify(verifiedDoc)!==JSON.stringify(s.doc))throw Error('Non-geometry scene data changed.');
 const output=new Map(s.entries);output.set(s.name,Buffer.from(text));
 const bytes=buildZipArchive([...output].map(([name,data])=>({name,data}))),verify=readZipArchive(bytes);
 if(verify.size!==s.entries.size||[...output].some(([n,b])=>!verify.get(n)?.equals(b)))throw Error('Scaled ball archive verification failed.');
 return {bytes,alreadyScaled:false,shape:shapeIdentity(bytes),factor:FACTOR,models:Object.keys(models).length};
}
module.exports={buildScaledBall,shapeIdentity,scene,FACTOR};
