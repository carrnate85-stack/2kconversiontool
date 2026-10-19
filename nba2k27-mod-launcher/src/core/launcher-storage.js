'use strict';
const fs=require('node:fs');
const path=require('node:path');

const FOLDER='2k Launcher';
const POINTER='nba2k27-mod-launcher-location.json';
const NATIVE_POINTER='nba2k27-mod-launcher-location.ini';
const legacyRoot=appData=>path.join(appData,'nba2k27-mod-launcher');
const appDataRoot=appData=>path.join(appData,FOLDER);
const pointerFile=appData=>path.join(appData,POINTER);
const nativePointerFile=appData=>path.join(appData,NATIVE_POINTER);
const storageRoot=gameRoot=>path.join(path.resolve(gameRoot),FOLDER);
const portableRoot=executable=>{
 const directory=path.dirname(path.resolve(executable));
 return path.basename(directory).toLowerCase()==='app'?path.dirname(directory):directory;
};

function layout(root){
 return {
  root,data:path.join(root,'Data'),music:path.join(root,'Library','Music'),
  videos:path.join(root,'Library','Videos'),previews:path.join(root,'Library','Preview Packs'),
  outfits:path.join(root,'Library','Outfits'),cache:path.join(root,'Cache'),
  chromium:path.join(root,'Cache','Chromium'),runtime:path.join(root,'Runtime'),
  recovery:path.join(root,'Recovery'),logs:path.join(root,'Logs'),
 };
}

// Validate the explicit WPF root without probing/importing legacy AppData.
function resolveContainedStorage(root){
 const paths=layout(path.resolve(root));
 try{
  const check=(target,directory,required=false)=>{
   let stat;try{stat=fs.lstatSync(target);}catch(error){if(error.code==='ENOENT'&&!required)return;throw error;}
   if(stat.isSymbolicLink()||(directory?!stat.isDirectory():!stat.isFile()))throw Error(`Portable storage is not a regular ${directory?'directory':'file'}: ${target}`);
  };
  check(paths.root,true,true);
  for(const relative of ['Data','Library','Library/Music','Library/Videos','Library/Preview Packs','Library/Outfits','Cache','Cache/Temp','Cache/Chromium','Cache/WebView2','Runtime','Recovery','Logs'])check(path.join(paths.root,relative),true);
  if(fs.existsSync(path.join(paths.data,'.portable-import-pending')))throw Error('Portable import is incomplete. The existing library was preserved.');
  const file=path.join(paths.data,'settings.json');check(file,false);
  if(fs.existsSync(file)){
   const settings=JSON.parse(fs.readFileSync(file,'utf8'));
   if(!settings||typeof settings!=='object'||Array.isArray(settings))throw Error('Portable settings are invalid.');
  }
  return {mode:'organized',placement:'portable',needsPortableMigration:false,...paths};
 }catch(error){return {mode:'blocked',placement:'portable',...paths,error};}
}

function readLocation(appData){
 const file=pointerFile(appData);
 let raw;
 try{raw=fs.readFileSync(file,'utf8');}catch(error){if(error.code==='ENOENT')return null;throw error;}
 const record=JSON.parse(raw);
 if(record?.schema!==1||typeof record.gameRoot!=='string'||!path.isAbsolute(record.gameRoot))throw Error('Launcher data location pointer is invalid.');
 const gameRoot=path.resolve(record.gameRoot),root=storageRoot(gameRoot);
 if(record.root!==root)throw Error('Launcher data location pointer does not match the game folder.');
 const game=fs.statSync(path.join(gameRoot,'NBA2K27.exe'));
 const folder=fs.lstatSync(root),data=fs.lstatSync(path.join(root,'Data'));
 if(!game.isFile()||!folder.isDirectory()||folder.isSymbolicLink()||!data.isDirectory()||data.isSymbolicLink())throw Error('Launcher data folder is unavailable. Restore it or update the location pointer.');
 return {placement:'game',gameRoot,...layout(root)};
}

function readAppDataLocation(appData){
 const root=appDataRoot(appData);
 let folder;
 try{folder=fs.lstatSync(root);}catch(error){if(error.code==='ENOENT')return null;throw error;}
 if(!folder.isDirectory()||folder.isSymbolicLink())throw Error('AppData launcher folder is not a regular directory.');
 const data=fs.lstatSync(path.join(root,'Data'));
 if(!data.isDirectory()||data.isSymbolicLink())throw Error('AppData launcher Data folder is unavailable.');
 const settingsFile=path.join(root,'Data','settings.json');
 let gameRoot=null,hasSettings=false;
 try{
  const settings=JSON.parse(fs.readFileSync(settingsFile,'utf8'));
  hasSettings=true;
  if(typeof settings?.gameRoot==='string'&&path.isAbsolute(settings.gameRoot))gameRoot=path.resolve(settings.gameRoot);
 }catch(error){if(error.code!=='ENOENT')throw error;}
 return {placement:'appdata',gameRoot,hasSettings,...layout(root)};
}

