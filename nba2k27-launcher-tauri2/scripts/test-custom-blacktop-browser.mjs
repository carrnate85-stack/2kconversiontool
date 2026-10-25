// Isolated headless UI fixture. Does not open a launcher or access real mods.
import { createServer } from 'vite';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { mkdir } from 'node:fs/promises';
import assert from 'node:assert/strict';
const require = createRequire(resolve(process.env.USERPROFILE, '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/package.json'));
const { chromium } = require('playwright');
const html = `<!doctype html><html class="wpf-ui"><head><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><div id="root"></div><script type="module">
import React from 'react'; import {createRoot} from 'react-dom/client';
import {BlacktopCatalog} from '/BlacktopCatalog.tsx';
import '/style.css'; import '/ux-polish.css'; import '/wpf/launcher-styles.ts';
window.requests=[];let entries=[];
window.__TAURI_INTERNALS__={invoke:async(command,{channel,args})=>{
 window.requests.push({channel,args});
 if(channel==='native-session-status')return {phase:'idle'};
 if(channel==='arena-favorites')return [];
 if(channel==='choices')return {items:entries,total:entries.length,groups:['NBA','Custom']};
 if(channel==='custom-blacktop'&&args.action==='sources')return {modsRoot:'C:/Fixture/Mods',levelsRoot:'C:/Fixture/Mods/levels',files:['levels/Arena pack/arena_example_int.iff','levels/Arena pack/arena_example_int_floor.iff'],bases:[{id:'blacktop-stock',name:'Stock Blacktop court',file:'arena_700_int',floorFile:'arena_700_int_floor'}]};
 if(channel==='custom-blacktop'&&args.action==='get')return entries.find(e=>e.id===args.id);
 if(channel==='custom-blacktop'&&['add','edit'].includes(args.action)){const row={...args,id:'custom-'+ (args.action==='edit'?'b':'a').repeat(48),previousId:args.id,name:args.name,group:'Custom'};entries=[row];return row;}
 throw Error('Unexpected fixture API '+channel);
}};
const root=createRoot(document.getElementById('root'));
root.render(React.createElement(BlacktopCatalog,{kind:'arena',customArenas:true,settings:{gameRoot:'C:/Fixture/Game',modsRoot:'C:/Fixture/Mods',arena:'Default · Day / Night'},change:p=>{window.selection=p},onClose:()=>{window.closedCatalog=true}}));
</script></body></html>`;
const server = await createServer({server:{host:'127.0.0.1',port:0},plugins:[{name:'custom-catalog-fixture',configureServer(s){
 s.middlewares.use(async(req,res,next)=>{if(req.url!=='/__custom-blacktop-qa')return next();res.setHeader('Content-Type','text/html');res.end(await s.transformIndexHtml('/__custom-blacktop-qa',html));});
}}]});
let browser;
try {
 await server.listen();
 browser=await chromium.launch({headless:true,executablePath:'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'});
 const page=await browser.newPage({viewport:{width:1100,height:820}}), errors=[];
 page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/__custom-blacktop-qa`);
 await page.getByRole('button',{name:'Custom',exact:true}).click();
 await page.getByRole('button',{name:'Add from mods'}).click();
 await page.getByLabel('Name',{exact:true}).fill('My custom arena');
 await page.getByLabel('Arena mod',{exact:true}).selectOption('levels/Arena pack/arena_example_int.iff');
 assert.equal(await page.getByLabel('Court / floor mod').inputValue(),'levels/Arena pack/arena_example_int_floor.iff');
 await mkdir('artifacts',{recursive:true});
 for(const width of [1100,600]){
  await page.setViewportSize({width,height:820});
  const overflow=await page.locator('.custom-blacktop-import').evaluate(el=>el.scrollWidth>el.clientWidth+1);
  assert.equal(overflow,false,`Import form overflows at ${width}px`);
  assert.equal(await page.locator('.custom-blacktop-import select').evaluateAll(elements => elements.every(el => {
   const s=getComputedStyle(el), line=s.lineHeight==='normal'?parseFloat(s.fontSize)*1.2:parseFloat(s.lineHeight);return parseFloat(s.paddingTop)+parseFloat(s.paddingBottom)+line<=el.clientHeight;
  })),true,`Selected file text is clipped at ${width}px`);
  await page.screenshot({path:`artifacts/custom-blacktop-import-${width}.png`});
 }
 await page.getByRole('button',{name:'Add to Custom'}).click();
 await page.getByRole('button',{name:'My custom arena Custom',exact:true}).waitFor();
 const add=await page.evaluate(()=>window.requests.find(r=>r.channel==='custom-blacktop'&&r.args.action==='add').args);
 assert.deepEqual(add,{action:'add',name:'My custom arena',baseId:'blacktop-stock',arenaSource:'levels/Arena pack/arena_example_int.iff',floorSource:'levels/Arena pack/arena_example_int_floor.iff'});
 await page.getByRole('button',{name:'My custom arena Custom',exact:true}).click({button:'right'});
 assert.equal(await page.evaluate(()=>window.selection),undefined,'Right click must not select an arena');
 await page.getByRole('menuitem',{name:'Edit custom arena'}).click();
 assert.equal(await page.getByLabel('Name',{exact:true}).inputValue(),'My custom arena');
 assert.equal(await page.getByLabel('Arena mod',{exact:true}).inputValue(),'levels/Arena pack/arena_example_int.iff');
 await page.getByLabel('Name',{exact:true}).fill('Edited arena');
 await page.getByRole('button',{name:'Cancel',exact:true}).click();
 assert.equal(await page.evaluate(()=>window.requests.filter(r=>r.args?.action==='edit').length),0,'Cancel must not save');
 await page.getByRole('button',{name:'My custom arena Custom',exact:true}).click({button:'right'});
 await page.getByRole('menuitem',{name:'Edit custom arena'}).click();
 await page.getByLabel('Name',{exact:true}).fill('Edited arena');
 await page.getByLabel('Arena mod',{exact:true}).selectOption('');
 await page.getByRole('button',{name:'Save changes',exact:true}).click();
 await page.getByRole('button',{name:'Edited arena Custom',exact:true}).waitFor();
 const edit=await page.evaluate(()=>window.requests.find(r=>r.args?.action==='edit').args);
 assert.equal(edit.id,'custom-'+ 'a'.repeat(48)); assert.equal(edit.arenaSource,null);
 await page.getByRole('button',{name:'Edited arena Custom',exact:true}).click();
 assert.deepEqual(await page.evaluate(()=>window.selection),{arena:'Edited arena',arenaId:'custom-'+ 'b'.repeat(48)});
 assert.equal(await page.evaluate(()=>window.closedCatalog),true);
 assert.deepEqual(errors,[]);
 console.log('PASS: Custom import, right-click edit, prefilled files, cancel, save, selection, and 1100/600px layouts; isolated headless fixture only');
} finally { await browser?.close(); await server.close(); }
