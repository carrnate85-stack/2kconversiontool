(function(root,factory){
  const policy=factory();
  if(typeof module==='object'&&module.exports)module.exports=policy;
  else root.GameRuleValues=policy;
})(typeof globalThis!=='undefined'?globalThis:this,function(){
  'use strict';
  // Exact-build values paired with the stock debug Game Rules menu.
  const numeric={
    OffensiveViolationInKey:['Off','3 seconds','5 seconds'],
    DefensiveViolationInKey:['Off','3 seconds','5 seconds'],
    FoulOut:['Off','1','2','3','4','5','6'],
    ThreePointShotRestriction:['Enabled','Enabled after steal or block','Disabled'],
  };
  const labelFields={
    '3 in the Key':'OffensiveViolationInKey',
    'Def 3 in the Key':'DefensiveViolationInKey',
    'Foul Out':'FoulOut',
    '3Pt Shots':'ThreePointShotRestriction',
  };
  function optionsFor(field,width){
    if(width===1)return [{value:0,label:'Off'},{value:1,label:'On'}];
    if(width!==4||!Object.prototype.hasOwnProperty.call(numeric,field))return [];
    return numeric[field].map((label,value)=>({value,label}));
  }
  return {optionsFor,optionsForLabel:(label,width)=>optionsFor(labelFields[label],width)};
});
