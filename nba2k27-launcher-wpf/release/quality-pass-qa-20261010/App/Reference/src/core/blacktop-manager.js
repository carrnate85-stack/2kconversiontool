'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const run = promisify(execFile);
const { composeLoadedPair, composeStreetwear,layerChoices,layerItems,randomStreetwear,accessoryChoices,normalizeAccessories } = require('./clothing-components');
let loadedStreetCatalog;
const streetCatalog=new Proxy(Object.create(null),{
  get(_target,key){return (loadedStreetCatalog??=require('../data/streetwear-components.json'))[key];},
  has(_target,key){return key in (loadedStreetCatalog??=require('../data/streetwear-components.json'));},
  ownKeys(){return Reflect.ownKeys(loadedStreetCatalog??=require('../data/streetwear-components.json'));},
  getOwnPropertyDescriptor(_target,key){const value=loadedStreetCatalog??=require('../data/streetwear-components.json');return Object.getOwnPropertyDescriptor(value,key);},
});
let loadedShoeLabels;
const getShoeLabels=()=>loadedShoeLabels??=require('./shoe-labels');
const {BlacktopMatchChoices}=require('./blacktop-match-choices');
const {matchesTeam,isTeamClothing}=require('./clothing-team');
let streetOnlyCatalog;
const teamCatalogs=new Map();
function randomTeamClothes(player,assigned,random=Math.random){
 const pool=playerClothingPool(player),key=JSON.stringify([pool,player.teamId,player.teamName]);
 if(!teamCatalogs.has(key)){
  const presets=streetCatalog.presets.filter(p=>p.sourceGroup.pool===pool&&p.separable&&!p.manualOnly&&matchesTeam(p.top,player)&&p.original.every(a=>!isTeamClothing(a)||matchesTeam(a,player)));
  teamCatalogs.set(key,presets);if(teamCatalogs.size>128)teamCatalogs.delete(teamCatalogs.keys().next().value);
 }
 const choices=teamCatalogs.get(key)||[];
 if(!choices.length){if(!assigned.length)throw Error('No compatible team clothing is available for this player.');return assigned[Math.floor(random()*assigned.length)];}
 const preset=choices[Math.floor(random()*choices.length)];
 const bottoms=[...new Set(choices.map(p=>p.bottom))];
 return composeStreetwear(streetCatalog,preset.id,bottoms[Math.floor(random()*bottoms.length)],preset.shoes?.length===1?'roster':undefined);
}
function playerClothingPool(player) {
  const names = player.outfits.filter(o => o.id !== 'custom').flatMap(o => o.items)
    .filter(n => /^(female_)?(tshirt_|hoodie_|jacket_|jogger_|jeans_|warmup_|pants_|shorts_|basketball_shorts_|canvas_shorts_|sweats_|cargo_|tanktop_|shirt_)/.test(n));
  if (!names.length) return null;
  if (names.every(n => n.startsWith('female_'))) return 'female';
  if (names.every(n => !n.startsWith('female_'))) return 'male';
  return null;
}
const streetLabel = name => name.replace(/^female_/, '').replace(/^tshirt_ss_/, 'T-shirt ').replace(/^tshirt_ls_/, 'Long-sleeve shirt ').replaceAll('_', ' ');
let catalogView;
let knownAccessoryAssets;
function accessoryAssetSet() {
  return knownAccessoryAssets ??= new Set(accessoryChoices(streetCatalog).map(choice => choice.asset));
}
function staticCatalogView() {
  return catalogView ||= { accessories:accessoryChoices(streetCatalog), shoes:streetCatalog.additionalShoes||[], shoeLabels:getShoeLabels(),
    streetwear:streetCatalog.presets.filter(p=>['male','female'].includes(p.sourceGroup.pool)&&!p.id.startsWith('accessory-test-')).sort((a,b)=>Number(b.id==='shirtless-male')-Number(a.id==='shirtless-male')).map(p=>({
      id:p.id,pool:p.sourceGroup.pool,top:p.top,previewAsset:p.previewAsset||p.top,experimental:Boolean(p.experimental),manualOnly:Boolean(p.manualOnly),
      topLabel:p.label||streetLabel(p.top),bottom:p.bottom,bottomLabel:streetLabel(p.bottom),extras:p.original.length-2,
      separable:p.separable,shoes:p.shoes||[],socks:(p.accessories||[]).filter(s=>/^(female_)?acc_sock_/.test(s)),
      layers:p.layers||[],layerChoices:layerChoices(streetCatalog,p)
    })) };
}

