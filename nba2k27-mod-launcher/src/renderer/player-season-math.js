(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.playerSeasonMath=api;})(typeof window==='object'?window:globalThis,()=>{
 const averages={ppg:'points',mpg:'minutes',orpg:'oreb',apg:'assists',spg:'steals',bpg:'blocks',topg:'turnovers',fpg:'fouls',plusMinusAverage:'plusMinus'};
 const percentages={fgPercent:['fgm','fga'],threePercent:['threeMade','threeAttempted'],ftPercent:['ftm','fta']};
 const limits={ftm:2047,fta:2047,fgm:8191,fga:16383,threeMade:4095,threeAttempted:4095,oreb:1023,dreb:4095,steals:2047,blocks:2047,assists:4095,turnovers:2047,points:16383,gp:127,gs:127,minutes:8191,fouls:1023,doubleDoubles:127,tripleDoubles:127,plusMinus:16383};
 const clamp=(v,lo,hi)=>Math.max(lo,Math.min(hi,v));
 const scoring=t=>2*t.fgm+t.threeMade+t.ftm;
 function display(t){const result={...t,rebounds:t.oreb+t.dreb};for(const [key,total]of Object.entries(averages))result[key]=t.gp&&Number.isFinite(t[total])?t[total]/t.gp:null;result.rpg=t.gp?result.rebounds/t.gp:null;for(const [key,[made,attempts]]of Object.entries(percentages))result[key]=t[attempts]?100*t[made]/t[attempts]:null;return result;}
 // Find the nearest attainable scoring total, keeping every shot-attempt count.
 // Among equally close results, prefer makes near the existing scoring mix.
 function scoreTo(t,target){
  const twoAttempts=t.fga-t.threeAttempted,threeAttempts=t.threeAttempted,freeAttempts=t.fta;
  if(twoAttempts<0)throw Error('Three-point attempts cannot exceed field-goal attempts.');
  const maximum=2*twoAttempts+3*threeAttempts+freeAttempts;
  target=clamp(Math.round(target),0,Math.min(maximum,16383));
  const old=scoring(t),factor=old?target/old:maximum?target/maximum:0;
  const idealTwo=(old?t.fgm-t.threeMade:twoAttempts)*factor,idealThree=(old?t.threeMade:threeAttempts)*factor,idealFree=(old?t.ftm:freeAttempts)*factor;
  let best=null;
  for(let three=0;three<=threeAttempts;three++){
   const residual=target-3*three;
   const twoMax=Math.min(twoAttempts,8191-three),low=clamp(Math.ceil((residual-freeAttempts)/2),0,twoMax),high=clamp(Math.floor(residual/2),0,twoMax);
   const optimum=(idealTwo+2*(residual-idealFree))/5;
   for(const two of new Set([low,high,clamp(Math.floor(optimum),Math.min(low,high),Math.max(low,high)),clamp(Math.ceil(optimum),Math.min(low,high),Math.max(low,high))])){
    const free=clamp(Math.round(residual-2*two),0,freeAttempts),points=2*two+3*three+free,error=Math.abs(points-target),cost=(two-idealTwo)**2+(three-idealThree)**2+(free-idealFree)**2;
    if(!best||error<best.error||error===best.error&&cost<best.cost)best={two,three,free,points,error,cost};
   }
  }
  return {...t,fgm:best.two+best.three,threeMade:best.three,ftm:best.free,points:best.points};
 }
 function update(t,key,value){
  if(typeof value!=='number'||!Number.isFinite(value))throw Error('Enter a valid number.');
  if(value<0&&!['plusMinus','plusMinusAverage'].includes(key))throw Error('This value cannot be negative.');
  let next={...t};
  if(Object.hasOwn(averages,key)){
   if(!t.gp)throw Error('Set games played before editing averages.');
   if(key==='ppg')return scoreTo(t,value*t.gp);
   next[averages[key]]=Math.round(value*t.gp);return next;
  }
  if(key==='rpg'||key==='rebounds'){
   if(key==='rpg'&&!t.gp)throw Error('Set games played before editing averages.');
   const target=Math.round(key==='rpg'?value*t.gp:value),before=t.oreb+t.dreb;
   next.oreb=before?Math.round(target*t.oreb/before):0;next.dreb=target-next.oreb;return next;
  }
  if(Object.hasOwn(percentages,key)){
   if(value>100)throw Error('Shooting percentages must be 0–100.');
   const [made,attempted]=percentages[key];if(!t[attempted])throw Error('Set shot attempts before editing this percentage.');
   return change(t,made,Math.min(limits[made],Math.round(value*t[attempted]/100)));
  }
  if(!Object.hasOwn(t,key))throw Error('Unsupported season field.');
  if(!Number.isInteger(value))throw Error('Totals must be whole numbers.');
  next[key]=value;
  if(key==='fgm'){
   if(value>t.fga)throw Error('Made shots cannot exceed attempts.');
   next.threeMade=clamp(t.threeMade,Math.max(0,value-(t.fga-t.threeAttempted)),Math.min(value,t.threeAttempted));
  }else if(key==='threeMade'){
   if(value>t.threeAttempted)throw Error('Made shots cannot exceed attempts.');
   next.fgm=t.fgm-t.threeMade+value;
  }else if(key==='ftm'&&value>t.fta)throw Error('Made shots cannot exceed attempts.');
  if(['fga','threeAttempted','fta'].includes(key)){
   if(next.threeAttempted>next.fga)throw Error('Three-point attempts cannot exceed field-goal attempts.');
   const two=Math.min(t.fgm-t.threeMade,next.fga-next.threeAttempted);next.threeMade=Math.min(t.threeMade,next.threeAttempted);next.fgm=two+next.threeMade;next.ftm=Math.min(t.ftm,next.fta);
  }
  if(['fgm','threeMade','ftm','fga','threeAttempted','fta'].includes(key))next.points=scoring(next);
  return next;
 }
 function change(t,key,value){
  function validate(values){for(const [field,max]of Object.entries(limits))if(Object.hasOwn(values,field)&&(!Number.isInteger(values[field])||values[field]<(field==='plusMinus'?-16384:0)||values[field]>max))throw Error('Outside the stored range: '+field);}
  validate(t);const next=update(t,key,value);validate(next);return next;
 }
 return {averages,percentages,display,change,scoreTo,limits};
});
