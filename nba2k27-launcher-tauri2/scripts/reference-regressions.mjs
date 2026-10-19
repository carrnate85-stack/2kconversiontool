// Build-time WPF parser oracle. No web request or JavaScript backend is shipped.
import fs from 'node:fs';
import path from 'node:path';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const require=createRequire(import.meta.url);
const source=require(path.join(root,'../nba2k27-mod-launcher/src/core/basketball-reference-stats.js'));
const url='https://www.basketball-reference.com/players/m/maxeyty01.html';
const cells={year_id:'2025-26',team_name_abbr:'PHI',comp_name_abbr:'NBA',g:70,gs:70,mp:2661,fg:694,fga:1501,fg3:220,fg3a:599,ft:372,fta:417,orb:24,drb:266,ast:461,stl:130,blk:55,tov:171,pf:151,pts:1980};
const row=(changes={})=>'<tr>'+Object.entries({...cells,...changes}).map(([k,v])=>`<td data-stat="${k}">${v}</td>`).join('')+'</tr>';
const page=(rows=row(),id='totals_stats')=>`<h1><span>Tyrese Maxey</span></h1><!-- <table id="${id}">${rows}</table> --><table id="totals_stats_post">${row({pts:999})}</table>`;
const cases=[];
function check(html,value=url){let expected=null;try{expected=source.parse(html,value)}catch{}cases.push({html,url:value,expected});}
check(page());
check(page(row({mp:'2,661'}),'totals').replace('Tyrese Maxey','Test &amp; Player <script>ignored()</script>'));
check(page(row({team_name_abbr:'2TM'})+row({team_name_abbr:'GSW'})+row()));
check(page(row()+row({team_name_abbr:'GSW'})));
check(page(row({team_name_abbr:'TOT'})+row({team_name_abbr:'2TM'})));
check(page(row({comp_name_abbr:'ABA',year_id:'1970-71'})+row({year_id:'Career'})+row({year_id:'2026-27',g:0})+row()));
check(page(row({orb:'',drb:'',stl:'',blk:'',fg3:'',fg3a:'',tov:''})));
for(const key of Object.keys(cells)) for(const value of ['',null,'-1','0.5','NaN','Infinity','100000']) check(page(row({[key]:value})));
for(const team of ['ATL','BOS','BRK','NJN','CHA','CHO','CHH','CHI','CLE','DAL','DEN','DET','GSW','HOU','IND','LAC','LAL','MEM','VAN','MIA','MIL','MIN','NOP','NOH','NOK','NYK','OKC','ORL','PHI','PHO','PHX','POR','SAC','SAS','TOR','UTA','WAS','WSB','ZZZ','TOT','9TM']) check(page(row({team_name_abbr:team})));
for(const year of ['1898-99','1899-00','1999-00','2025–26','2154-55','2155-56','2025-28']) check(page(row({year_id:year})));
check('<h1>Blocked</h1>');check(page()+page());check(page().replace('data-stat="pts"','data-stat="g"'));
check(page().replace(/"/g,"'"));
check(page().replace('Tyrese Maxey','Name &#65; &#x42; &nbsp; &ndash; &mdash;'));
const urlCases=[];
for(const value of [url,' https://basketball-reference.com/players/m/maxeyty01.html?x=1#totals ','http://www.basketball-reference.com/players/m/maxeyty01.html','https://evil.example/players/m/maxeyty01.html','https://basketball-reference.com.evil.example/players/m/maxeyty01.html','https://user:pass@www.basketball-reference.com/players/m/maxeyty01.html','https://www.basketball-reference.com:444/players/m/maxeyty01.html','https://www.basketball-reference.com/teams/PHI/2026.html','file:///c:/test','https://127.0.0.1/players/m/maxeyty01.html']){let expected=null;try{expected=source.playerUrl(value)}catch{}urlCases.push({value,expected});}
const output=JSON.stringify({cases,urlCases,fixture:source.parse(page(),url),fixtureHtml:page()});
const file=path.join(root,'assets/reference-regressions.json');
if(!fs.existsSync(file)||fs.readFileSync(file,'utf8')!==output)fs.writeFileSync(file,output);
const fixtureFile=path.join(root,'assets/reference-fixture.html');
if(!fs.existsSync(fixtureFile)||fs.readFileSync(fixtureFile,'utf8')!==page())fs.writeFileSync(fixtureFile,page());
console.log(JSON.stringify({referenceParserOracleCases:cases.length,referenceUrlOracleCases:urlCases.length,networkRequests:0}));
