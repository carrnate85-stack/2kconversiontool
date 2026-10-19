'use strict';
const codes={0:'phi',1:'mil',2:'was',3:'chi',4:'cle',5:'bos',6:'lac',8:'mem',9:'atl',10:'mia',11:'nop',12:'uta',13:'sac',14:'nyk',15:'lal',16:'orl',17:'dal',18:'bkn',19:'den',20:'ind',21:'det',22:'tor',23:'hou',24:'okc',25:'sas',26:'phx',27:'min',28:'por',29:'gsw',31:'cha'};
function matchesTeam(asset,team){
 if(!Number.isInteger(team.teamId)||team.teamId<0)return false;
 const s=asset.replace(/^female_/,'').toLowerCase();
 const tags=[...s.matchAll(/(?:^|_)team_(?:[a-z][a-z0-9]*_)*?(\d{3})(?:_|$)/g)];
 if(tags.length)return tags.some(m=>Number(m[1])===team.teamId);
 const words=s.split('_');
 const name=(team.teamName||'').toLowerCase().replace(/[^a-z0-9]/g,'');
 return Boolean((codes[team.teamId]&&words.includes(codes[team.teamId]))||(name.length>3&&words.includes(name)));
}
const cities={26:['phoenix'],0:['philadelphia'],1:['milwaukee'],2:['washington'],3:['chicago'],4:['cleveland'],5:['boston'],8:['memphis'],9:['atlanta'],10:['miami'],12:['utah'],13:['sacramento'],16:['orlando'],17:['dallas'],18:['brooklyn'],19:['denver'],20:['indiana'],21:['detroit'],22:['toronto'],23:['houston'],27:['minnesota'],28:['portland'],31:['charlotte']};
const baseMatch=matchesTeam;
function match(asset,team){return baseMatch(asset,team)||(Number.isInteger(team.teamId)&&(cities[team.teamId]||[]).some(s=>asset.split('_').includes(s)));}
const teamNames='76ers bucks wizards bulls cavaliers celtics clippers grizzlies hawks heat pelicans jazz kings knicks lakers magic mavericks nets nuggets pacers pistons raptors rockets thunder spurs suns timberwolves blazers warriors hornets aces dream fever liberty lynx mercury mystics sky sparks storm sun wings valkyries tempo fire'.split(' ');
function isTeamClothing(asset){const words=asset.split('_');return /(?:^|_)(team_\d+|nba|wnba)(?:_|$)/.test(asset)||[...Object.values(codes),...teamNames,...Object.values(cities).flat()].some(s=>words.includes(s));}
module.exports={matchesTeam:match,isTeamClothing};
