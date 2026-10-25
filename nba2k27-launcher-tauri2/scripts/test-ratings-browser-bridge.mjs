import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

test('onlineRatings uses the real browser RPC despite beginning with on',async()=>{
 const calls=[],window={addEventListener(){}},token='a'.repeat(64);
 vm.runInNewContext(fs.readFileSync(new URL('../browser-overrides/roster-browser-bridge.js',import.meta.url),'utf8'),{
  window,document:{documentElement:{classList:{add(){}}}},crypto:{randomUUID:()=> 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'},location:{hash:'#'+token,pathname:'/',search:''},sessionStorage:{setItem(){}},history:{replaceState(){}},
  fetch:async(url,options)=>{const body=JSON.parse(options.body);calls.push(body);return {ok:true,json:async()=>({result:body.args[0].action==='start'?{id:'native',url:body.args[0].url,isolation:'native-webview2-no-launcher-bridge'}:{closed:true}})};}
 });
 const url='https://2kauction.com/mynba/2k17/players/tayshaun-prince';
 const receipt=await window.modLauncher.onlineRatings({action:'start',url,destination:{recordIndex:1}});
 assert.equal(receipt.url,url);assert.equal(receipt.id,'native');assert.equal(receipt.isolation,'native-webview2-no-launcher-bridge');
 await window.modLauncher.onlineRatings({action:'cancel',id:receipt.id});
 assert.deepEqual(calls.map(c=>c.method),['onlineRatings','onlineRatings']);
 assert.equal(typeof window.modLauncher.onPlayerChanged(()=>{}),'function');assert.equal(calls.length,2);
});
