'use strict';
const http=require('node:http'),fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const METHODS=Object.freeze({editorPresets:'editor:presets',basketballReferenceStats:'player:reference-stats',warmAnimationPreview:'animation:warm',openAnimationPreview:'animation:open',animationPreviewCatalog:'animation:catalog',animationPreviewRequest:'animation:request',closeAnimationPreview:'animation:close',animationPreviewProgress:'animation:progress-status',nativeRosterExport:'roster:native-export',rosterAutoAttach:'roster:auto-attach',rosterBootstrap:'roster:bootstrap',openLauncher:'roster:launcher',playerResearchStatus:'player-research:status',playerEditorArtwork:'player-research:artwork',playerEditorHeaderLabels:'player-research:header-labels',playerEditorBadgeIcons:'player-research:badge-icons',previewPlayerAudio:'player-research:preview-audio',scanPlayerResearch:'player-research:scan',exportPlayerResearch:'player-research:export',savePlayer:'player:save',playerSeasons:'player:seasons',player23Catalog:'player23:catalog',playerDna:'player-dna:transfer',teamStaff:'team:staff',saveTeamStaff:'team:staff-save',teamDna:'team-dna:transfer',teamRotations:'team:rotations',saveTeam:'team:save',saveTeamUniform:'team-uniform:save'});
class RosterWebServer{
 constructor({root,invoke,readAnimationPayload=async()=>null,closePreviewOwner=async()=>{},previewOwnerTimeout=120000}){this.root=path.resolve(root);this.invoke=invoke;this.readAnimationPayload=readAnimationPayload;this.closePreviewOwner=closePreviewOwner;this.previewOwnerTimeout=previewOwnerTimeout;this.previewOwners=new Map();this.token=crypto.randomBytes(32).toString('hex');this.tail=Promise.resolve();}
 start(){return this.starting ||= this.listen().catch(error=>{this.starting=null;throw error;});}
 async listen(){
  this.server=http.createServer((req,res)=>this.handle(req,res).catch(()=>{if(!res.headersSent)res.writeHead(500);res.end('Local editor request failed.');}));
  this.server.requestTimeout=15000;this.server.headersTimeout=10000;
  await new Promise((resolve,reject)=>{this.server.once('error',reject);this.server.listen(0,'127.0.0.1',resolve);});
  if(this.server.address().port===43110){await new Promise(r=>this.server.close(r));return this.listen();}
  this.origin='http://127.0.0.1:'+this.server.address().port;this.host=new URL(this.origin).host;
  this.url=this.origin+'/renderer/index.html?workspace=roster#'+this.token;return this.url;
 }
 async close(){clearInterval(this.previewSweep);await Promise.all([...this.previewOwners.values()].map(row=>this.closePreviewOwner(row.owner)));this.previewOwners.clear();if(!this.server)return;const s=this.server;this.server=null;this.starting=null;s.closeAllConnections();await new Promise(r=>s.close(r));}
 previewOwner(req,create=false){
  const id=req.headers['x-animation-owner'];if(typeof id!=='string'||! /^[a-f0-9-]{36}$/.test(id))return null;
  let row=this.previewOwners.get(id);
  if(!row&&create){if(this.previewOwners.size>=64)throw Error('Close another editor tab first.');row={owner:{browser:true},seen:Date.now()};this.previewOwners.set(id,row);}
  if(row)row.seen=Date.now();
  if(!this.previewSweep){this.previewSweep=setInterval(()=>{for(const [key,value] of this.previewOwners)if(Date.now()-value.seen>this.previewOwnerTimeout){this.previewOwners.delete(key);void Promise.resolve(this.closePreviewOwner(value.owner)).catch(error=>console.warn('Browser preview cleanup:',error.message));}},Math.min(5000,this.previewOwnerTimeout));this.previewSweep.unref();}
  return row?.owner;
 }
 authenticated(req){const token=req.headers['x-roster-token'];return typeof token==='string'&&/^[a-f0-9]{64}$/.test(token)&&crypto.timingSafeEqual(Buffer.from(token),Buffer.from(this.token));}
 async handle(req,res){
  res.setHeader('Cache-Control','no-store');res.setHeader('X-Content-Type-Options','nosniff');res.setHeader('Referrer-Policy','no-referrer');
  res.setHeader('Content-Security-Policy',"default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; media-src 'self' data: blob:; connect-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'");
  if(req.headers.host!==this.host||req.headers['sec-fetch-site']==='cross-site'){res.writeHead(403);return res.end('Local requests only.');}
  const route=new URL(req.url,this.origin).pathname;
  if(route==='/rpc'){
   if(req.method!=='POST'||req.headers.origin!==this.origin||!this.authenticated(req)||req.headers['content-type']!=='application/json'){res.writeHead(403);return res.end('Open the editor from the launcher.');}
   let length=0;const chunks=[];
   for await(const b of req){length+=b.length;if(length>2*1024*1024){res.writeHead(413);return res.end('Request too large.');}chunks.push(b);}
   let body;try{body=JSON.parse(Buffer.concat(chunks).toString());}catch{res.writeHead(400);return res.end('Invalid JSON.');}
   if(body.method==='ping'){this.previewOwner(req);res.setHeader('Content-Type','application/json');return res.end('{"result":true}');}
   if(!Object.hasOwn(METHODS,body.method)||!Array.isArray(body.args)||body.args.length>1){res.writeHead(404);return res.end('Unsupported editor operation.');}
   const animation=METHODS[body.method].startsWith('animation:');let owner;
   if(animation){owner=this.previewOwner(req,true);if(!owner){res.writeHead(403);return res.end('Open animation previews from this editor tab.');}}
   // Read-only animation preparation must not hold the roster-save RPC queue.
   const job=animation?Promise.resolve().then(()=>this.invoke(METHODS[body.method],body.args,owner)):this.tail.then(()=>this.invoke(METHODS[body.method],body.args));if(!animation)this.tail=job.catch(()=>{});
   res.setHeader('Content-Type','application/json');
   try{res.end(JSON.stringify({result:await job??null}));}catch(error){res.statusCode=400;res.end(JSON.stringify({error:String(error.message||'Editor request failed.')}));}return;
  }
  if(req.method!=='GET'&&req.method!=='HEAD'){res.writeHead(405);return res.end();}
  if(route.startsWith('/animation-payload/')){
   const owner=this.authenticated(req)&&this.previewOwner(req),token=route.slice('/animation-payload/'.length);
   if(!owner||! /^[a-f0-9]{48}$/.test(token)){res.writeHead(403);return res.end();}
   const bytes=await this.readAnimationPayload(token,owner);if(!bytes){res.writeHead(410);return res.end();}
   res.setHeader('Content-Type','application/json');return res.end(req.method==='HEAD'?undefined:bytes);
  }
  let name;try{name=decodeURIComponent(route);}catch{res.writeHead(400);return res.end();}
  if(name.split('/').some(part=>part==='..'||part==='.')||!/^\/(?:renderer\/[\w.-]+\.(?:html|css|js)|renderer\/animation-preview\/(?:vendor\/)?[\w.-]+\.js|shared\/[\w.-]+\.js|assets\/app-icon\.png)$/.test(name)){res.writeHead(404);return res.end();}
  let bytes;try{bytes=await fs.readFile(path.join(this.root,name.slice(1)));}catch{res.writeHead(404);return res.end();}
  if(name==='/renderer/index.html')bytes=Buffer.from(bytes.toString().replace('<script src="theme.js">','<script src="roster-browser-bridge.js"></script><script src="theme.js">'));
  const ext=path.extname(name),types={'.html':'text/html; charset=utf-8','.css':'text/css; charset=utf-8','.js':'text/javascript; charset=utf-8','.png':'image/png'};
  res.setHeader('Content-Type',types[ext]);res.end(req.method==='HEAD'?undefined:bytes);
 }
}
module.exports={RosterWebServer,METHODS};
