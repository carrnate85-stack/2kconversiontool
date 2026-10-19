'use strict';
const fs=require('node:fs/promises');
const path=require('node:path');
const {randomUUID}=require('node:crypto');
const catalog=require('../data/streetwear-components.json');
const shoeLabels=require('./shoe-labels');
const {composeStreetwear,layerChoices,accessoryChoices,normalizeAccessories}=require('./clothing-components');
const SCHEMA='nba2k27-outfit-presets-v1';
const validId=id=>typeof id==='string' && /^[0-9a-f-]{36}$/.test(id);
const label=asset=>(asset||'').replace(/^female_/,'').replaceAll('_',' ');
function choices() {
  return catalog.presets.filter(p=>p.top&&p.bottom&&['male','female'].includes(p.sourceGroup.pool)&&!p.id.startsWith('accessory-test-')).sort((a,b)=>Number(b.id==='shirtless-male')-Number(a.id==='shirtless-male')).map(p=>({
    id:p.id,pool:p.sourceGroup.pool,top:p.top,previewAsset:p.previewAsset||p.top,bottom:p.bottom,topLabel:p.label||label(p.top),bottomLabel:label(p.bottom),
    separable:p.separable,experimental:Boolean(p.experimental),extras:p.original.length-2,shoes:p.shoes||[],socks:(p.accessories||[]).filter(s=>/^(female_)?acc_sock_/.test(s)),layers:p.layers||[],layerChoices:layerChoices(catalog,p)
  }));
}
function selection(value) {
  value=normalizeAccessories(catalog,value||{});
  const preset=catalog.presets.find(p=>p.id===value?.streetId);
  if(!preset?.top || !preset.bottom || !['male','female'].includes(preset.sourceGroup.pool)
    || !composeStreetwear(catalog,value.streetId,value.bottom,value.shoe,value.sock,value.layer,value.accessory,value.accessories)) throw Error('Choose compatible streetwear pieces (seven pieces maximum, including layers and accessories).');
  const composed=composeStreetwear(catalog,value.streetId,value.bottom,value.shoe,value.sock,value.layer,value.accessory,value.accessories);
  return {streetId:preset.id,bottom:value.bottom,pool:preset.sourceGroup.pool,...(value.shoe ? {shoe:value.shoe} : {}),...(value.sock?{sock:value.sock}:{}),...(value.layer?{layer:value.layer}:{}),...(composed.accessories!==undefined?{accessories:composed.accessories}:value.accessory!==undefined?{accessory:value.accessory}:{})};
}
class OutfitPresets {
  constructor(file) {this.file=file;this.tail=Promise.resolve();}
  async read() {
    let raw;
    try {
      if((await fs.stat(this.file)).size>1024*1024)throw Error('Outfit library is too large; it was left untouched.');
      raw=JSON.parse(await fs.readFile(this.file,'utf8'));
    } catch(error) {if(error.code==='ENOENT')return [];throw error;}
    if(raw.schema!==SCHEMA||!Array.isArray(raw.outfits)||raw.outfits.length>1000
      ||raw.outfits.some(o=>!validId(o.id)||typeof o.name!=='string'||!o.name.trim()||o.name.length>64)
      ||new Set(raw.outfits.map(o=>o.id)).size!==raw.outfits.length)throw Error('Invalid outfit library; it was left untouched.');
    return raw.outfits;
  }
  async list(options) {
    const outfits=(await this.read()).map(o=>{
      try {return {id:o.id,name:o.name,...selection(o),available:true};}
      catch {return {id:o.id,name:o.name,available:false};}
    });
    // The F8 refresh only needs the fresh saved presets. Other callers keep
    // the complete library used by the outfit creator/mannequin by default.
    if(options?.omitCatalog===true)return {outfits};
    return {outfits,catalog:choices(),accessories:accessoryChoices(catalog),shoes:catalog.additionalShoes||[],shoeLabels};
  }
  async resolve(id) {
    const record=(await this.read()).find(o=>o.id===id);
    if(!record)throw Error('Saved outfit no longer exists. Refresh the outfit list.');
    return selection(record);
  }
  mutate(action) {
    const job=this.tail.then(async()=>{
      const outfits=await this.read();const result=action(outfits);
      await fs.mkdir(path.dirname(this.file),{recursive:true});
      const temporary=this.file+'.'+randomUUID()+'.tmp';
      try {
        await fs.writeFile(temporary,JSON.stringify({schema:SCHEMA,outfits},null,2),{flag:'wx'});
        await fs.rename(temporary,this.file);
      } finally {await fs.unlink(temporary).catch(error=>{if(error.code!=='ENOENT')throw error;});}
      return result;
    });
    this.tail=job.catch(()=>{});return job;
  }
  save(value) {
    return this.mutate(outfits=>{
      if(typeof value?.name!=='string'||!value.name.trim()||value.name.trim().length>64||/[\u0000-\u001f\u007f]/.test(value.name))throw Error('Enter an outfit name (1–64 characters).');
      const pieces=selection(value);
      const index=value.id?outfits.findIndex(o=>o.id===value.id):-1;
      if(value.id&&index<0)throw Error('Saved outfit no longer exists. Create a new outfit instead.');
      if(index<0&&outfits.length>=1000)throw Error('Outfit library limit reached (1,000).');
      const record={id:index<0?randomUUID():outfits[index].id,name:value.name.trim(),...pieces};
      if(index<0)outfits.push(record);else outfits[index]=record;
      return record;
    });
  }
  remove(id) {
    return this.mutate(outfits=>{
      const index=outfits.findIndex(o=>o.id===id);
      if(index<0)throw Error('Saved outfit no longer exists.');
      outfits.splice(index,1);return {removed:true};
    });
  }
}
module.exports={OutfitPresets,selection,choices};
