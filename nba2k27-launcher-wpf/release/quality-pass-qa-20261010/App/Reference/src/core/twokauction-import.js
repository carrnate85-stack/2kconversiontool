'use strict';
const common=require('./twokdb-import');
function url(input){
 let u;try{u=new URL(input);}catch{throw Error('Paste a 2KAuction MyNBA player link.');}
 const match=/^\/mynba\/(?:(2k\d{2})\/)?players\/[a-z0-9]+(?:-[a-z0-9]+)*\/?$/.exec(u.pathname);
 if(u.protocol!=='https:'||u.hostname!=='2kauction.com'||u.port||u.username||u.password||!match)throw Error('Use a 2KAuction MyNBA player link, with an optional game version before /players/.');
 for(const key of new Set(u.searchParams.keys())){
  const values=u.searchParams.getAll(key),v=values[0];
  if(values.length!==1||!(key==='game'?/^2k\d{2}$/.test(v):key==='roster'?/^[a-z0-9_-]{1,80}$/.test(v):key==='version'?/^\d{1,10}$/.test(v):false))throw Error('Unsupported or ambiguous 2KAuction game/roster selection.');
 }
 if(match[1]&&u.searchParams.has('game')&&u.searchParams.get('game')!==match[1])throw Error('Conflicting 2KAuction game versions in the path and query.');
 u.hash='';return u.href;
}
function extract(input,address){
 if(typeof input!=='string'||input.length>4000000)throw Error('Player page is too large.');
 const u=new URL(url(address)),slug=u.pathname.replace(/\/$/,'').split('/').pop();
 const requestedGame=/^\/mynba\/(2k\d{2})\//.exec(u.pathname)?.[1]||u.searchParams.get('game');
 const stream=[...input.matchAll(/self\.__next_f\.push\(\[1,("(?:\\.|[^"\\])*")\]\)/g)].map(m=>JSON.parse(m[1])).join('');
 const found=[];let visited=0;
 function walk(v,depth=0){
  if(!v||typeof v!=='object'||depth>80)return;if(++visited>100000)throw Error('Player page structure is too large.');
  if(v.page?.selected&&typeof v.game==='string'){found.push(v);return;}
  for(const child of Object.values(v))walk(child,depth+1);
 }
 for(const line of stream.split('\n')){const colon=line.indexOf(':');if(colon<0)continue;let v;try{v=JSON.parse(line.slice(colon+1));}catch{continue;}walk(v);}
 const matches=found.filter(v=>v.page.person_slug===slug&&v.page.selected.person_slug===slug
  &&/^2k\d{2}$/.test(v.game)&&(!requestedGame||requestedGame===v.game)
  &&(!u.searchParams.has('roster')||u.searchParams.get('roster')===v.page.selected.roster_key)
  &&(!u.searchParams.has('version')||u.searchParams.get('version')===String(v.page.selected.roster_version_id)));
 if(matches.length!==1)throw Error('No unique matching 2KAuction player/game/roster data found. Let the selected page finish loading.');
 return {player:matches[0].page.selected,game:matches[0].game};
}
function map(snapshot,address){
 const s=snapshot.player;
 if(!s||typeof s.display_name!=='string'||!s.attributes||typeof s.attributes!=='object'||Array.isArray(s.attributes))throw Error('The selected snapshot has no usable attributes.');
 const p={};
 for(const group of ['attributes','tendencies','badges','hot_zones']){
  const values=s[group];if(values&&typeof values==='object'&&!Array.isArray(values))for(const [k,v] of Object.entries(values))if(typeof v==='number'||v===null)p[k]=v;
 }
 Object.assign(p,{name:s.display_name,first_name:s.first_name,last_name:s.last_name,version:snapshot.game.slice(2),height:s.height_cm,wingspan:s.wingspan_cm,weight:s.weight_lb,position:s.position,secondary_position:s.secondary_position,college:s.from_college??s.college??s.bio?.from_college});
 // Null means unspecified, not a request to erase the player's secondary position.
 const result=common.map(p,address,{nativeNames:true});
 result.source='2KAuction';result.rosterLabel=s.roster_label||s.roster_key;
 result.warnings=['Source: '+result.rosterLabel+'. Older-game fields/badges may not exist in NBA 2K27; only matching supported fields are imported.','Animation names/IDs are reference-only; cross-version animation IDs are not verified.'];
 if(!s.hot_zones||!Object.keys(s.hot_zones).length)result.warnings.push('This snapshot supplies no hot zones. Your current hot zones remain unchanged.');
 const used=new Set(result.rows.map(r=>r.source));
 result.reference=[];
 function flatten(v,path){if(v===null||v===undefined)return;if(v&&typeof v==='object'&&!Array.isArray(v)){for(const [k,x] of Object.entries(v))flatten(x,path?path+'.'+k:k);}else result.reference.push({key:path,value:(typeof v==='object'?JSON.stringify(v):String(v)).slice(0,2000)});}
 for(const [key,value] of Object.entries(s)){
  if(['attributes','tendencies','badges','hot_zones'].includes(key)){for(const [k,v] of Object.entries(value||{}))if(!used.has(k))flatten(v,key+'.'+k);}
  else if(!['display_name','first_name','last_name','height_cm','wingspan_cm','weight_lb','position','secondary_position','from_college','college'].includes(key))flatten(value,key);
 }
 return result;
}
module.exports={url,extract,map};
