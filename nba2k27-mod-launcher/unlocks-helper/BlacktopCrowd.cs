using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Threading;
using System.Collections.Generic;

// Data-only, Blacktop-visit scoped. Apply at exact selection; restore at the
// verified inactive main menu. Never write during loading/gameplay. No hooks,
// game-file changes, GameData writes, remote calls or ambient-pocket changes.
internal static class BlacktopCrowdPolicy {
 internal const uint Timestamp=0x6AA2013D, ImageSize=0x36558000;
 internal const ulong OwnerPointer=0xAA51F50, WindowOffset=0x2C48, GetterRva=0x26BAF80;
 internal const ulong PreloadTablePointer=0xAACAFE8, RecordStride=0xC810, FrontWindowOffset=0x7428;
 internal const uint FirstBlacktopRecord=175, LastBlacktopRecord=179;
 internal const int MaxWrites=6;
 internal const int WindowSize=16, CrowdIndex=1, FrontWindowSize=24, FrontIndex=8;
 internal const string GetterHex="4883EC28E85700E1FF837808007F1E803D7A134108007515488B05B16F3908803801751C4883C428E9C39B3E01488B05EC0941080FB680F90C00004883C428C30FB680492C00004883C428C3";
 internal static byte[] Hex(string hex){return Enumerable.Range(0,hex.Length/2).Select(i=>Convert.ToByte(hex.Substring(i*2,2),16)).ToArray();}
 internal static bool Selection(uint a,uint b,uint stage,byte active,byte kind){return a==0x92DF06DE&&b==a&&stage==3&&active==0&&kind==0;}
 // Both inactive exits were observed with identical owned preload layouts.
 // Do not infer a safe exit from an arbitrary non-Blacktop/gameplay mode.
 internal static bool MainMenu(uint a,uint b,uint stage,byte active,byte kind){return (a==0x6509837B||a==0x079964C0)&&b==a&&stage==1&&active==0&&kind==0;}
 internal static bool ValidOwner(ulong owner){return owner>=0x10000&&owner<=0x7fffffffffffUL-0x7448;}
 internal static bool BlacktopRecord(uint index){return index>=FirstBlacktopRecord&&index<=LastBlacktopRecord;}
 internal static ulong SourceRow(ulong table,uint index){
  if(!BlacktopRecord(index))return 0;
  ulong offset=index*RecordStride+8;
  return table>=0x10000&&table%8==0&&table<=0x7fffffffffffUL-offset-0x7448?table+offset:0;
 }
 // Selection becomes visible before the preload object finishes initializing.
 // An unowned object may wait, but never use an unverified tag for a write.
 // Once we own a change, an unexpected tag remains an ownership conflict.
 internal static string SourceTagPhase(bool owned,uint index,uint tag){return BlacktopRecord(index)&&tag==index?"ready":owned?"conflict":"waiting";}
 internal static bool ValidFront(byte[] values){return values!=null&&values.Length==FrontWindowSize&&values[0]==0&&values[1]==0x4C&&values[FrontIndex]<=1;}
 internal static bool UnselectedEqual(byte[] before,byte[] after,int index){return before!=null&&after!=null&&before.Length==after.Length&&Enumerable.Range(0,before.Length).Where(i=>i!=index).All(i=>before[i]==after[i]);}
 internal static bool OwnedValues(byte original,byte current,byte[] frontOriginal,byte[] front){
  return original<=1&&current<=1&&(current==0||current==original)&&ValidFront(frontOriginal)&&ValidFront(front)&&UnselectedEqual(frontOriginal,front,FrontIndex)&&(front[FrontIndex]==0||front[FrontIndex]==frontOriginal[FrontIndex]);
 }
 internal static bool Owns(string phase){return phase=="applied"||phase=="partial";}
 internal static string FailurePhase(bool waiting,bool attempted,string previous){return waiting&&attempted?"partial":attempted?"failed":Owns(previous)?previous:"restored";}
 internal static string Decide(bool stop,bool owned,bool selection,bool menu,bool sameOwners,bool valuesOwned,bool originalRestored,bool removed){
  if(owned&&(!sameOwners||!valuesOwned))return "conflict";
  if(owned&&menu)return originalRestored?"reset":"restore";
  if(stop)return owned?"wait-exit":"noop";
  if(!owned)return selection?removed?"unchanged":"apply":"wait";
  if(selection)return removed?"observe":"reapply";
  return removed?"observe":"wait-reset";
 }
}
internal static partial class FreestyleTeams {
 sealed class CrowdRecordJournal {public uint index;public ulong owner;public byte[] original;}
 sealed class CrowdJournal {
  public int schema=3,pid;public long created;public ulong basis,owner;public string image,phase;
  public uint timestamp=BlacktopCrowdPolicy.Timestamp,size=BlacktopCrowdPolicy.ImageSize;
  public byte original,expected;public CrowdRecordJournal[] records;
 }
 sealed class CrowdContext {
  internal uint current,alternate,stage;internal byte active,kind;internal ulong gameOwner;
  internal bool Selection {get{return BlacktopCrowdPolicy.Selection(current,alternate,stage,active,kind);}}
  internal bool MainMenu {get{return BlacktopCrowdPolicy.MainMenu(current,alternate,stage,active,kind);}}
  internal bool Exact(CrowdContext s){return current==s.current&&alternate==s.alternate&&stage==s.stage&&active==s.active&&kind==s.kind&&gameOwner==s.gameOwner;}
 }
 sealed class CrowdSource {
  internal uint index;internal ulong owner;internal byte[] front,ambient;
  internal byte Front {get{return front[BlacktopCrowdPolicy.FrontIndex];}}
  internal bool Exact(CrowdSource s){return index==s.index&&owner==s.owner&&front.SequenceEqual(s.front)&&ambient.SequenceEqual(s.ambient);}
 }
 sealed class CrowdSnapshot {
  internal ulong owner,preloadTable;internal byte[] window;internal CrowdSource[] sources;internal CrowdContext context;
  internal byte Value {get{return window[BlacktopCrowdPolicy.CrowdIndex];}}
  internal bool Removed {get{return Value==0&&sources.All(row=>row.Front==0);}}
  // Loaded GameData mirrors change during initialization and are not owned by
  // this transaction. Check the actual preload/global windows, all neighboring
  // bytes and owner identities instead; context remains a separate write gate.
  internal bool DataExact(CrowdSnapshot s){return owner==s.owner&&preloadTable==s.preloadTable&&window.SequenceEqual(s.window)&&sources.Length==s.sources.Length&&sources.Zip(s.sources,(a,b)=>a.Exact(b)).All(v=>v);}
  internal bool Exact(CrowdSnapshot s){return DataExact(s)&&context.Exact(s.context);}
  internal string Difference(CrowdSnapshot s){
   if(owner!=s.owner)return "global owner";if(preloadTable!=s.preloadTable)return "preload table";
   if(!window.SequenceEqual(s.window))return "global flag window";
   if(sources.Length!=s.sources.Length)return "preset count";
   for(int i=0;i<sources.Length;i++)if(!sources[i].Exact(s.sources[i]))return "preset "+sources[i].index+" owner/flags";
   return context.Exact(s.context)?"none":"selection/menu context";
  }
 }
 static void CrowdSignature(ulong rva,string hex){if(!Read(basis+rva,hex.Length/2).SequenceEqual(BlacktopCrowdPolicy.Hex(hex)))throw new Stop("unsupported","Crowd preload code differs from the verified game build.");}
 static void CrowdBuild(){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664||D(b,pe+8)!=BlacktopCrowdPolicy.Timestamp||D(b,pe+80)!=BlacktopCrowdPolicy.ImageSize)throw new Stop("unsupported","Crowd control does not support this game build.");
  CrowdSignature(BlacktopCrowdPolicy.GetterRva,BlacktopCrowdPolicy.GetterHex);
  CrowdSignature(0x24C8D79,"488B3570226008");CrowdSignature(0x24C8E50,"4889F189FAE836E46801");CrowdSignature(0x3B572A2,"89D64889CF");
  CrowdSignature(0x3B91032,"0FB68729740000A804");CrowdSignature(0x3B910E1,"0FB68F30740000BABE0D9B5341B8AA4C0000E808AEB2FE");CrowdSignature(0x26BC034,"0FB680FD0C0000");
  // Verify the table anchor, native row arithmetic, and native source selection.
  CrowdSignature(0x24C8D45,"488D0D9C226008");CrowdSignature(0x28D8AB5,"4869C910C80000");
  CrowdSignature(0x28D8AD8,"48895608896E10897E14");CrowdSignature(0x28D8906,"4883C10841B86043F200");
  // Stock Blacktop uses match-size index 0..4, not court/day-night, and
  // initializes its preset as 175+index. Never edit any other table records.
  CrowdSignature(0x22DFE30,"8B05DA287C08C3");
  CrowdSignature(0x32BE34D,"E8DE1A02FF89C6");CrowdSignature(0x32BE3A9,"89F1E8700F0000");
  CrowdSignature(0x32BF33B,"89CE");CrowdSignature(0x32BF488,"83FE040F871C010000");
  CrowdSignature(0x32BF569,"8D8EAF000000E8BC7624FF");
  CrowdSignature(0x32C1B85,"E8A6E201FF83F80477138D044502000000");
 }
 static CrowdContext CrowdMode(){
  Identity();var s=new CrowdContext{current=PresentationMode(0xAC77E28),alternate=PresentationMode(0xAC77E24),stage=D(Read(basis+0xAEB69D0+0xEA8,4)),active=Read(basis+0xAACC310,1)[0],gameOwner=Q(Read(basis+0xAACB9A0,8))};
  if(!BlacktopCrowdPolicy.ValidOwner(s.gameOwner))throw new Stop("waiting","Waiting for a stable crowd mode owner.");s.kind=Read(s.gameOwner,1)[0];Identity();return s;
 }
 static CrowdSnapshot CrowdRead(bool owned=true){
  Identity();var s=new CrowdSnapshot{context=CrowdMode(),owner=Q(Read(basis+BlacktopCrowdPolicy.OwnerPointer,8)),preloadTable=Q(Read(basis+BlacktopCrowdPolicy.PreloadTablePointer,8))};
  // The current-row pointer is null before the first match. Prepare only the
  // five stock Blacktop match-size records already loaded in the table; this
  // also covers a size change before the next match. Never edit the manager,
  // current-row pointer, match-size selector, or a non-Blacktop record.
  if(!BlacktopCrowdPolicy.ValidOwner(s.owner)||Read(s.owner,1)[0]!=0)throw new Stop("conflict","Crowd settings owner is unsupported. No crowd write attempted.");
  var rows=new List<CrowdSource>();
  for(uint index=BlacktopCrowdPolicy.FirstBlacktopRecord;index<=BlacktopCrowdPolicy.LastBlacktopRecord;index++){
   ulong at=BlacktopCrowdPolicy.SourceRow(s.preloadTable,index);
   if(!BlacktopCrowdPolicy.ValidOwner(at))throw new Stop("waiting","Waiting for the crowd preload table.");
   uint sourceTag=D(Read(at,4));string sourcePhase=BlacktopCrowdPolicy.SourceTagPhase(owned,index,sourceTag);
   if(sourcePhase!="ready")throw new Stop(sourcePhase,owned?"Crowd preload record changed.":"Waiting for Blacktop crowd preset "+index+" to initialize.");
   Area(at+0x73F8,0x50,0x20000,true);
   var row=new CrowdSource{index=index,owner=at,front=Read(at+BlacktopCrowdPolicy.FrontWindowOffset,BlacktopCrowdPolicy.FrontWindowSize),ambient=Read(at+0x73F8,24)};
   if(!BlacktopCrowdPolicy.ValidFront(row.front)||row.ambient[1]!=0x4D||row.ambient[8]>1)throw new Stop("conflict","Blacktop crowd preset layout changed.");
   rows.Add(row);
  }
  s.sources=rows.ToArray();Area(s.owner+BlacktopCrowdPolicy.WindowOffset,BlacktopCrowdPolicy.WindowSize,0x20000,true);
  s.window=Read(s.owner+BlacktopCrowdPolicy.WindowOffset,BlacktopCrowdPolicy.WindowSize);
  if(s.Value>1)throw new Stop("conflict","Crowd flag changed.");
  if(Q(Read(basis+BlacktopCrowdPolicy.OwnerPointer,8))!=s.owner||Q(Read(basis+BlacktopCrowdPolicy.PreloadTablePointer,8))!=s.preloadTable||!s.context.Exact(CrowdMode()))throw new Stop("waiting","Crowd owners or mode are initializing.");Identity();return s;
 }
 static void CrowdSave(string file,CrowdJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);
  var bytes=System.Text.Encoding.UTF8.GetBytes(json.Serialize(j));using(var stream=new FileStream(temp,FileMode.Create,FileAccess.Write,FileShare.None)){stream.Write(bytes,0,bytes.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static CrowdJournal CrowdLoad(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>8192)throw new Stop("error","Invalid crowd recovery data.");
  string raw=File.ReadAllText(file);var fields=json.DeserializeObject(raw) as IDictionary<string,object>;
  var names=new[]{"schema","pid","created","basis","owner","image","phase","timestamp","size","original","expected"};
  if(fields==null||!names.All(fields.ContainsKey)||!new[]{"schema","pid","created","basis","owner","timestamp","size","original","expected"}.All(k=>fields[k] is int||fields[k] is long)||!(fields["image"] is string)||!(fields["phase"] is string))throw new Stop("error","Invalid crowd recovery fields.");
  var j=json.Deserialize<CrowdJournal>(raw);bool legacy=j.schema==1&&fields.Count==11;
  // Recognize legacy journals for diagnosis only. RunCrowd refuses adoption
  // in the same live game; a game restart clears those memory-only changes.
  bool scoped=j.schema==2&&fields.Count==13&&fields.ContainsKey("sourceOwner")&&(fields["sourceOwner"] is int||fields["sourceOwner"] is long)&&fields.ContainsKey("frontOriginal");
  if(scoped){ulong owner=Convert.ToUInt64(fields["sourceOwner"]);var front=json.Deserialize<byte[]>(json.Serialize(fields["frontOriginal"]));scoped=BlacktopCrowdPolicy.ValidOwner(owner)&&BlacktopCrowdPolicy.ValidFront(front);}
  bool complete=j.schema==3&&fields.Count==12&&fields.ContainsKey("records")&&CrowdRecordsValid(fields["records"],j.records);
  if((!legacy&&!scoped&&!complete)||j.pid<=0||j.created<=0||!BlacktopCrowdPolicy.ValidOwner(j.owner)||j.basis<0x10000||String.IsNullOrWhiteSpace(j.image)||j.original>1||j.expected!=0||j.timestamp!=BlacktopCrowdPolicy.Timestamp||j.size!=BlacktopCrowdPolicy.ImageSize||!new[]{"prepared","partial","applied","restoring","restored","failed"}.Contains(j.phase))throw new Stop("error","Invalid crowd recovery data.");return j;
 }
 static bool CrowdRecordsValid(object raw,CrowdRecordJournal[] records){
  var entries=raw as object[];
  if(entries==null||entries.Length!=5||records==null||records.Length!=5)return false;
  for(int i=0;i<5;i++){
   var fields=entries[i] as IDictionary<string,object>;
   if(fields==null||fields.Count!=3||!new[]{"index","owner","original"}.All(fields.ContainsKey)||!(fields["index"] is int||fields["index"] is long)||!(fields["owner"] is int||fields["owner"] is long))return false;
   if(records[i]==null||records[i].index!=BlacktopCrowdPolicy.FirstBlacktopRecord+i||!BlacktopCrowdPolicy.ValidOwner(records[i].owner)||!BlacktopCrowdPolicy.ValidFront(records[i].original))return false;
   if(i>0&&records[i].owner!=records[i-1].owner+BlacktopCrowdPolicy.RecordStride)return false;
  }return true;
 }
 static object CrowdChange(string file,CrowdJournal j,CrowdSnapshot s,bool restoring){
  DebugEac();Identity();CrowdBuild();Thread.Sleep(250);
  var settled=CrowdRead(restoring||BlacktopCrowdPolicy.Owns(j.phase));
  if(!s.Exact(settled))throw new Stop("waiting","Crowd preparation settling: "+s.Difference(settled)+" changed before writing; no write attempted.");
  string previous=j.phase,step="journal";bool attempted=false;int writes=0;j.phase=restoring?"restoring":"prepared";CrowdSave(file,j);
  try{
   for(int index=0;index<BlacktopCrowdPolicy.MaxWrites;index++){
    byte target=restoring?(index==0?j.original:j.records[index-1].original[BlacktopCrowdPolicy.FrontIndex]):(byte)0;
    if((index==0?s.Value:s.sources[index-1].Front)==target)continue;
    step=index==0?"stand flag":"standing preset "+s.sources[index-1].index;
    DebugEac();Identity();CrowdBuild();var check=CrowdRead();
    if(!s.DataExact(check))throw new Stop("conflict","Crowd "+s.Difference(check)+" changed before "+step+".");
    if(!s.context.Exact(check.context)||(restoring?!check.context.MainMenu:!check.context.Selection))throw new Stop("waiting","Safe selection/menu boundary changed before "+step+"; no gameplay write attempted.");
    ulong at=index==0?s.owner+BlacktopCrowdPolicy.WindowOffset+BlacktopCrowdPolicy.CrowdIndex:s.sources[index-1].owner+BlacktopCrowdPolicy.FrontWindowOffset+BlacktopCrowdPolicy.FrontIndex;
    UIntPtr done;attempted=true;
    if(!WriteProcessMemory(handle,(IntPtr)(long)at,new[]{target},(UIntPtr)1,out done)||done.ToUInt64()!=1)throw new Stop("write-failed","Crowd write could not be verified.");
    if(index==0)s.window[BlacktopCrowdPolicy.CrowdIndex]=target;else s.sources[index-1].front[BlacktopCrowdPolicy.FrontIndex]=target;
    step+=" readback";var after=CrowdRead();
    if(!s.DataExact(after))throw new Stop("write-failed","Crowd "+s.Difference(after)+" readback changed.");writes++;
    if(!s.context.Exact(after.context))throw new Stop("waiting","Safe selection/menu boundary changed after verified "+step+".");
   }
   j.phase=restoring?"restored":"applied";CrowdSave(file,j);
   return new{phase=j.phase,message=restoring?"Original crowd settings restored after leaving Blacktop.":"Stand crowd and all five Blacktop standing-crowd presets prepared for this visit.",writes=writes,scoped=true,ownsChanges=!restoring};
  }catch(Exception error){
   var stop=error as Stop;bool waiting=stop!=null&&stop.phase=="waiting";
   j.phase=BlacktopCrowdPolicy.FailurePhase(waiting,attempted,previous);CrowdSave(file,j);
   if(waiting&&BlacktopCrowdPolicy.Owns(j.phase))return new{phase="waiting",message="Crowd preparation paused at "+step+" after "+writes+" verified writes: "+error.Message+" Originals retained for guarded selection retry/menu restoration.",writes=writes,ownsChanges=true};
   if(attempted)throw new Stop("write-failed","Crowd update stopped at "+step+" after "+writes+" verified writes: "+error.Message+" Restart the game to clear changes; recovery retained.");throw;
  }
 }
 static object RunCrowd(string[] args){
  int pid;long birth;ulong expectedBase;
  if(args.Length!=6||!new[]{"crowd-bind","crowd-tick","crowd-restore"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out birth)||!ulong.TryParse(args[5],out expectedBase))throw new Stop("error","Invalid crowd control request.");
  bool bind=args[0]=="crowd-bind",stop=args[0]=="crowd-restore";
  if(!bind&&(pid<=0||birth<=0||expectedBase<0x10000))throw new Stop("error","Crowd control requires an owned Play session.");
  string file=Path.GetFullPath(args[2]),expectedImage=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  using(var gate=new Mutex(false,"Local\\2kLauncherBlacktopPresentation")){
   bool locked=false;try{
    try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}
    if(!locked)throw new Stop("waiting","Another presentation operation is running; crowd control will wait.");
    var games=Process.GetProcessesByName("NBA2K27");try{
     if(games.Length==0)return new{phase=bind?"waiting":"ended",message=bind?"Waiting for a new Play session.":"Game ended; memory-only crowd changes cleared.",writes=0};
     if(games.Length!=1)throw new Stop("error","Crowd control expected one game process.");var game=games[0];
     string image=Path.GetFullPath(game.MainModule.FileName);created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(!bind&&(game.Id!=pid||created!=birth||basis!=expectedBase))throw new Stop("ended","Owned crowd session ended.");
     if(!String.Equals(image,expectedImage,StringComparison.OrdinalIgnoreCase))throw new Stop("error","Crowd control game folder differs.");
     DebugEac();handle=OpenProcess(bind?0x410U:0x438U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access crowd settings. Match game and launcher permissions.");
     Identity();CrowdBuild();
     if(bind)return new{phase="bound",message="Waiting for Blacktop player selection.",pid=game.Id,created=created.ToString(),basis=basis.ToString(),writes=0};
     var j=CrowdLoad(file);bool same=j!=null&&j.pid==pid&&j.created==birth&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);
     if(same&&(j.schema!=3||(!BlacktopCrowdPolicy.Owns(j.phase)&&j.phase!="restored")))throw new Stop("conflict","Legacy or failed crowd changes require a game restart. No further writes attempted.");
     bool owned=same&&BlacktopCrowdPolicy.Owns(j.phase);var context=CrowdMode();
     if(!owned&&(stop||!context.Selection))return new{phase=stop?"restored":"waiting",message=stop?"No owned crowd changes to restore.":"Waiting for Blacktop player selection.",writes=0};
     // Do not read transient crowd owners or write anything during load/play.
     if(owned&&!context.Selection&&!context.MainMenu)return new{phase=stop||j.phase=="partial"?"waiting":"applied",message=stop?"Crowd restoration waits until Blacktop ends at the main menu.":j.phase=="partial"?"Crowd preparation is incomplete; waiting for selection or the safe menu to restore. No gameplay writes.":"Crowd removal retained for this Blacktop visit.",writes=0,ownsChanges=true};
     var s=CrowdRead(owned);bool sameOwners=!owned||(j.owner==s.owner&&s.sources.Zip(j.records,(a,b)=>a.index==b.index&&a.owner==b.owner).All(v=>v));
     bool valuesOwned=!owned||s.sources.Zip(j.records,(a,b)=>BlacktopCrowdPolicy.OwnedValues(j.original,s.Value,b.original,a.front)).All(v=>v);
     bool originalRestored=owned&&s.Value==j.original&&s.sources.Zip(j.records,(a,b)=>a.front.SequenceEqual(b.original)).All(v=>v);
     string decision=BlacktopCrowdPolicy.Decide(stop,owned,s.context.Selection,s.context.MainMenu,sameOwners,valuesOwned,originalRestored,s.Removed);
     if(decision=="conflict")throw new Stop("conflict","Crowd owners or owned settings changed outside the launcher. No further writes attempted.");
     if(decision=="wait"||decision=="wait-reset"||decision=="wait-exit")return new{phase="waiting",message=decision=="wait-exit"?"Crowd restoration waits until Blacktop ends at the main menu.":"Waiting for verified Blacktop player selection.",writes=0};
     if(decision=="observe"){if(j.phase=="partial"){j.phase="applied";CrowdSave(file,j);}return new{phase="applied",message="Crowd removal retained for this Blacktop visit.",writes=0,ownsChanges=true};}
     if(decision=="unchanged")return new{phase="unchanged",message="Both crowd settings were already off; no change owned.",writes=0};
     if(decision=="noop")return new{phase="restored",message="No owned crowd changes to restore.",writes=0};
     if(decision=="reset"){j.phase="restored";CrowdSave(file,j);return new{phase="restored",message="The game already restored the original crowd settings.",writes=0};}
     bool restoring=decision=="restore";
     if(!owned)j=new CrowdJournal{pid=pid,created=birth,basis=basis,image=image,owner=s.owner,original=s.Value,records=s.sources.Select(row=>new CrowdRecordJournal{index=row.index,owner=row.owner,original=(byte[])row.front.Clone()}).ToArray(),expected=0};
     return CrowdChange(file,j,s,restoring);
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var p in games)p.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
}
