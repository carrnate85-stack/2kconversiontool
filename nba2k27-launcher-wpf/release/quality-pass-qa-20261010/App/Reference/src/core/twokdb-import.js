'use strict';
const schema=require('../data/player-field-schema.json');
const dna=require('./player-dna');
const norm=s=>s.toLowerCase().replace(/[^a-z0-9]/g,'');
function url(value){let u;try{u=new URL(value);}catch{throw Error('Paste a 2KDB player link.');}if(u.protocol!=='https:'||u.hostname!=='2kdb.net'||u.port||u.username||u.password||!/^\/player\/\d+$/.test(u.pathname))throw Error('Use https://2kdb.net/player/ followed by the player number.');return u.origin+u.pathname;}
function extract(s,id){
 if(typeof s!=='string'||s.length>4000000)throw Error('Player page is too large.');
 // Parse JSON string literals from Next's inline transport; never execute website code.
 const decoded=[s];for(const m of s.matchAll(/self\.__next_f\.push\(\[1,("(?:\\.|[^"\\])*")\]\)/g)){try{decoded.push(JSON.parse(m[1]));}catch{}}
 for(const text of decoded)for(const m of text.matchAll(/"player"\s*:\s*\{/g)){
  const start=m.index+m[0].lastIndexOf('{');let depth=0,quoted=false,escape=false;
  for(let i=start;i<text.length;i++){const c=text[i];if(quoted){if(escape)escape=false;else if(c==='\\')escape=true;else if(c==='"')quoted=false;}else if(c==='"')quoted=true;else if(c==='{')depth++;else if(c==='}'&&!--depth){try{const p=JSON.parse(text.slice(start,i+1));if(String(p.relationId)===String(id)&&typeof p.name==='string')return p;}catch{}break;}}
 }
 throw Error('No matching player data found. Let the 2KDB page finish loading.');
}
const aliases={
 ShotMidrange:'shot_mid',ShotThree:'shot_3pt',ShotFreeThrow:'free_throw',PostFadeaway:'post_fade',BallControl:'ball_handle',PassIQ:'passing_iq',PassAccuracy:'passing_accuracy',PassVision:'passing_vision',ReboundOffense:'offensive_rebound',ReboundDefense:'defensive_rebound',
 Shot:'shoot',ShotAreaUnder:'shot_under_basket',ShotAreaCloseLeft:'shot_close_left',ShotAreaCloseCenter:'shot_close_middle',ShotAreaCloseRight:'shot_close_right',SpotupShotMidrange:'spot_up_shot_mid',OffscreenShotMidrange:'off_screen_shot_mid',SpotupShotThree:'spot_up_shot_3pt',OffscreenShotThree:'off_screen_shot_3pt',ContestedJumperThree:'contested_jumper_3pt',ContestedJumperMidrange:'contested_jumper_mid',StepbackJumperThree:'stepback_jumper_3pt',StepbackJumperMidrange:'stepback_jumper_mid',TransitionPullupThree:'transition_pull_up_3pt',DrivePullupThree:'drive_pull_up_3pt',DrivePullupMidrange:'drive_pull_up_mid',SpotupDrive:'spot_up_drive',OffscreenDrive:'off_screen_drive',TripleThreatNothing:'triple_threat_idle',SetupSizeup:'setup_with_sizeup',SetupHesitation:'setup_with_hesitation',SetupNothing:'no_setup_dribble',DribbleCrossover:'driving_crossover',DribbleSpin:'driving_spin',DribbleStepBack:'driving_step_back',DribbleHalfSpin:'driving_half_spin',DribbleDoubleCross:'driving_double_crossover',DribbleBehindBack:'driving_behind_the_back',DribbleHesitation:'driving_dribble_hesitation',DribbleInAndOut:'driving_in_and_out',DribbleNothing:'no_driving_dribble_move',FinishAttack:'attack_strong_on_drive',FindOpenMan:'dish_to_open_man',Touch:'touches',RollVsFade:'roll_vs_pop',IsoVsElite:'iso_vs_elite_defender',IsoVsGood:'iso_vs_good_defender',IsoVsAverage:'iso_vs_average_defender',IsoVsPoor:'iso_vs_poor_defender',PostShoot:'shoot_from_post',Interception:'pass_interception'
};
const hz={UnderBasket:'under_basket',CloseLeft:'close_left',CloseCenter:'close_middle',CloseRight:'close_right',MedLeft:'mid_left',MedLeftCenter:'mid_left_center',MedCenter:'mid_center',MedRightCenter:'mid_right_center',MedRight:'mid_right',ThreeLeft:'3pt_left',ThreeLeftCenter:'3pt_left_center',ThreeCenter:'3pt_center',ThreeRightCenter:'3pt_right_center',ThreeRight:'3pt_right'};
function map(p,sourceUrl,{nativeNames=false}={}){
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
 const data=dna.validate({format:'2kplayer',version:1,game:'NBA2K27',schemaId:dna.schemaId,name:p.name.slice(0,80),fields,info});
 const missing=schema.filter(f=>!Object.hasOwn(fields,f.key)&&f.key!=='PotentialRaw').map(f=>({group:f.group,label:f.label}));
 for(const [key,label] of [['firstName','First name'],['lastName','Last name']])if(!Object.hasOwn(info,key))missing.push({group:'Other player data',label});
 if(!info.fromName)missing.push({group:'Other player data',label:'From (college/city)'});
 for(const label of ['Jersey number','Cyberface ID','Full-name audio ID','Team assignment','Birth date / age','Years pro','Archetype','Season history','Award history'])missing.push({group:'Other player data',label});
 const reference=Object.entries(p).filter(([k,v])=>!used.has(k)&&v!=null).map(([key,value])=>({key,value:typeof value==='object'?JSON.stringify(value).slice(0,2000):String(value).slice(0,1000)}));
 return {data,rows,missing,reference,warnings,sourceUrl,sourceVersion:String(p.version),name:p.name};
}
module.exports={url,extract,map};
