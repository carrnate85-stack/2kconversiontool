const fsp = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { parseZip, replaceZipEntries } = require('./zip-replacement');

const exec = promisify(execFile);
const LOGICAL_PATH = 'gooey_scenes/startup_boot_transition.iff';
const LOGO_MEMBER = 'logo_sku_white_2048.c49d1530a3d568ba.dds';
const BACKGROUND_MEMBER = 'bg_keyart_texttexture_main.74ca0cc89a9d809d.dds';
const SCENE_MEMBER = 'startup_boot_transition.SCNE';
const STOCK_MEMBER_HASHES = Object.freeze({
  [SCENE_MEMBER]:'11E0078D65CAAE913DB82E81F62C128A643239106F76225C4F0948882F09E11D',
  [LOGO_MEMBER]:'06A3FFA198BD05729323FA5DDB9B18B6B08E23F547FAE73F397CAFCCF98AF9CC',
  [BACKGROUND_MEMBER]:'C0A1A025F2FD453A6BA93CBE38C9C822FE0523D56C22F08853C40A1BBE4748FE',
});

const sha256 = bytes => crypto.createHash('sha256').update(bytes).digest('hex').toUpperCase();

function validateDds(bytes,width=2048,height=1024) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 148 || bytes.subarray(0,4).toString('ascii') !== 'DDS ' || bytes.subarray(84,88).toString('ascii') !== 'DX10') throw new Error('Converted splash is not a DX10 DDS texture.');
  const actualWidth=bytes.readUInt32LE(16),actualHeight=bytes.readUInt32LE(12),mips=bytes.readUInt32LE(28),format=bytes.readUInt32LE(128);
  if(actualWidth!==width||actualHeight!==height||mips!==12||format!==98)throw new Error(`Converted splash has an unexpected layout (${actualWidth}x${actualHeight}, ${mips} mips, DXGI ${format}).`);
}

function withStockDdsHeader(replacement,stock){
  validateDds(replacement,stock.readUInt32LE(16),stock.readUInt32LE(12));
  validateDds(stock,stock.readUInt32LE(16),stock.readUInt32LE(12));
  if(replacement.length!==stock.length)throw new Error('Converted splash size does not match the stock texture layout.');
  const output=Buffer.from(replacement);
  stock.copy(output,0,0,148);
  return output;
}

function namedObjectRange(source,name,start=0,end=source.length){
  const marker=`"${name}": {`;
  const markerAt=source.indexOf(marker,start);
  if(markerAt<0||markerAt>=end)throw new Error(`The installed startup scene is missing ${name}.`);
  const open=source.indexOf('{',markerAt+name.length+2);
  let depth=0,inString=false,escaped=false;
  for(let index=open;index<end;index+=1){
    const char=source[index];
    if(inString){
      if(escaped)escaped=false;
      else if(char==='\\')escaped=true;
      else if(char==='"')inString=false;
      continue;
    }
    if(char==='"'){inString=true;continue;}
    if(char==='{')depth+=1;
    else if(char==='}'&&--depth===0)return {start:open,end:index+1};
  }
  throw new Error(`The installed startup scene has an incomplete ${name} object.`);
}

function suppressStockOverlayModels(sceneBytes){
  const source=sceneBytes.toString('utf8');
  if(!Buffer.from(source,'utf8').equals(sceneBytes))throw new Error('The installed startup scene text encoding is unsupported.');
  const model=namedObjectRange(source,'Model');
  const expectedCounts=new Map([
    ['swoop_SVG_sprite_VFX_Swoop',24],
    ['logo_bg',1],
    ['sku_pieces_grp',13],
    ['logo_sku',1],
  ]);
  const edits=[];
  for(const [name,expected] of expectedCounts){
    const range=namedObjectRange(source,name,model.start,model.end);
    const block=source.slice(range.start,range.end);
    const pattern=/("Count"[\t ]*:[\t ]*)([1-9][0-9]*)/g;
    let match;
    while((match=pattern.exec(block))!==null){
      const valueStart=range.start+match.index+match[1].length;
      edits.push({start:valueStart,length:match[2].length});
    }
    const actual=edits.filter(edit=>edit.start>=range.start&&edit.start<range.end).length;
    if(actual!==expected)throw new Error(`The installed startup scene ${name} layout is unsupported.`);
  }
  let output=source;
  for(const edit of edits.sort((left,right)=>right.start-left.start))output=`${output.slice(0,edit.start)}0${output.slice(edit.start+edit.length)}`;
  return Buffer.from(output,'utf8');
}