// Native helpers cannot read Electron's virtual app.asar filesystem.
function resolveBlacktopLayoutPath({ isPackaged, resourcesPath, sourceDir }) {
  return isPackaged
    ? path.join(resourcesPath, 'roster-research', 'blacktop-preset-layouts.json')
    : path.join(sourceDir, 'data', 'blacktop-preset-layouts.json');
}

function outfitLabel(items) {
  const top = items[0] || '';
  const upper = top.includes('hoodie') ? 'Hoodie' : top.includes('jacket') ? 'Warm-up jacket'
    : top.includes('tshirt_ls') ? 'Long-sleeved shirt' : top.includes('tshirt_ss') ? 'T-shirt' : top.replaceAll('_', ' ');
  const bottom = items[1] === 'UNIFORM_SHORTS' ? 'uniform shorts'
    : /warmup/.test(items[1]) ? 'warm-up pants' : (items[1] || '').replaceAll('_', ' ');
  return `${upper} + ${bottom}`;
}
function requiresUniform(option) { return option.items.some(item => item.startsWith('UNIFORM')); }
function clothingLabel(item) {
  if(getShoeLabels()[item])return getShoeLabels()[item];
  if (item === 'UNIFORM_SHORTS') return 'Uniform shorts';
  if (item.includes('hoodie')) return 'Hoodie';
  if (item.includes('jacket')) return 'Warm-up jacket';
  if (item.includes('tshirt_ls')) return 'Long-sleeved shirt';
  if (item.includes('tshirt_ss')) return 'T-shirt';
  if (item.startsWith('warmup_nba')) return 'Warm-up pants (team home)';
  if (item.startsWith('warmup')) return 'Warm-up pants';
  return item.replace(/^acc_sock_/,'').replace(/^acc_/,'').replaceAll('_', ' ');
}
function resolveOutfit(player, value) {
  value=normalizeAccessories(streetCatalog,value);
  if(value.randomFit===true){
    const mode=value.randomMode||'streetwear';
    if(!['streetwear','team','all','accessories'].includes(mode))return undefined;
    if(mode==='accessories')return randomAccessories(player);
    const team=player.outfits.filter(o=>o.id!=='custom'&&o.items?.length>=2&&!uniformUnavailable(player,o));
    if(mode==='team')return randomTeamClothes(player,team);
    const accessories=value.accessories??(value.accessory!==undefined?(value.accessory?[value.accessory]:[]):player.outfits.find(o=>o.pointer===player.pointer)?.items.filter(a=>accessoryAssetSet().has(a))||[]);
    streetOnlyCatalog||={...streetCatalog,presets:streetCatalog.presets.filter(p=>!p.original.some(isTeamClothing))};
    return randomStreetwear(mode==='all'?streetCatalog:streetOnlyCatalog,playerClothingPool(player),Math.random,accessories,true);
  }
  if (Object.hasOwn(value, 'streetId') && value.streetId != null) {
    const preset = streetCatalog.presets.find(p => p.id === value.streetId);
    if (!preset || !['male','female'].includes(preset.sourceGroup.pool) || preset.sourceGroup.pool !== playerClothingPool(player)) return undefined;
    return composeStreetwear(streetCatalog, value.streetId, value.bottom, value.shoe,value.sock,value.layer,value.accessory,value.accessories);
  }
  if (Object.hasOwn(value, 'top') || Object.hasOwn(value, 'bottom')) {
    if (typeof value.top !== 'string' || typeof value.bottom !== 'string') return undefined;
    return composeLoadedPair(player.outfits, value.top, value.bottom);
  }
  return player.outfits.find(o => o.id === value.outfitId);
}
function randomAccessories(player,random=Math.random){
  const slots=require('../shared/accessory-slots');
  const current=player.outfits.find(o=>o.pointer===player.pointer);
  const preset=Number.isInteger(player.streetIndex)?streetCatalog.presets[player.streetIndex]:null;
  const pool=playerClothingPool(player);
  if(!current||!preset||preset.sourceGroup.pool!==pool)throw Error('Choose a streetwear outfit first, then add random accessories.');
  const all=accessoryChoices(streetCatalog),known=new Set(all.map(a=>a.asset));
  const existing=current.items.filter(a=>known.has(a));
  const occupied=new Set();
  const available=all.filter(a=>(a.asset.startsWith('female_')?'female':'male')===pool&&!occupied.has(slots.slot(a.asset))&&slots.slot(a.asset));
  const room=Math.min(3,7-(current.items.length-existing.length),new Set(available.map(a=>slots.slot(a.asset))).size);
  if(room<1)throw Error('No compatible accessory slots are available for this outfit.');
  const pick=n=>Math.floor(random()*n),amount=1+pick(room),chosen=[];
  for(let i=0;i<amount;i++){
    const choices=available.filter(a=>!occupied.has(slots.slot(a.asset)));
    const item=choices[pick(choices.length)].asset;chosen.push(item);occupied.add(slots.slot(item));
  }
  // Recover every selected piece from the current native list, not stale UI inputs.
  const template=layerItems(streetCatalog,preset,player.layer).filter(a=>!player.rosterShoes||!preset.shoes.includes(a));
  const sock=current.items.find(a=>/^(female_)?acc_sock_/.test(a));
  const shoe=player.rosterShoes?'roster':preset.shoes.length===1?current.items[template.indexOf(preset.shoes[0])]:undefined;
  return composeStreetwear(streetCatalog,preset.id,current.items[template.indexOf(preset.bottom)],shoe,sock,player.layer,undefined,chosen);
}
function uniformUnavailable(player, option) {
  if (!requiresUniform(option) || player.uniformVerified) return '';
  if (player.uniform !== 0) return 'The current uniform could not be verified. Choose pants for now.';
  return 'This player\'s uniform is not loaded. Choose pants for now; independent uniform loading is not available yet.';
}
function uniformTarget(player, option, original, restore) {
  if (!Number.isSafeInteger(player.uniform) || player.uniform < 0) throw new Error('Uniform context is unavailable; reopen the updated launcher.');
  if (restore && original && Object.hasOwn(original, 'uniform')) {
    if (original.uniform !== player.uniform && player.uniform !== original.managedUniform) throw new Error('Uniform changed outside this operation; refresh and keep the current outfit.');
    if (original.uniform === 0 && requiresUniform(option)) throw new Error('The original outfit needs uniform data that was missing. Select pants to recover instead.');
    return original.uniform;
  }
  const unavailable = uniformUnavailable(player, option);
  if (unavailable) throw new Error(unavailable);
  if (requiresUniform(option)) return player.uniform;
  // Release only a reference added by this manager, never an existing context.
  if (original?.uniform === 0 && original.managedUniform && player.uniform === original.managedUniform) return 0;
  return player.uniform;
}
class BlacktopManager {
  constructor({ helperPath, layoutPath, statePath, execute = run, onAutoStatus }) {
    Object.assign(this, { helperPath, layoutPath, statePath, execute, onAutoStatus });
    this.snapshot = null; this.busy = false; this.token = null;
    this.autoRandomSeen = new Set(); this.autoRandomSession = null;
    this.autoRandomRetryAt = 0; this.autoRandomRetrySignature = null;
    this.autoStatusAt = new Map();
    this.matchChoices=new BlacktopMatchChoices(`${statePath}.match-choices.json`);
  }
  emitAuto(phase, message='') {
    const key=`${phase}:${message}`,now=Date.now();
    if(now-(this.autoStatusAt.get(key)||0)<10000)return;
    this.autoStatusAt.set(key,now);
    try { void Promise.resolve(this.onAutoStatus?.({phase,message})).catch(()=>{}); } catch { /* diagnostics cannot interrupt outfit safety */ }
  }
  async exclusive(callback) {
    if (this.busy) throw new Error('An outfit operation is already running.');
    this.busy = true;
    try { return await callback(); } finally { this.busy = false; }
  }
  async invoke(action, gameRoot, requestPath) {
    try {
      const { stdout } = await this.execute(this.helperPath,
        [action, gameRoot, this.layoutPath, ...(requestPath ? [requestPath] : [])],
        { windowsHide: true, timeout: 15000, maxBuffer: 2 * 1024 * 1024, encoding: 'utf8' });
      return JSON.parse(stdout.trim());
    } catch (error) { throw new Error(String(error.stderr || error.message).trim()); }
  }
  async journal() {
    try { return JSON.parse(await fs.readFile(this.statePath, 'utf8')); }
    catch (error) { if (error.code === 'ENOENT') return {}; throw error; }
  }
  async focusGame(gameRoot) { return this.invoke('focus', gameRoot); }
  key(snapshot, player) { return `${snapshot.pid}:${snapshot.started}:${player.description}:${player.roster}`; }
  async scan(gameRoot,options) { return this.exclusive(() => this.scanUnlocked(gameRoot,options?.omitCatalog!==true)); }
  async scanUnlocked(gameRoot,includeCatalog=true) {
    this.snapshot = null; this.token = null;
    const snapshot = await this.invoke('scan', gameRoot);
    if (typeof snapshot.started !== 'string') throw new Error('Invalid game-session identity.');
    const journal = await this.journal();
    this.snapshot = snapshot; this.gameRoot = path.resolve(gameRoot); this.token = crypto.randomUUID();
    return { token: this.token, ...(includeCatalog?staticCatalogView():{}), players: snapshot.players.map(p => {
      const original = journal[this.key(snapshot, p)];
      const street = Number.isInteger(p.streetIndex) && p.streetIndex >= 0 ? streetCatalog.presets[p.streetIndex] : null;
      const current = p.outfits.find(o => o.pointer === p.pointer);
      const streetAccessories=current?.items.filter(a=>accessoryAssetSet().has(a))||[];
      const originalSock=street?.accessories?.find(s=>/^(female_)?acc_sock_/.test(s));
      const template=street?layerItems(streetCatalog,street,p.layer).filter(a=>!p.rosterShoes||!street.shoes?.includes(a)):null;
      return { slot: p.slot, name: p.name, teamId:p.teamId, teamName:p.teamName, cyberface: p.cyberface, error: p.error, clothingPool: playerClothingPool(p),
        selected: current?.id || '', streetId: normalizeAccessories(streetCatalog,{streetId:street?.id}).streetId,streetAccessories,streetAccessory:streetAccessories[0]||'',streetLayer:p.layer||'', streetBottom: template ? current?.items[template.indexOf(street.bottom)] : undefined,
        streetShoe:p.rosterShoes?'roster':template&&street?.shoes?.length===1 ? current?.items[template.indexOf(street.shoes[0])] : undefined,
        streetSock:template ? originalSock ? current?.items[template.indexOf(originalSock)] : current?.items[template.length] : undefined,
        outfits: p.outfits.map(o => ({ id: o.id, label: outfitLabel(o.items), items: o.items,
          itemLabels: o.items.map(clothingLabel), unavailable: uniformUnavailable(p, o) })),
        canRestore: Boolean(original && (original.pointer !== p.pointer || (Object.hasOwn(original, 'uniform') && original.uniform !== p.uniform))),
      };
    }) };
  }
  async apply(gameRoot, value) {
    return this.exclusive(() => this.applyUnlocked(gameRoot, value));
  }
  async autoRandomSelection(gameRoot, observation) {
    if (observation?.selection !== true || observation.playersReady !== true || !Array.isArray(observation.lineupPlayers)) return;
    const entries=observation.lineupPlayers;
    if(entries.length>10||entries.some(p=>!Number.isInteger(p.slot)||p.slot<0||p.slot>9||!Number.isSafeInteger(p.description)||p.description<=0||!Number.isSafeInteger(p.roster)||p.roster<=0)||new Set(entries.map(p=>p.slot)).size!==entries.length){this.emitAuto('invalid-lineup');return;}
    if(this.autoRandomSession!==observation.session){this.autoRandomSession=observation.session;this.autoRandomSeen.clear();this.autoRandomRetryAt=0;this.autoRandomRetrySignature=null;this.autoStatusAt.clear();this.emitAuto('new-session',`${entries.length} players`);}
    const identity=p=>`${p.slot}:${p.description}:${p.roster}`;
    const active=new Set(entries.map(identity));
    for(const key of this.autoRandomSeen)if(!active.has(key))this.autoRandomSeen.delete(key);
    if(this.busy){this.emitAuto('busy');return;}
    if(entries.every(p=>this.autoRandomSeen.has(identity(p)))){this.emitAuto('all-seen',`${entries.length} players`);return;}
    // A new occupant should not inherit the previous lineup's retry delay.
    // Still throttle repeated scans of the same unstable lineup.
    const retrySignature=[...active].sort().join('|');
    if(retrySignature===this.autoRandomRetrySignature&&Date.now()<this.autoRandomRetryAt)return;
    this.autoRandomRetrySignature=retrySignature;
    this.autoRandomRetryAt=Date.now()+1000;
    return this.exclusive(async()=>{
      this.emitAuto('scan-start',`${entries.length} players`);
      let view=await this.scanUnlocked(gameRoot,false);
      const matches=()=>this.snapshot&&`${this.snapshot.pid}:${this.snapshot.started}`===observation.session&&
        this.snapshot.players.length===entries.length&&entries.every(p=>this.snapshot.players.some(q=>identity(p)===identity(q)));
      if(!matches()){this.emitAuto('scan-mismatch',`${entries.length} detected, ${this.snapshot?.players?.length??0} scanned`);return;}
      this.emitAuto('scan-match',`${entries.length} players`);
      const journal=await this.journal();
      for(const entry of entries){
        const key=identity(entry);if(this.autoRandomSeen.has(key))continue;
        if(!matches()){this.emitAuto('snapshot-changed');break;}
        const player=this.snapshot.players.find(p=>p.slot===entry.slot);
        if(player.error||!['male','female'].includes(playerClothingPool(player))){this.emitAuto('player-unavailable',`slot ${entry.slot}`);continue;}
        // Existing manual choices win, including after a launcher restart.
        if(journal[this.key(this.snapshot,player)]){this.autoRandomSeen.add(key);this.emitAuto('manual-preserved',`slot ${entry.slot}`);continue;}
        // An uncertain native result must never cause repeated automatic writes.
        this.autoRandomSeen.add(key);
        this.emitAuto('apply-start',`slot ${entry.slot}`);
        try { view=await this.applyUnlocked(gameRoot,{token:view.token,slot:entry.slot,randomFit:true,randomMode:'all',omitCatalog:true,rosterShoes:true}); }
        catch(error){this.emitAuto('apply-error',`slot ${entry.slot}: ${error.message}`);throw error;}
        const r=view.report;
        if(!r||(!r.unchanged&&!(r.consumed&&r.pointerMatches&&r.uniformMatches&&r.countMatches!==false))){this.emitAuto('apply-unverified',`slot ${entry.slot}`);break;}
        this.emitAuto('apply-verified',`slot ${entry.slot}; carryover ${r.matchCarryover||'unknown'}; native ${r.timings?.nativeApplyMs??'unknown'}ms; verify ${r.timings?.verifiedScanMs??'unknown'}ms`);
      }
      return view;
    });
  }
  async applyUnlocked(gameRoot, value) {
      if (!this.snapshot || value?.token !== this.token || path.resolve(gameRoot) !== this.gameRoot) throw new Error('Refresh Blacktop players before applying.');
      const snapshot = this.snapshot;
      const player = snapshot.players.find(p => p.slot === value.slot);
      if (!player || player.error) throw new Error('That player is not available for outfit changes.');
      const journal = await this.journal(); const key = this.key(snapshot, player);
      let option = value.restore === true
        ? player.outfits.find(o => o.pointer === journal[key]?.pointer)
        : resolveOutfit(player, value);
      if (!option && value.restore === true && journal[key]) {
        const saved=journal[key];
        const originalRequest={pid:snapshot.pid,started:snapshot.started,controller:snapshot.controller,slot:player.slot,
          description:player.description,model:player.model,roster:player.roster,expected:player.pointer,expectedCount:player.count ?? 2,
          target:saved.pointer,targetCount:saved.count ?? 2,expectedUniform:player.uniform,targetUniform:saved.uniform ?? player.uniform,uniformProtocol:5};
        const requestPath=`${this.statePath}.${crypto.randomUUID()}.original.json`;
        await fs.writeFile(requestPath,JSON.stringify(originalRequest),{flag:'wx'});
        try { option=await this.invoke('original',gameRoot,requestPath); } finally { await fs.unlink(requestPath).catch(()=>{}); }
      }
      if (!option) throw new Error('Choose a verified top and bottom from this player\'s wardrobe, or refresh players.');
      const targetUniform = uniformTarget(player, option, journal[key], value.restore === true);
      const current = player.outfits.find(o => o.pointer === player.pointer);
      if ((option.pointer === player.pointer || (option.streetId && current && JSON.stringify(current.items) === JSON.stringify(option.items))) && targetUniform === player.uniform) {
        const verified=Array.isArray(current?.items)&&JSON.stringify(current.items)===JSON.stringify(option.items);
        const supported=verified&&(value.restore===true||current.items.length>=3&&current.items.length<=7);
        if(verified)await this.matchChoices.record(snapshot,player,current.items,value.restore===true);
        return {report:{consumed:true,pointerMatches:true,countMatches:true,uniformMatches:true,unchanged:true,
          matchCarryover:supported?'queued':verified?'unsupported-piece-count':'not-verified'},
          ...await this.scanUnlocked(gameRoot,value.omitCatalog!==true)};
      }
      await fs.mkdir(path.dirname(this.statePath), { recursive: true });
      if (!journal[key]) journal[key] = { pointer: player.pointer, name: player.name, slot: player.slot };
      // Upgrade old outfit-only journals without inventing a past context value.
      if (!Object.hasOwn(journal[key], 'uniform')) journal[key].uniform = player.uniform;
      if (!Object.hasOwn(journal[key], 'count')) journal[key].count = player.count ?? current?.items.length ?? 2;
      if (option.composed) {
        const prefix = `${snapshot.pid}:${snapshot.started}:`;
        const reserved = Object.entries(journal).filter(([id]) => id.startsWith(prefix)).reduce((n, [, value]) => n + (value.composeReservations || 0), 0);
        if (reserved >= 256) throw new Error('This game session has reached its custom-outfit limit. Restart the game to clear temporary clothing lists.');
        // Reserve before invoking native code, including failed attempts. At most
        // 256 read-only 4-KiB lists per session; Windows reclaims them on game exit.
        journal[key].composeReservations = (journal[key].composeReservations || 0) + 1;
      }
      if (player.uniform === 0 && targetUniform !== 0) journal[key].managedUniform = targetUniform;
      // Persist original before making any memory write. Keep it across Apply/Refresh/app restart.
      const temp = `${this.statePath}.${crypto.randomUUID()}.tmp`;
      const file = await fs.open(temp, 'wx');
      try { await file.writeFile(JSON.stringify(journal)); await file.sync(); } finally { await file.close(); }
      await fs.rename(temp, this.statePath);
      const requestPath = `${this.statePath}.${crypto.randomUUID()}.request.json`;
      const request = { pid: snapshot.pid, started: snapshot.started, controller: snapshot.controller,
        slot: player.slot, description: player.description, model: player.model, roster: player.roster,
        expected: player.pointer, expectedCount: player.count ?? current?.items.length ?? 2, target: option.pointer, targetCount: option.items.length,
        top: option.items[0], bottom: option.bottom ?? option.items[1], streetId: option.streetId, shoe:option.shoe,sock:option.sock,layer:option.layer,accessory:option.accessory,accessories:option.accessories, items: option.items, uniformProtocol: 5,
        expectedUniform: player.uniform, targetUniform };
      await fs.writeFile(requestPath, JSON.stringify(request), { flag: 'wx' });
      this.snapshot = null; this.token = null;
      try {
        const applyStarted=performance.now();
        const report = await this.invoke('apply', gameRoot, requestPath);
        const applyFinished=performance.now();
        const refreshed=await this.scanUnlocked(gameRoot,value.omitCatalog!==true);
        const scanFinished=performance.now();
        report.timings={nativeApplyMs:Math.round(applyFinished-applyStarted),verifiedScanMs:Math.round(scanFinished-applyFinished)};
        const verified=this.snapshot.players.find(p=>p.roster===player.roster&&p.slot===player.slot);
        const actual=verified?.outfits.find(o=>o.pointer===verified.pointer);
        const exactMatch=report.consumed&&report.pointerMatches&&report.uniformMatches&&actual&&
           this.snapshot.pid===snapshot.pid&&this.snapshot.started===snapshot.started&&
           JSON.stringify(actual.items)===JSON.stringify(option.items);
        const supported=exactMatch&&(value.restore===true||actual.items.length>=3&&actual.items.length<=7);
        if(exactMatch) {
          await this.matchChoices.record(snapshot,player,actual.items,value.restore===true);
        }
        report.matchCarryover=supported?'queued':exactMatch?'unsupported-piece-count':'not-verified';
        return { report, ...refreshed };
      } finally { await fs.unlink(requestPath).catch(() => {}); }
  }
}
module.exports = { BlacktopManager, outfitLabel, clothingLabel, resolveOutfit, resolveBlacktopLayoutPath, uniformTarget, uniformUnavailable, playerClothingPool, randomAccessories, randomTeamClothes };