function readMigratedLocationFromLegacy(appData){
 const source=legacyRoot(appData),settingsFile=path.join(source,'settings.json');
 let settings;
 try{settings=JSON.parse(fs.readFileSync(settingsFile,'utf8'));}catch(error){if(error.code==='ENOENT')return null;throw error;}
 if(typeof settings?.gameRoot!=='string'||!path.isAbsolute(settings.gameRoot))return null;
 const gameRoot=path.resolve(settings.gameRoot),root=storageRoot(gameRoot);
 const manifestFile=path.join(root,'Data','migration-manifest.json');
 let manifest;
 try{manifest=JSON.parse(fs.readFileSync(manifestFile,'utf8'));}catch(error){if(error.code==='ENOENT')return null;throw error;}
 if(manifest?.schema!==1||manifest.originalsPreserved!==true||typeof manifest.source!=='string'||path.basename(manifest.source).toLowerCase()!=='nba2k27-mod-launcher')return null;
 const game=fs.lstatSync(path.join(gameRoot,'NBA2K27.exe'));
 const folder=fs.lstatSync(root),data=fs.lstatSync(path.join(root,'Data'));
 if(!game.isFile()||game.isSymbolicLink()||!folder.isDirectory()||folder.isSymbolicLink()||!data.isDirectory()||data.isSymbolicLink())throw Error('Migrated launcher data folder is unavailable.');
 if(!fs.statSync(path.join(root,'Data','settings.json')).isFile())throw Error('Migrated launcher settings are missing.');
 return {placement:'game',gameRoot,...layout(root)};
}

function resolveStorage({appData,selfTestOutput}){
 if(selfTestOutput)return {mode:'self-test',data:path.join(path.dirname(path.resolve(selfTestOutput)),'launcher-self-test-data')};
 try{
  const appDataLocation=readAppDataLocation(appData);
  if(appDataLocation){
   if(!appDataLocation.hasSettings&&(fs.existsSync(pointerFile(appData))||fs.existsSync(path.join(legacyRoot(appData),'settings.json'))))
    throw Error('AppData launcher folder is incomplete. The previous library was preserved; do not start with empty settings.');
   return {mode:'organized',...appDataLocation};
  }
  const location=readLocation(appData);
  if(location)return {mode:'organized',needsAppDataMigration:true,...location};
  const migrated=readMigratedLocationFromLegacy(appData);
  if(migrated)return {mode:'organized',needsAppDataMigration:true,...migrated};
  if(fs.existsSync(path.join(legacyRoot(appData),'settings.json')))return {mode:'legacy',needsAppDataMigration:true,data:legacyRoot(appData)};
  return {mode:'organized',placement:'appdata',gameRoot:null,...layout(appDataRoot(appData))};
 }catch(error){return {mode:'blocked',data:legacyRoot(appData),error};}
}

// A folder release carries its executable in App/. All persistent launcher
// state lives beside App/, never beside the game or in hidden OS directories.
// Older libraries are read only here; their verified import runs after ready.
function resolvePortableStorage({executable,appData,selfTestOutput}){
 if(selfTestOutput)return {mode:'self-test',data:path.join(path.dirname(path.resolve(selfTestOutput)),'launcher-self-test-data')};
 const root=portableRoot(executable),paths=layout(root),settingsFile=path.join(paths.data,'settings.json');
 try{
  const folder=fs.lstatSync(root);
  if(!folder.isDirectory()||folder.isSymbolicLink())throw Error('The portable launcher folder is not a regular directory.');
  const appFolder=path.dirname(path.resolve(executable));
  if(path.basename(appFolder).toLowerCase()!=='app')throw Error('The portable executable must remain inside its App folder. Extract the complete release folder before starting it.');
  const appStat=fs.lstatSync(appFolder);
  if(!appStat.isDirectory()||appStat.isSymbolicLink())throw Error('The portable App folder is not a regular directory.');
  const importPending=fs.existsSync(path.join(paths.data,'.portable-import-pending'));
  if(fs.existsSync(settingsFile)&&!importPending){
   const settings=JSON.parse(fs.readFileSync(settingsFile,'utf8'));
   if(!settings||typeof settings!=='object'||Array.isArray(settings))throw Error('Portable settings are invalid.');
   return {mode:'organized',placement:'portable',...paths};
  }
  const previous=resolveStorage({appData});
  if(previous.mode==='blocked')throw previous.error;
  const source=previous.mode==='legacy'||(previous.mode==='organized'&&(
   previous.placement==='game'||fs.existsSync(path.join(previous.data,'settings.json'))))?previous:null;
  if(importPending&&!source)throw Error('Portable import is incomplete, and its previous library is unavailable. No empty settings were created.');
  return {mode:'organized',placement:'portable',needsPortableMigration:Boolean(source),portableImportPending:importPending,migrationSource:source,...paths};
 }catch(error){return {mode:'blocked',placement:'portable',...paths,error};}
}

// The portable bootstrap cannot parse JSON before Electron starts. This tiny,
// derived hint is never authoritative: the app still validates the JSON pointer.
function ensureNativeLocationHint(appData,location){
 if(location?.mode!=='organized')return false;
 const file=nativePointerFile(appData),body=`[Launcher]\r\nRoot=${location.root}\r\n`;
 try{if(fs.readFileSync(file,'utf8')===body)return false;}catch(error){if(error.code!=='ENOENT')throw error;}
 const temporary=path.join(appData,`.launcher-native-location-${process.pid}.tmp`);
 try{fs.writeFileSync(temporary,body,{flag:'wx'});fs.renameSync(temporary,file);}
 finally{try{fs.unlinkSync(temporary);}catch(error){if(error.code!=='ENOENT')throw error;}}
 return true;
}

module.exports={FOLDER,POINTER,NATIVE_POINTER,legacyRoot,appDataRoot,pointerFile,nativePointerFile,storageRoot,portableRoot,layout,resolveContainedStorage,readLocation,readAppDataLocation,readMigratedLocationFromLegacy,resolveStorage,resolvePortableStorage,ensureNativeLocationHint};