async function findManifestEntry(gameRoot) {
  const manifest = await fsp.readFile(path.join(gameRoot,'manifest'),'utf8');
  const line = manifest.split(/\r?\n/).find(value=>value.startsWith(`${LOGICAL_PATH},`));
  if(!line)throw new Error('The startup transition is missing from the installed manifest.');
  const parts=line.split(',');
  if(parts.length!==4||!/^[0-9A-Z]{2}$/.test(parts[1]))throw new Error('The startup transition manifest entry is invalid.');
  const offset=Number(parts[2]),size=Number(parts[3]);
  if(!Number.isSafeInteger(offset)||offset<0||!Number.isSafeInteger(size)||size<1||size>16*1024*1024)throw new Error('The startup transition manifest range is invalid.');
  return {archive:parts[1],offset,size};
}

async function buildStartupTransition({gameRoot,pngPath,outputPath,decoderPath,texconvPath,tempDirectory}) {
  const entry=await findManifestEntry(gameRoot);
  const work=path.join(tempDirectory,`startup-transition-${process.pid}-${crypto.randomUUID()}`);
  await fsp.mkdir(work,{recursive:true});
  try {
    const stockPath=path.join(work,'stock.iff');
    await exec(decoderPath,[path.join(gameRoot,entry.archive),String(entry.offset),String(entry.size),path.join(gameRoot,'data','oodle','oo2core_9_win64.dll'),stockPath],{windowsHide:true,timeout:60000,maxBuffer:65536});
    const stock=await fsp.readFile(stockPath);
    const parsed=parseZip(stock);
    for(const [name,expected] of Object.entries(STOCK_MEMBER_HASHES)){
      const member=parsed.entries.find(candidate=>candidate.name===name);
      if(!member||sha256(member.data)!==expected)throw new Error('The installed startup transition does not match the supported NBA 2K27 build.');
    }
    const originalLogo=parsed.entries.find(candidate=>candidate.name===LOGO_MEMBER),originalBackground=parsed.entries.find(candidate=>candidate.name===BACKGROUND_MEMBER),originalScene=parsed.entries.find(candidate=>candidate.name===SCENE_MEMBER);
    if(!originalLogo||!originalBackground||!originalScene)throw new Error('The supported startup presentation resources are missing.');
    validateDds(originalLogo.data,2048,512);validateDds(originalBackground.data,2048,1024);

    await exec(texconvPath,['-nologo','-y','-ft','dds','-f','BC7_UNORM','-m','0','-w','2048','-h','1024','-o',work,pngPath],{windowsHide:true,timeout:120000,maxBuffer:65536});
    const ddsPath=path.join(work,`${path.basename(pngPath,path.extname(pngPath))}.dds`);
    const converted=await fsp.readFile(ddsPath);
    validateDds(converted,2048,1024);
    const replacement=withStockDdsHeader(converted,originalBackground.data);
    const scene=suppressStockOverlayModels(originalScene.data);
    const rebuilt=replaceZipEntries(stock,new Map([[BACKGROUND_MEMBER,replacement],[SCENE_MEMBER,scene]]));
    await fsp.writeFile(outputPath,rebuilt,{flag:'wx'});
    return {bytes:rebuilt.length,sha256:sha256(rebuilt),members:[BACKGROUND_MEMBER,SCENE_MEMBER]};
  } finally { await fsp.rm(work,{recursive:true,force:true}).catch(()=>{}); }
}

module.exports={BACKGROUND_MEMBER,LOGICAL_PATH,LOGO_MEMBER,SCENE_MEMBER,STOCK_MEMBER_HASHES,buildStartupTransition,findManifestEntry,namedObjectRange,suppressStockOverlayModels,validateDds,withStockDdsHeader};
