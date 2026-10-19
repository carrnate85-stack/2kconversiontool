'use strict';
const https=require('node:https'),math=require('../renderer/player-season-math'),transfer=require('../renderer/player-season-transfer');
const MAX_BYTES=4*1024*1024;
const teams=Object.freeze({ATL:'Hawks',BOS:'Celtics',BRK:'Nets',NJN:'Nets',CHA:'Hornets',CHO:'Hornets',CHH:'Hornets',CHI:'Bulls',CLE:'Cavaliers',DAL:'Mavericks',DEN:'Nuggets',DET:'Pistons',GSW:'Warriors',HOU:'Rockets',IND:'Pacers',LAC:'Clippers',LAL:'Lakers',MEM:'Grizzlies',VAN:'Grizzlies',MIA:'Heat',MIL:'Bucks',MIN:'Timberwolves',NOP:'Pelicans',NOH:'Pelicans',NOK:'Pelicans',NYK:'Knicks',OKC:'Thunder',ORL:'Magic',PHI:'76ers',PHO:'Suns',PHX:'Suns',POR:'Trail Blazers',SAC:'Kings',SAS:'Spurs',TOR:'Raptors',UTA:'Jazz',WAS:'Wizards',WSB:'Wizards'});
const columns=Object.freeze({gp:['g','games'],gs:['gs','games_started'],minutes:['mp','minutes'],fgm:['fg','fgm'],fga:['fga'],threeMade:['fg3','fg3m'],threeAttempted:['fg3a'],ftm:['ft','ftm'],fta:['fta'],oreb:['orb'],dreb:['drb'],assists:['ast'],steals:['stl'],blocks:['blk'],turnovers:['tov'],fouls:['pf'],points:['pts']});
function playerUrl(value){
 if(typeof value!=='string'||value.length>500)throw Error('Paste a Basketball-Reference NBA player URL.');
 let url;try{url=new URL(value.trim());}catch{throw Error('Paste a Basketball-Reference NBA player URL.');}
 if(url.protocol!=='https:'||!['basketball-reference.com','www.basketball-reference.com'].includes(url.hostname)||url.username||url.password||url.port||!/^\/players\/[a-z]\/[a-z0-9]+\.html$/.test(url.pathname))throw Error('Use an https://www.basketball-reference.com/players/… .html NBA player URL.');
 url.hostname='www.basketball-reference.com';url.search='';url.hash='';return url.href;
}
function text(html){return html.replace(/<[^>]*>/g,'').replace(/&(#x[0-9a-f]+|#\d+|amp|lt|gt|quot|apos|nbsp|ndash|mdash);/gi,(_,entity)=>{const names={amp:'&',lt:'<',gt:'>',quot:'"',apos:"'",nbsp:' ',ndash:'–',mdash:'—'};if(entity[0]!=='#')return names[entity.toLowerCase()];const n=entity[1].toLowerCase()==='x'?parseInt(entity.slice(2),16):Number(entity.slice(1));return n>0&&n<=0x10ffff?String.fromCodePoint(n):'';}).replace(/\s+/g,' ').trim();}
function attribute(tag,name){const match=tag.match(new RegExp('\\b'+name+'\\s*=\\s*(?:"([^"]*)"|\'([^\']*)\'|([^\\s>]+))','i'));return match?.[1]??match?.[2]??match?.[3]??'';}
function parse(html,url){
 if(typeof html!=='string'||Buffer.byteLength(html)>MAX_BYTES)throw Error('Basketball-Reference page is too large.');
 // Comments can contain the site's lazy tables. They remain inert text; no HTML
 // is ever executed or passed to the renderer, only bounded numeric records.
 const tables=[...html.matchAll(/<table\b([^>]*)>([\s\S]*?)<\/table>/gi)].filter(m=>['totals','totals_stats'].includes(attribute(m[1],'id')));
 if(tables.length!==1)throw Error('No unique NBA regular-season totals table was found. The page may be blocked or its format changed.');
 const h1=html.match(/<h1\b[^>]*>([\s\S]*?)<\/h1>/i),name=text(h1?.[1]||'');if(!name||name.length>120)throw Error('The player name could not be verified.');
 const years=new Map(),warnings=[];
 for(const match of tables[0][2].matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/gi)){
  const cells={};for(const cell of match[1].matchAll(/<(?:td|th)\b([^>]*)>([\s\S]*?)<\/(?:td|th)>/gi)){const key=attribute(cell[1],'data-stat');if(key){if(Object.hasOwn(cells,key))throw Error('Duplicate totals column.');cells[key]=text(cell[2]);}}
  const label=cells.season??cells.year_id;if(!/^\d{4}[-–]\d{2}$/.test(label||''))continue;
  const year=Number(label.slice(0,4))+1;if(Number(label.slice(-2))!==year%100||year<1900||year>2155)throw Error('Invalid season year.');
  const league=cells.lg_id??cells.comp_name_abbr??cells.league_id;if(league!=='NBA'){warnings.push(`${label}: non-NBA row skipped.`);continue;}
  const abbreviation=cells.team_id??cells.team_name_abbr;if(!abbreviation)throw Error('A season team is missing.');
  const totals=Object.fromEntries(Object.keys(math.limits).map(key=>[key,0])),missingFields=[];
  for(const [key,max] of Object.entries(math.limits)){
   const aliases=columns[key],raw=aliases?.map(alias=>cells[alias]).find(v=>v!==undefined&&v!=='');
   if(raw===undefined){missingFields.push(key);continue;}
   const normalized=raw.replaceAll(',','');if(!/^\d+$/.test(normalized)||Number(normalized)>max)throw Error(`${label}: unsupported ${key} total (${raw}). Nothing was imported.`);
   totals[key]=Number(normalized);
  }
  for(const key of ['gp','minutes','points','fgm','fga','ftm','fta'])if(missingFields.includes(key))throw Error(`${label}: required total ${key} is missing.`);
  if(!totals.gp)continue; // No played season / unfinished placeholder.
  const aggregate=/^(TOT|[2-9]TM)$/.test(abbreviation),row={year,label,team:teams[abbreviation]||abbreviation,abbreviation,aggregate,totals,missingFields};
  const list=years.get(year)||[];list.push(row);years.set(year,list);
 }
 const seasons=[];
 for(const [year,rows] of years){const aggregate=rows.filter(row=>row.aggregate);if(aggregate.length>1||!aggregate.length&&rows.length!==1)throw Error(`${year}: no unique aggregate season totals. Nothing was imported.`);const row=aggregate[0]??rows[0];transfer.validate([{year,team:row.team,totals:row.totals}]);seasons.push(row);}
 if(!seasons.length||seasons.length>40)throw Error('No supported NBA regular-season history was found.');
 return {name,url:playerUrl(url),seasons:seasons.sort((a,b)=>b.year-a.year),warnings:[...new Set(warnings)],source:'Basketball-Reference',awardsSupported:false};
}
function download(url,{signal,redirects=0}={}){
 return new Promise((resolve,reject)=>{
  const request=https.get(playerUrl(url),{signal,headers:{'User-Agent':'2k-Launcher/0.37.85 (season stats import)','Accept':'text/html','Accept-Encoding':'identity'}},response=>{
   if([301,302,303,307,308].includes(response.statusCode)){response.resume();if(redirects>=2)return reject(Error('Too many player-page redirects.'));let target;try{target=playerUrl(new URL(response.headers.location,url).href);}catch(error){reject(error);return;}download(target,{signal,redirects:redirects+1}).then(resolve,reject);return;}
   if(response.statusCode!==200){response.resume();return reject(Error([403,429].includes(response.statusCode)?`Basketball-Reference blocked this request (HTTP ${response.statusCode}). Try again later. No stats were imported.`:`Basketball-Reference returned HTTP ${response.statusCode}. No stats were imported.`));}
   if(!/^text\/html\b/i.test(response.headers['content-type']||'')){response.resume();return reject(Error('The player URL did not return an HTML page.'));}
   const chunks=[];let size=0;response.on('data',chunk=>{size+=chunk.length;if(size>MAX_BYTES)request.destroy(Error('Basketball-Reference page is too large.'));else chunks.push(chunk);});response.on('end',()=>resolve(Buffer.concat(chunks).toString('utf8')));response.on('error',reject);
  });request.setTimeout(15000,()=>request.destroy(Error('Basketball-Reference request timed out. Try again later.')));request.on('error',reject);
 });
}
class BasketballReferenceStats{
 constructor({fetchPage=download,now=Date.now}={}){this.fetchPage=fetchPage;this.now=now;this.cache=new Map();this.busy=false;this.lastRequest=-Infinity;}
 async load(value){
  const url=playerUrl(value),cached=this.cache.get(url);if(cached&&this.now()-cached.at<600000)return structuredClone(cached.data);
  if(this.busy)throw Error('A player-page request is already running.');if(this.now()-this.lastRequest<5000)throw Error('Please wait a few seconds before requesting another player page.');
  this.busy=true;this.lastRequest=this.now();try{const html=await this.fetchPage(url,{signal:AbortSignal.timeout(20000)}),data=parse(html,url);this.cache.set(url,{at:this.now(),data});while(this.cache.size>4)this.cache.delete(this.cache.keys().next().value);return structuredClone(data);}finally{this.busy=false;}
 }
}
module.exports={BasketballReferenceStats,playerUrl,parse,download,MAX_BYTES};
