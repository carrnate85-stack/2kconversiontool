(function(root,factory){const api=factory();if(typeof module==='object'&&module.exports)module.exports=api;else root.playerBio=api;})(globalThis,()=>{
 function dateParts(value){
  if(typeof value!=='string'||!/^\d{4}-\d{2}-\d{2}$/.test(value))throw Error('Birthday must be a complete YYYY-MM-DD date.');
  const [year,month,day]=value.split('-').map(Number),leap=year%4===0&&(year%100!==0||year%400===0);
  const days=[31,leap?29:28,31,30,31,30,31,31,30,31,30,31];
  if(year<1||year>4095||month<1||month>12||day<1||day>days[month-1])throw Error('Birthday must be a real calendar date between years 0001 and 4095.');
  return {year,month,day};
 }
 function years(value){if(!Number.isInteger(value)||value<0||value>31)throw Error('Years pro must be a whole number from 0 to 31.');return value;}
 function plan(input,player){
  const result={args:[],writes:0,info:{}};
  if(input.birthDate===undefined&&input.yearsPro===undefined)return result;
  if(player?.bioLayoutVerified!==true)throw Error('Refresh the verified birthday/years-pro mapping before editing these details.');
  if(input.birthDate!==undefined){
   const date=dateParts(input.birthDate);
   for(const [key,max] of [['birthYear',4095],['birthMonth',15],['birthDay',31]])if(!Number.isInteger(player[key])||player[key]<0||player[key]>max)throw Error('The original birthday is unavailable. Refresh before saving.');
   for(const key of ['Year','Month','Day'])if(input['expectedBirth'+key]!==player['birth'+key])throw Error('The original birthday changed since this player was opened. Refresh before saving.');
   const changed=Number(date.year!==player.birthYear)+Number(date.month!==player.birthMonth)+Number(date.day!==player.birthDay);
   if(changed){result.args.push('--birth-date',input.birthDate,'--expected-birth-year',String(player.birthYear),'--expected-birth-month',String(player.birthMonth),'--expected-birth-day',String(player.birthDay));result.writes+=changed;result.info={...result.info,birthDate:input.birthDate,birthYear:date.year,birthMonth:date.month,birthDay:date.day};}
  }
  if(input.yearsPro!==undefined){years(input.yearsPro);years(player.yearsPro);years(input.expectedYearsPro);if(input.expectedYearsPro!==player.yearsPro)throw Error('The original years pro changed since this player was opened. Refresh before saving.');if(input.yearsPro!==player.yearsPro){result.args.push('--years-pro',String(input.yearsPro),'--expected-years-pro',String(player.yearsPro));result.writes++;result.info.yearsPro=input.yearsPro;}}
  return result;
 }
 return {dateParts,years,plan};
});
