(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.twokdbApply=api;})(globalThis,()=>{
 const groups=new Set(['attributes','tendencies','badges','hotzones']);
 function prepare(player,data,schema,origins=[]){
  if(player?.source!=='live-roster'||player.ratingsRaw?.length!==schema.length)throw Error('Open a live roster player before importing.');
  const fields={},ratingEdits=[];
  for(const [key,value] of Object.entries(data?.fields||{})){
   const index=schema.findIndex(f=>f.key===key),field=schema[index];
   if(!field||!groups.has(field.group)||key==='PotentialRaw')continue;
   const max=field.group==='attributes'?222:field.group==='tendencies'?100:field.group==='hotzones'?2:Math.min(5,2**field.bits-1);
   if(!Number.isInteger(value)||value<0||value>max)throw Error('Invalid imported value for '+field.label);
   fields[key]=value;
   if(value!==player.ratingsRaw[index])ratingEdits.push({index,expected:player.ratingsRaw[index],value});
  }
  const payload={recordIndex:player.recordIndex,expectedFirstName:player.firstName,expectedLastName:player.lastName,expectedCyberfaceId:player.cyberfaceId,ratingEdits};
  for(const key of ['firstName','lastName','cyberfaceId','positionId','secondaryPositionId','jerseyNumberRaw','weightLbs','heightCm','wingspanCm','potential'])payload[key]=player[key];
  if(data?.info?.potential!==undefined){const v=data.info.potential;if(!Number.isInteger(v)||v<25||v>99)throw Error('Invalid imported potential.');payload.potential=v;}
  const importedInfo={};
  for(const key of ['firstName','lastName','heightCm','wingspanCm','weightLbs','positionId','secondaryPositionId','potential']){
   const v=data?.info?.[key];if(v===undefined)continue;
   const valid=key.endsWith('Name')?typeof v==='string'&&v.length>0&&v.length<=19&&!/[\u0000-\u001F]/.test(v):key==='weightLbs'?Number.isFinite(v)&&v>=70&&v<=500:['heightCm','wingspanCm'].includes(key)?Number.isFinite(v)&&v>=120&&v<=(key==='heightCm'?260:320):key==='potential'?Number.isInteger(v)&&v>=25&&v<=99:Number.isInteger(v)&&v>=0&&v<=5;
   if(!valid)throw Error('Invalid imported '+key+'.');
   payload[key]=v;importedInfo[key]=v;
  }
  const warnings=[];
  const bioInfo={};
  for(const key of ['birthDate','yearsPro'])if(data?.info?.[key]!==undefined)bioInfo[key]=data.info[key];
  if(Object.keys(bioInfo).length){
   const bio=typeof module==='object'&&module.exports?require('./player-bio'):globalThis.playerBio;
   const expected={...(bioInfo.birthDate!==undefined?{expectedBirthYear:player.birthYear,expectedBirthMonth:player.birthMonth,expectedBirthDay:player.birthDay}:{}),...(bioInfo.yearsPro!==undefined?{expectedYearsPro:player.yearsPro}:{})};
   bio.plan({...bioInfo,...expected},player);Object.assign(payload,bioInfo,expected);Object.assign(importedInfo,bioInfo);
  }
  if(data?.info?.fromName!==undefined){
   const name=data.info.fromName;if(typeof name!=='string'||!name.trim()||name.length>47||/[\u0000-\u001F]/.test(name))throw Error('Invalid imported From.');
   const matches=origins.filter(o=>typeof o.name==='string'&&o.name.trim().toLowerCase()===name.trim().toLowerCase()&&Number.isInteger(o.index)&&o.index>=0&&o.index<=4095);
   if(matches.length===1&&Number.isInteger(player.fromIndex)&&player.fromIndex>=-1){payload.fromIndex=matches[0].index;importedInfo.fromName=matches[0].name;}
   else warnings.push('From "'+name+'" was left unchanged: '+(matches.length>1?'multiple matching entries.':'no verified unique match in the loaded roster.'));
  }
  if(!Object.keys(fields).length&&!Object.keys(importedInfo).length)throw Error(warnings.join(' ')||'No supported ratings were found on that page.');
  return {payload,fields,importedInfo,warnings,count:Object.keys(fields).length+Object.keys(importedInfo).length};
 }
 return {prepare};
});
