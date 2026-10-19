(function(root){
 const groups=['NBA','WNBA','Historic','Gyms','Outdoor','Other'];
 function group(c){
  const id=c.id||'',n=Number(/^arena_(\d+)_int$/.exec(id)?.[1]);
  if(id==='default')return 'Outdoor';
  if((n>=300&&n<=317)||id.startsWith('arena_wnba')||id==='arena_mycourtw_int')return 'WNBA';
  if((n>=551&&n<=649)||n===800||n===924||id.startsWith('arena_era'))return 'Historic';
  if((n>=0&&n<=31)||id==='arena_clippers2023_int')return 'NBA';
  if(id.startsWith('park_')||id.endsWith('_ext'))return 'Outdoor';
  if(n===729||n===906||/gym|facility|mycourt|scrimmage|hsmycareer|rechighschool|privatehome|animationlab/.test(id))return 'Gyms';
  return 'Other';
 }
 const normalize=s=>String(s||'').normalize('NFKD').replace(/[\u0300-\u036f]/g,'').toLowerCase().replace(/[^a-z0-9]+/g,' ').trim();
 function filter(choices,{query='',category='All',favorites=[]}={}){
  const starred=new Set(favorites),terms=normalize(query).split(' ').filter(Boolean);
  return choices.filter(c=>(category==='All'||(category==='Favorites'?starred.has(c.id):group(c)===category))&&terms.every(t=>normalize([c.label,c.id,c.file,c.floorFile,group(c)].join(' ')).includes(t)))
   .sort((a,b)=>a.id==='default'?-1:b.id==='default'?1:a.label.localeCompare(b.label,undefined,{numeric:true}));
 }
 const api={groups,group,filter};if(typeof module==='object'&&module.exports)module.exports=api;else root.ArenaBrowser=api;
})(typeof window==='object'?window:globalThis);
