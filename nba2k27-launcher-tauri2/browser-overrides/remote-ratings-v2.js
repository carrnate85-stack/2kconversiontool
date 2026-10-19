(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.SavedRatingsSource=factory();})(typeof window==='object'?window:globalThis,()=>{
 'use strict';
 const MAX=4000000,norm=s=>s.toLowerCase().replace(/[^a-z0-9]/g,'');
 function address(input){
  let u;try{u=new URL(input);}catch{throw Error('Enter the original player page URL.');}
  if(u.protocol!=='https:'||u.port||u.username||u.password||u.hash)throw Error('Use an HTTPS player URL without credentials, ports or fragments.');
  if(u.hostname==='2kdb.net'){
   if(!/^\/player\/[1-9]\d{0,11}$/.test(u.pathname)||u.search)throw Error('Use a 2KDB /player/number URL without query parameters.');
   return {source:'2KDB',url:u.href,id:u.pathname.split('/').pop()};
  }
  const m=/^\/mynba\/(?:(2k\d{2})\/)?players\/[a-z0-9]+(?:-[a-z0-9]+)*\/?$/.exec(u.pathname);
  if(u.hostname!=='2kauction.com'||!m)throw Error('Use a 2KDB or 2KAuction MyNBA player URL.');
  for(const k of new Set(u.searchParams.keys())){
   const a=u.searchParams.getAll(k);if(a.length!==1||!(k==='game'?/^2k\d{2}$/.test(a[0]):k==='roster'?/^[a-z0-9_-]{1,80}$/.test(a[0]):k==='version'?/^\d{1,10}$/.test(a[0]):false))throw Error('Ambiguous or unsupported game/roster selection.');
  }
  if(m[1]&&u.searchParams.has('game')&&m[1]!==u.searchParams.get('game'))throw Error('Conflicting game selections.');
  return {source:'2KAuction',url:u.href,id:u.pathname.replace(/\/$/,'').split('/').pop(),game:m[1]||u.searchParams.get('game'),roster:u.searchParams.get('roster'),version:u.searchParams.get('version')};
 }
 function extract(input,inputUrl){
  const a=address(inputUrl);if(typeof input!=='string'||new TextEncoder().encode(input).length>MAX)throw Error('Saved page data exceeds 4 MB.');
  let budget=MAX*4,nodes=0;const found=[],documents=[input];let decodedBytes=0;
  for(const m of input.matchAll(/self\.__next_f\.push\(\[1,("(?:\\.|[^"\\])*")\]\)/g)){
   if(documents.length>=513)throw Error('Too many page transport chunks.');
   let s;try{s=JSON.parse(m[1]);}catch{continue;}
   decodedBytes+=new TextEncoder().encode(s).length;if(decodedBytes>MAX)throw Error('Decoded page data exceeds 4 MB.');documents.push(s);
  }
  const parsed=new Set();
  function tree(v){
   const pending=[[v,0]];while(pending.length){const [x,d]=pending.pop();if(++nodes>100000||d>64)throw Error('Page data is too complex.');if(!x||typeof x!=='object')continue;
    if(a.source==='2KDB'&&String(x.relationId)===a.id&&typeof x.name==='string')found.push(x);
    if(a.source==='2KAuction'&&x.page?.selected&&x.page.person_slug===a.id&&x.page.selected.person_slug===a.id&&/^2k\d{2}$/.test(x.game)&&(!a.game||x.game===a.game)&&(!a.roster||x.page.selected.roster_key===a.roster)&&(!a.version||String(x.page.selected.roster_version_id)===a.version))found.push(x);
    for(const child of Object.values(x))if(child&&typeof child==='object')pending.push([child,d+1]);
   }
  }
  function jsonAt(text,start){
   let quoted=false,escape=false,depth=0;
   for(let i=start;i<text.length;i++){
    if(--budget<0)throw Error('Page extraction work limit exceeded.');const c=text[i];
    if(quoted){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"')quoted=false;}
    else if(c==='"')quoted=true;else if(c==='{'||c==='['){if(++depth>64)throw Error('Page data is too deeply nested.');}
    else if(c==='}'||c===']'){if(--depth===0){const s=text.slice(start,i+1);if(parsed.has(s))return;parsed.add(s);let v;try{v=JSON.parse(s);}catch{return;}tree(v);return;}}
   }
  }
  for(const text of documents){
   const trim=text.trim();if(trim.startsWith('{')||trim.startsWith('['))jsonAt(trim,0);
   for(const m of text.matchAll(/"(?:player|page)"\s*:\s*\{/g)){
    // For Auction, the envelope has game alongside page: parse complete transport lines below.
    if(a.source==='2KDB')jsonAt(text,m.index+m[0].lastIndexOf('{'));
   }
  }
  if(a.source==='2KAuction')for(const line of documents.slice(1).join('').split('\n')){const colon=line.indexOf(':');if(colon>=0){const s=line.slice(colon+1).trim();if(s.startsWith('{')||s.startsWith('['))jsonAt(s,0);}}
  // Identical repeated transport objects are one source; conflicting matches refuse.
  const unique=new Map(found.map(x=>[JSON.stringify(x),x]));if(unique.size!==1)throw Error('No unique matching player/game/roster was found in the saved data.');
  return {selection:a,player:[...unique.values()][0]};
 }
const aliases={
 ShotMidrange:'shot_mid',ShotThree:'shot_3pt',ShotFreeThrow:'free_throw',PostFadeaway:'post_fade',BallControl:'ball_handle',PassIQ:'passing_iq',PassAccuracy:'passing_accuracy',PassVision:'passing_vision',ReboundOffense:'offensive_rebound',ReboundDefense:'defensive_rebound',
 Shot:'shoot',ShotAreaUnder:'shot_under_basket',ShotAreaCloseLeft:'shot_close_left',ShotAreaCloseCenter:'shot_close_middle',ShotAreaCloseRight:'shot_close_right',SpotupShotMidrange:'spot_up_shot_mid',OffscreenShotMidrange:'off_screen_shot_mid',SpotupShotThree:'spot_up_shot_3pt',OffscreenShotThree:'off_screen_shot_3pt',ContestedJumperThree:'contested_jumper_3pt',ContestedJumperMidrange:'contested_jumper_mid',StepbackJumperThree:'stepback_jumper_3pt',StepbackJumperMidrange:'stepback_jumper_mid',TransitionPullupThree:'transition_pull_up_3pt',DrivePullupThree:'drive_pull_up_3pt',DrivePullupMidrange:'drive_pull_up_mid',SpotupDrive:'spot_up_drive',OffscreenDrive:'off_screen_drive',TripleThreatNothing:'triple_threat_idle',SetupSizeup:'setup_with_sizeup',SetupHesitation:'setup_with_hesitation',SetupNothing:'no_setup_dribble',DribbleCrossover:'driving_crossover',DribbleSpin:'driving_spin',DribbleStepBack:'driving_step_back',DribbleHalfSpin:'driving_half_spin',DribbleDoubleCross:'driving_double_crossover',DribbleBehindBack:'driving_behind_the_back',DribbleHesitation:'driving_dribble_hesitation',DribbleInAndOut:'driving_in_and_out',DribbleNothing:'no_driving_dribble_move',FinishAttack:'attack_strong_on_drive',FindOpenMan:'dish_to_open_man',Touch:'touches',RollVsFade:'roll_vs_pop',IsoVsElite:'iso_vs_elite_defender',IsoVsGood:'iso_vs_good_defender',IsoVsAverage:'iso_vs_average_defender',IsoVsPoor:'iso_vs_poor_defender',PostShoot:'shoot_from_post',Interception:'pass_interception'
};
const hz={UnderBasket:'under_basket',CloseLeft:'close_left',CloseCenter:'close_middle',CloseRight:'close_right',MedLeft:'mid_left',MedLeftCenter:'mid_left_center',MedCenter:'mid_center',MedRightCenter:'mid_right_center',MedRight:'mid_right',ThreeLeft:'3pt_left',ThreeLeftCenter:'3pt_left_center',ThreeCenter:'3pt_center',ThreeRightCenter:'3pt_right_center',ThreeRight:'3pt_right'};
function map(p,sourceUrl,schema,{nativeNames=false}={}){
 const fields={},info={},rows=[],used=new Set(),warnings=['MyTEAM card values may differ from standard roster ratings. Missing fields remain unchanged.','Animation names are reference-only: cross-version animation IDs are not verified.'];
 const add=(target,key,source,group,label,value,shown=value)=>{target[key]=value;used.add(source);rows.push({key,source,group,label,value:shown,target:target===info?'info':'field'});};
 const entries=Object.keys(p),lookup=(n)=>entries.find(k=>norm(k)===norm(n));
 const parts=p.name.trim().split(/\s+/);
 const first=p.first_name??parts[0],last=p.last_name??parts.slice(1).join(' ');
 for(const [key,label,value] of [['firstName','First name',first],['lastName','Last name',last]]){
  if(typeof value==='string'&&value.length>0&&value.length<=19&&!/[\u0000-\u001F]/.test(value))add(info,key,'name','info',label,value);
  else warnings.push(label+' could not be safely imported; it remains unchanged.');
 }
 if(typeof p.college==='string'&&p.college.trim()&&p.college.trim().length<=47&&!/[\u0000-\u001F]/.test(p.college))add(info,'fromName','college','info','From',p.college.trim());
 for(const f of schema){
  if(!['attributes','tendencies','badges','hotzones','personality'].includes(f.group)||f.key==='PotentialRaw')continue;
  const base=f.key.replace(/AbilityRaw$|TendencyRaw$|Raw$|Badge$/g,'');let source;
  if(f.group==='hotzones')source='hz_'+hz[f.key.replace('HotZone','')];
  else source=lookup((aliases[base]||base)+(f.group==='tendencies'?'_t':''));
  if(nativeNames&&p[source]==null){
   source=lookup(base+(f.group==='tendencies'?'_t':''));
   if(f.group==='attributes'&&base.startsWith('Durability')){
    const m=base.match(/^Durability(Shoulder|Elbow|Hip|Knee|Ankle|Foot)(Left|Right)$/);
    if(m)source=lookup('durability_'+m[2]+'_'+m[1]);
    else if(base==='DurabilityOther')source=lookup('durability_misc');
   }
  }
  if(f.group==='attributes'&&base.startsWith('Durability')&&p.overall_durability!=null)source='overall_durability';
  if(f.group==='attributes'&&p[source]==null){
   const fallback={PostControl:'post_moves',Agility:'lateral_quickness'}[base];
   if(fallback&&p[fallback]!=null)source=fallback;
  }
  const v=p[source];if(v==null||!Number.isInteger(v))continue;
  const max=f.group==='attributes'?99:f.group==='tendencies'?100:f.group==='badges'?5:f.group==='hotzones'?2:(f.max??2**f.bits-1);
  if(v<(f.group==='attributes'?25:0)||v>max)continue;
  const raw=f.group==='attributes'?(v-25)*3:v;
  add(fields,f.key,source,f.group,f.label,raw,f.group==='hotzones'?['Cold','Neutral','Hot'][v]:f.group==='badges'?['None','Bronze','Silver','Gold','HOF','Legend'][v]:v);
 }
 for(const [source,key,label,min,max] of [['height','heightCm','Height (cm)',120,260],['wingspan','wingspanCm','Wingspan (cm)',120,320],['weight','weightLbs','Weight (lb)',70,500]])if(typeof p[source]==='number'&&p[source]>=min&&p[source]<=max)add(info,key,source,'appearance',label,p[source]);
 for(const [source,key,label] of [['position','positionId','Primary position'],['secondary_position','secondaryPositionId','Secondary position']]){const v=['PG','SG','SF','PF','C'].indexOf(p[source]);if(v>=0)add(info,key,source,'info',label,v,p[source]);}
 if(Number.isInteger(p.potential)&&p.potential>=25&&p.potential<=99)add(info,'potential','potential','attributes','Potential',p.potential);
 for(const [source,key,label,options] of [['dominant_hand','BestHand','Dominant Hand',['Left','Right']],['dominant_dunk_hand','BestDunkHand','Dominant Dunk Hand',['Left','Right','Either']]]){const value=options.indexOf(p[source]);if(value>=0)add(fields,key,source,'appearance',label,value,p[source]);}
 const data={format:'2kplayer',version:1,game:'NBA2K27',name:p.name.slice(0,80),fields,info};
 const missing=schema.filter(f=>!Object.hasOwn(fields,f.key)&&f.key!=='PotentialRaw').map(f=>({group:f.group,label:f.label}));
 for(const [key,label] of [['firstName','First name'],['lastName','Last name']])if(!Object.hasOwn(info,key))missing.push({group:'Other player data',label});
 if(!info.fromName)missing.push({group:'Other player data',label:'From (college/city)'});
 for(const label of ['Jersey number','Cyberface ID','Full-name audio ID','Team assignment','Birth date / age','Years pro','Archetype','Season history','Award history'])missing.push({group:'Other player data',label});
 const reference=Object.entries(p).filter(([k,v])=>!used.has(k)&&v!=null).slice(0,100).map(([key,value])=>({key:key.slice(0,200),value:typeof value==='object'?'[Nested source data omitted]':String(value).slice(0,1000)}));
 return {data,rows,missing,reference,warnings,sourceUrl,sourceVersion:String(p.version),name:p.name};
}

 function read(input,inputUrl,schema){
  if(!Array.isArray(schema)||schema.length>1000)throw Error('Verified player field schema unavailable.');
  const snapshot=extract(input,inputUrl),a=snapshot.selection;let p=snapshot.player,nativeNames=false;
  if(a.source==='2KAuction'){
   const s=p.page.selected,out=Object.create(null);if(typeof s.display_name!=='string')throw Error('Missing selected player name.');
   for(const group of ['attributes','tendencies','badges','hot_zones']){const v=s[group];if(v&&typeof v==='object'&&!Array.isArray(v))for(const [k,x] of Object.entries(v))if(typeof x==='number'||x===null)out[k]=x;}
   Object.assign(out,{name:s.display_name,first_name:s.first_name,last_name:s.last_name,version:p.game.slice(2),height:s.height_cm,wingspan:s.wingspan_cm,weight:s.weight_lb,position:s.position,secondary_position:s.secondary_position,college:s.from_college??s.college??s.bio?.from_college});p=out;nativeNames=true;
  }
  if(!p.name.trim()||p.name.length>200)throw Error('Invalid player name.');
  const result=map(p,a.url,schema,{nativeNames});result.source=a.source;result.warnings.unshift('Saved page data: source identity is matched to the URL but authenticity is not verified. Review every staged value.');
  // Do not expose arbitrarily large nested reference payloads to the UI.
  result.reference=result.reference.slice(0,100);return result;
 }
 return {address,extract,read};
});
