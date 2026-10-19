'use strict';
function need(ok,message){if(!ok)throw Error('Clothing table: '+message);}
function u32(b,p){need(p>=0&&p+4<=b.length,'truncated record');return b.readUInt32LE(p);}
function ref(b,p){need(p>=0&&p+8<=b.length,'truncated reference');const n=b.readBigUInt64LE(p);need(n>0n&&n<=BigInt(Number.MAX_SAFE_INTEGER),'invalid reference');const t=p+Number(n)-1;need(t>p&&t<b.length,'reference outside table');return t;}
function string(b,p){const end=b.indexOf(0,p);need(end>p&&end-p<=256,'invalid string length');const value=b.subarray(p,end).toString('ascii');need(/^[a-zA-Z0-9_: .-]+$/.test(value),'invalid resource name');return value;}
function resolveItems(casuals,items,names,limit){
 need(u32(casuals,0)===0x3a70f295&&u32(items,0)===0x6e2fad8b,'unsupported schema');
 const wanted=new Set(names);need(wanted.size>0&&wanted.size<=limit,'invalid selection size');
 for(const n of wanted)need(typeof n==='string'&&/^[a-zA-Z0-9_]{1,200}$/.test(n),'invalid selected name');
 const count=u32(casuals,24),base=ref(casuals,32);need(count>0&&count<=200000&&base+count*24<=casuals.length,'invalid name records');
 const ids=new Map();
 for(let i=0;i<count;i++){
  const p=base+i*24,name=string(casuals,ref(casuals,p));if(!wanted.has(name))continue;
  const id=u32(casuals,p+8);need(!ids.has(id),'duplicate selected ID');ids.set(id,{asset:name,hashA:u32(casuals,p+12),hashB:u32(casuals,p+16)});
 }
 const total=u32(items,28),start=ref(items,32);need(total>0&&total<=200000&&start+total*40<=items.length,'invalid item records');
 const result=new Map();
 for(let i=0;i<total;i++){
  const p=start+i*40,entry=ids.get(u32(items,p+4));if(!entry)continue;
  need(u32(items,p)===0x6d762063,'unsupported item record');
  need(u32(items,p+8)===entry.hashA&&u32(items,p+12)===entry.hashB,'item identity mismatch');
  const size=u32(items,p+28);need(size>0&&size<=64,'invalid dependency count');const array=ref(items,p+32);need(array+size*8<=items.length,'truncated dependencies');
  const dependencies=[];
  try{for(let j=0;j<size;j++){const target=ref(items,array+j*8),kind=items[target],name=string(items,target+1);dependencies.push({kind,name});}}
  catch(error){if(limit===8)throw error;dependencies.length=0;}
  need(!result.has(entry.asset),'duplicate item');result.set(entry.asset,{asset:entry.asset,id:u32(items,p+4),dependencies});
 }
 return result;
}
function resolveClothingItems(casuals,items,names){return resolveItems(casuals,items,names,8);}
// Offline catalog discovery only; runtime selection retains its eight-item bound.
function resolveClothingInventory(casuals,items,names){return resolveItems(casuals,items,names,200000);}
module.exports={resolveClothingItems,resolveClothingInventory};
