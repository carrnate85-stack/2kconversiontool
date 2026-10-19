using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Collections.Generic;

internal static class BallSelectorPolicy {
 internal const uint HookRva=0x4138B79;
 internal const uint RegistryRva=0xAA81400;
 internal const uint RecordHolderRva=0xAA51F68;
 internal const uint RecordOffset=0xF4D28;
 internal const int RecordStride=0x1A0;
 internal const int RecordAssetOffset=0x9C;
 internal const uint ModeTableRva=0xAC77660;
 internal const uint ModeCurrentRva=0xAC77E28;
 internal const uint ModeAlternateRva=0xAC77E24;
 internal const uint BlacktopStageRva=0xAEB69D0+0xEA8;
 internal const uint SettingsRva=0xAACB9A0;
 internal const uint GameModeOffset=0xA04;
 internal static bool ActivationDisabled(string action){return action=="ball-bind"||action=="ball-select"||action=="ball-current"||action=="ball-observe";}
 internal const uint RendererRva=0xAA7B078;
 internal const uint LoadHookRva=0x2E40B6,ReleaseHookRva=0x2E4444;
 internal const uint LoadFunctionRva=0x3045C50,ReleaseFunctionRva=0x30462B0;
 internal static byte[] Call(ulong at,ulong target){return Patch(at,target).Take(5).ToArray();}
 internal static readonly byte[] Original={0x41,0x8B,0x8E,0x4C,0x03,0x00,0x00};
 internal static string Scope(uint current,uint alternate,uint stage,uint gameMode){
  if(current!=alternate)return "transition";
  if(stage==3&&current==0x92DF06DEU)return "selection";
  if(stage==2)return "transition";
  // Gameplay and pause hashes are shared by Freestyle and Blacktop. The
  // Blacktop stage can remain 1 after exit, so it cannot establish ownership.
  if(gameMode==uint.MaxValue)return "transition";
  if(gameMode!=6)return "outside";
  if(stage==1&&new uint[]{0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898}.Contains(current))return "match";
  if(stage==1||stage==2)return "transition";
  return "outside";
 }
 internal static byte[] Patch(ulong hook,ulong stub){
  long delta=(long)stub-(long)(hook+5);if(delta<int.MinValue||delta>int.MaxValue)throw new InvalidOperationException("Ball selector bridge is too far from the game code.");
  return new[]{(byte)0xE8}.Concat(BitConverter.GetBytes((int)delta)).Concat(new byte[]{0x90,0x90}).ToArray();
 }
 internal static byte[] Stub(ulong control,ulong basis,bool requireGameMode=true,bool requireReady=true){
  var b=new List<byte>();var labels=new Dictionary<string,int>();var fixups=new List<Tuple<int,string>>();
  Action<byte[]> add=value=>b.AddRange(value);Action<string> label=name=>labels[name]=b.Count;
  Action<byte[],string> jump=(opcode,target)=>{add(opcode);int at=b.Count;add(new byte[4]);fixups.Add(Tuple.Create(at,target));};
  add(Original);add(new byte[]{0x50,0x52});
  add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(basis+ModeCurrentRva));add(new byte[]{0x8B,0x10,0x81,0xFA,0x80,0,0,0});jump(new byte[]{0x0F,0x83},"done");
  add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(basis+ModeAlternateRva));add(new byte[]{0x8B,0x00,0x39,0xD0});jump(new byte[]{0x0F,0x85},"done");
  add(new byte[]{0x48,0xC1,0xE0,0x06,0x48,0xBA});add(BitConverter.GetBytes(basis+ModeTableRva));add(new byte[]{0x8B,0x14,0x02});
  add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(basis+BlacktopStageRva));add(new byte[]{0x8B,0x00,0x83,0xF8,0x03});jump(new byte[]{0x0F,0x85},"match");
  add(new byte[]{0x81,0xFA});add(BitConverter.GetBytes(0x92DF06DEU));jump(new byte[]{0x0F,0x84},"apply");jump(new byte[]{0xE9},"done");
  label("match");add(new byte[]{0x83,0xF8,0x01});jump(new byte[]{0x0F,0x85},"done");
  if(requireGameMode){
   add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(basis+SettingsRva));add(new byte[]{0x48,0x8B,0x00,0x48,0x3D,0x00,0x00,0x01,0x00});jump(new byte[]{0x0F,0x82},"done");
   add(new byte[]{0x83,0xB8});add(BitConverter.GetBytes(GameModeOffset));add(new byte[]{0x06});jump(new byte[]{0x0F,0x85},"done");
  }
  foreach(uint hash in new uint[]{0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898}){add(new byte[]{0x81,0xFA});add(BitConverter.GetBytes(hash));jump(new byte[]{0x0F,0x84},"apply");}
  jump(new byte[]{0xE9},"done");label("apply");add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(control));
  if(requireReady){
   // One aligned snapshot: low DWORD is model ID; high DWORD is registry index.
   // Never replace the native ball with a catalog-only, unloaded resource.
   add(new byte[]{0x48,0x8B,0x10,0x85,0xD2});jump(new byte[]{0x0F,0x84},"done");
   add(new byte[]{0x48,0x89,0xD0,0x48,0xC1,0xE8,0x20,0x3D});add(BitConverter.GetBytes(500));jump(new byte[]{0x0F,0x83},"done");
   add(new byte[]{0x48,0x6B,0xC0,0x30,0x52,0x48,0xBA});add(BitConverter.GetBytes(basis+RendererRva));
   add(new byte[]{0x48,0x01,0xD0,0x5A,0x48,0x83,0x78,0x38,0x00});jump(new byte[]{0x0F,0x84},"done");
   add(new byte[]{0x89,0xD1});
  }else add(new byte[]{0x8B,0x00,0x85,0xC0,0x0F,0x45,0xC8});
  label("done");add(new byte[]{0x5A,0x58,0xC3});
  foreach(var fixup in fixups){int delta=labels[fixup.Item2]-(fixup.Item1+4);byte[] bytes=BitConverter.GetBytes(delta);for(int i=0;i<4;i++)b[fixup.Item1+i]=bytes[i];}
  return b.ToArray();
 }
 // Runs only at the game's existing load/release callsites, on its own thread.
 // Does not change settings, AI_BALL identity, files, or the normal ball's lifetime.
 internal static byte[] LifetimeStub(ulong control,ulong basis,bool release){
  var b=new List<byte>();var labels=new Dictionary<string,int>();var fixups=new List<Tuple<int,string>>();
  Action<byte[]> add=v=>b.AddRange(v);Action<string> label=n=>labels[n]=b.Count;
  Action<byte[],string> jump=(op,n)=>{add(op);int at=b.Count;add(new byte[4]);fixups.Add(Tuple.Create(at,n));};
  Action<ulong> target=at=>{add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(at));};
  // 32-byte shadow space, saved arguments and native return value; 16-byte alignment.
  add(new byte[]{0x48,0x83,0xEC,0x58,0x48,0x89,0x4C,0x24,0x20,0x48,0x89,0x54,0x24,0x28,0x4C,0x89,0x44,0x24,0x30});
  target(basis+(release?ReleaseFunctionRva:LoadFunctionRva));add(new byte[]{0xFF,0xD0,0x48,0x89,0x44,0x24,0x38});
  target(control);
  if(release){add(new byte[]{0x8B,0x48,0x08,0x85,0xC9});jump(new byte[]{0x0F,0x84},"done");}
  else{
   add(new byte[]{0x83,0x78,0x08,0x00});jump(new byte[]{0x0F,0x85},"done");
   add(new byte[]{0x8B,0x08,0x85,0xC9});jump(new byte[]{0x0F,0x84},"done");
   add(new byte[]{0x3B,0x4C,0x24,0x20});jump(new byte[]{0x0F,0x84},"done");
   target(basis+SettingsRva);add(new byte[]{0x48,0x8B,0x00,0x48,0x3D,0,0,1,0});jump(new byte[]{0x0F,0x82},"done");
   add(new byte[]{0x83,0xB8});add(BitConverter.GetBytes(GameModeOffset));add(new byte[]{6});jump(new byte[]{0x0F,0x85},"done");
   target(basis+BlacktopStageRva);add(new byte[]{0x8B,0x00,0x83,0xE8,0x01,0x83,0xF8,0x02});jump(new byte[]{0x0F,0x87},"done");
   target(control);add(new byte[]{0x89,0x48,0x08});
  }
  add(new byte[]{0x48,0x8B,0x54,0x24,0x28,0x4C,0x8B,0x44,0x24,0x30});
  target(basis+(release?ReleaseFunctionRva:LoadFunctionRva));add(new byte[]{0xFF,0xD0});
  if(release){target(control);add(new byte[]{0xC7,0x40,0x08,0,0,0,0});}
  label("done");add(new byte[]{0x48,0x8B,0x44,0x24,0x38,0x48,0x83,0xC4,0x58,0xC3});
  foreach(var f in fixups){byte[] bytes=BitConverter.GetBytes(labels[f.Item2]-(f.Item1+4));for(int i=0;i<4;i++)b[f.Item1+i]=bytes[i];}
  return b.ToArray();
 }
}

internal static partial class FreestyleTeams {
 sealed class BallJournal {public int schema=2,pid;public long created;public ulong basis,stub,control;public string image,phase,logicalPath;public uint modelId,timestamp=0x6AA2013D,size=0x36558000;public byte[] original,patch,stubBytes;}
 static byte[] BallCode(ulong control,ulong image){
  var code=new byte[0xC00];var render=BallSelectorPolicy.Stub(control,image);var load=BallSelectorPolicy.LifetimeStub(control,image,false);var release=BallSelectorPolicy.LifetimeStub(control,image,true);
  if(render.Length>0x400||load.Length>0x400||release.Length>0x400)throw new InvalidOperationException("Ball bridge exceeds its bounded code slot.");
  Array.Copy(render,0,code,0,render.Length);Array.Copy(load,0,code,0x400,load.Length);Array.Copy(release,0,code,0x800,release.Length);return code;
 }
 static IEnumerable<Tuple<uint,byte[],byte[]>> BallLifetimeSites(BallJournal j){
  // Release is installed first so a published load always has its matching release.
  foreach(var site in new[]{Tuple.Create(BallSelectorPolicy.ReleaseHookRva,BallSelectorPolicy.ReleaseFunctionRva,0x800UL),Tuple.Create(BallSelectorPolicy.LoadHookRva,BallSelectorPolicy.LoadFunctionRva,0x400UL)})
   yield return Tuple.Create(site.Item1,BallSelectorPolicy.Call(basis+site.Item1,basis+site.Item2),BallSelectorPolicy.Call(basis+site.Item1,j.stub+site.Item3));
 }
 static void BallEac(){foreach(var p in Process.GetProcesses())using(p){if(p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase)||p.ProcessName.Equals("start_protected_game",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Ball selection is unavailable in a protected game session. No game code was changed.");}}
 static void BallBuild(BallJournal j=null){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664||D(b,pe+8)!=0x6AA2013D||D(b,pe+80)!=0x36558000)throw new Stop("unsupported","Ball selection does not support this game update.");
  var code=Read(basis+BallSelectorPolicy.HookRva,BallSelectorPolicy.Original.Length);
  if(!code.SequenceEqual(BallSelectorPolicy.Original)&&(j==null||j.patch==null||!code.SequenceEqual(j.patch)))throw new Stop("unsupported","The ball-render code differs from the supported build.");
  var check=j??new BallJournal{stub=basis+0x30000000};
  foreach(var site in BallLifetimeSites(check)){
   byte[] live=Read(basis+site.Item1,5);if(!live.SequenceEqual(site.Item2)&&(j==null||j.schema!=2||!live.SequenceEqual(site.Item3)))throw new Stop("unsupported","The native ball-loading code differs from the supported build.");
  }
 }
 static void BallSave(string file,BallJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);if(File.Exists(temp))File.Delete(temp);
  byte[] bytes=Encoding.UTF8.GetBytes(json.Serialize(j));using(var stream=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){stream.Write(bytes,0,bytes.Length);stream.Flush(true);}if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static BallJournal BallLoad(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>16384)throw new Stop("error","Invalid ball-selector recovery data.");var j=json.Deserialize<BallJournal>(File.ReadAllText(file));
  if(j==null||(j.schema!=1&&j.schema!=2)||j.original==null||j.patch==null||j.stubBytes==null||!j.original.SequenceEqual(BallSelectorPolicy.Original)||j.patch.Length!=7||j.stub<0x10000||j.control!=j.stub+0x1000||j.timestamp!=0x6AA2013D||j.size!=0x36558000)throw new Stop("error","Invalid ball-selector recovery data.");
  // Recognize the previous stub only so an upgrade can safely restore it.
  bool validCode=j.schema==2?j.stubBytes.SequenceEqual(BallCode(j.control,j.basis)):
   j.stubBytes.SequenceEqual(BallSelectorPolicy.Stub(j.control,j.basis,true,false))||j.stubBytes.SequenceEqual(BallSelectorPolicy.Stub(j.control,j.basis,false,false));
  if(!j.patch.SequenceEqual(BallSelectorPolicy.Patch(j.basis+BallSelectorPolicy.HookRva,j.stub))||!validCode)throw new Stop("error","Invalid ball-selector recovery code.");return j;
 }
 static bool BallSame(BallJournal j,int pid,string image){return j!=null&&j.pid==pid&&j.created==created&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);}
 static void BallWrite(ulong at,byte[] bytes,bool executable){
  uint old=0;if(executable&&!VirtualProtectEx(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length,0x40,out old))throw new Stop("write-failed","Ball selector could not protect its exact code range.");
  try{UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)at,bytes,(UIntPtr)bytes.Length,out done)||done.ToUInt64()!=(ulong)bytes.Length||!Read(at,bytes.Length).SequenceEqual(bytes)||(executable&&!FlushInstructionCache(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length)))throw new Stop("write-failed","Ball selection update could not be verified.");}
  finally{if(executable&&old!=0){uint ignored;VirtualProtectEx(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length,old,out ignored);}}
 }
 static ulong BallAllocate(){
  foreach(long offset in new long[]{0x30000000,0x40000000,0x50000000,0x60000000,-0x10000000,-0x20000000}){
   long candidate=((long)basis+offset)&~0xffffL;if(candidate<0x10000)continue;IntPtr p=VirtualAllocEx(handle,(IntPtr)candidate,(UIntPtr)0x2000,0x3000,0x40);if(p==IntPtr.Zero)continue;
   ulong value=(ulong)p.ToInt64();try{BallSelectorPolicy.Patch(basis+BallSelectorPolicy.HookRva,value);return value;}catch{VirtualFreeEx(handle,p,UIntPtr.Zero,0x8000);}
  }throw new Stop("waiting","Waiting for a safe nearby ball-selector code slot.");
 }
 static uint BallResolve(string logicalPath){uint index;return BallResolve(logicalPath,out index);}
 static uint BallResolve(string logicalPath,out uint foundIndex){
  foundIndex=uint.MaxValue;
  string logical=(logicalPath??"").Replace('\\','/').ToLowerInvariant();if(!Regex.IsMatch(logical,@"^balls/ball_[a-z0-9_.-]+\.iff$"))throw new Stop("error","Invalid stock ball selection.");string wanted=Path.GetFileName(logical);
  ulong table=Q(Read(basis+BallSelectorPolicy.RegistryRva,8));int count=(int)D(Read(basis+BallSelectorPolicy.RegistryRva+8,4));if(count<1||count>1024||table<0x10000)throw new Stop("waiting","Waiting for the stock ball registry.");
  ulong holder=Q(Read(basis+BallSelectorPolicy.RecordHolderRva,8));if(holder<0x10000)throw new Stop("waiting","Waiting for stock ball records.");ulong records=Q(Read(holder+0x18,8))+BallSelectorPolicy.RecordOffset;if(records<0x10000)throw new Stop("waiting","Waiting for stock ball records.");
  byte[] entries=Read(table,count*8);uint found=0;int matches=0;
  for(int i=0;i<count;i++){
   uint model=D(entries,i*8),index=D(entries,i*8+4);if(index>=500)continue;byte[] record;
   try{record=Read(records+(ulong)index*BallSelectorPolicy.RecordStride,BallSelectorPolicy.RecordStride);}catch(Stop){continue;}
   if(D(record,4)!=model)continue;int end=BallSelectorPolicy.RecordAssetOffset;while(end<record.Length&&record[end]!=0)end++;if(end==BallSelectorPolicy.RecordAssetOffset)continue;
   string asset=Encoding.ASCII.GetString(record,BallSelectorPolicy.RecordAssetOffset,end-BallSelectorPolicy.RecordAssetOffset).Replace('\\','/').ToLowerInvariant();if(Path.GetFileName(asset)!=wanted)continue;found=model;foundIndex=index;matches++;
  }
  if(matches==0)throw new Stop("unsupported","This stock ball is not available in the live game registry.");if(matches!=1||found==0)throw new Stop("conflict","The selected ball does not have one unique live model.");return found;
 }
 static string BallScope(){
  uint currentIndex=D(Read(basis+BallSelectorPolicy.ModeCurrentRva,4)),alternateIndex=D(Read(basis+BallSelectorPolicy.ModeAlternateRva,4));
  if(currentIndex>=128||alternateIndex>=128)return "transition";
  uint current=D(Read(basis+BallSelectorPolicy.ModeTableRva+(ulong)currentIndex*64,4)),alternate=D(Read(basis+BallSelectorPolicy.ModeTableRva+(ulong)alternateIndex*64,4));
  uint stage=D(Read(basis+BallSelectorPolicy.BlacktopStageRva,4));
  ulong settings=Q(Read(basis+BallSelectorPolicy.SettingsRva,8));
  uint gameMode=settings>=0x10000?D(Read(settings+BallSelectorPolicy.GameModeOffset,4)):uint.MaxValue;
  if(currentIndex!=D(Read(basis+BallSelectorPolicy.ModeCurrentRva,4))||alternateIndex!=D(Read(basis+BallSelectorPolicy.ModeAlternateRva,4))||stage!=D(Read(basis+BallSelectorPolicy.BlacktopStageRva,4))||settings!=Q(Read(basis+BallSelectorPolicy.SettingsRva,8))||(settings>=0x10000&&gameMode!=D(Read(settings+BallSelectorPolicy.GameModeOffset,4))))return "transition";
  return BallSelectorPolicy.Scope(current,alternate,stage,gameMode);
 }
 static object BallRestoreOwned(string file,BallJournal j,bool removeJournal=true){
  if(j.schema==2){
   BallBuild(j);if(!Read(j.stub,j.stubBytes.Length).SequenceEqual(j.stubBytes))throw new Stop("conflict","Ball selector code changed outside this launcher. Restart the game before retrying.");
   BallWrite(j.control,new byte[8],false);
   // Keep paired native lifetime hooks until process exit. Removing them while
   // an engine-owned request exists could strand its reference or race teardown.
   // With desired=0 they pass through; release drains only our captured model ID.
   if(j.phase=="applied"){
    if(!Read(basis+BallSelectorPolicy.HookRva,7).SequenceEqual(j.patch)||BallLifetimeSites(j).Any(s=>!Read(basis+s.Item1,5).SequenceEqual(s.Item3)))throw new Stop("conflict","A ball-loading hook changed outside this launcher. Restart the game.");
    j.modelId=0;j.logicalPath="current";BallSave(file,j);return new{phase="restored",message="Native ball behavior is restored; the game retains ownership of pending asset cleanup.",writes=1};
   }
   if(D(Read(j.control+8,4))!=0)throw new Stop("conflict","An interrupted ball load is awaiting game cleanup. Close the game before retrying.");
   foreach(var site in BallLifetimeSites(j)){var live=Read(basis+site.Item1,5);if(live.SequenceEqual(site.Item3))BallWrite(basis+site.Item1,site.Item2,true);else if(!live.SequenceEqual(site.Item2))throw new Stop("conflict","Ball-loading code changed outside this launcher.");}
  }
  ulong hook=basis+BallSelectorPolicy.HookRva;var code=Read(hook,7);
  if(code.SequenceEqual(j.patch)){if(!Read(j.stub,j.stubBytes.Length).SequenceEqual(j.stubBytes))throw new Stop("conflict","Ball selector code changed outside this launcher. Restart the game before retrying.");BallWrite(hook,j.original,true);}
  else if(!code.SequenceEqual(j.original))throw new Stop("conflict","Ball-render code changed outside this launcher. Restart the game before retrying.");
  // An in-flight game thread may still be returning through this bridge.
  // Retain the published 8 KB allocation until process exit after unpatching.
  if(removeJournal&&File.Exists(file))File.Delete(file);return new{phase="restored",message="Native ball behavior is restored.",writes=1};
 }
 static object BallStatus(BallJournal j,int writes){
  ulong choice=Q(Read(j.control,8));uint model=(uint)choice,index=(uint)(choice>>32);string scope=BallScope();
  if(model==0)return new{phase="restored",message="Native ball behavior is active.",writes=writes,scope=scope};
  bool ready=index<500&&Q(Read(basis+BallSelectorPolicy.RendererRva+0x38+(ulong)index*48,8))!=0;
  uint requested=D(Read(j.control+8,4));
  return new{phase=ready?"applied":"loading",message=ready?"Selected ball resource is ready for Blacktop.":requested==model?"Waiting for the game's selected-ball load; the normal ball remains active until ready.":"Selected ball will load with the next Blacktop match; the normal ball remains active until ready.",writes=writes,scope=scope,modelId=model.ToString("X8"),requestedModelId=requested.ToString("X8")};
 }
 static object RunBall(string[] args){
  int pid;long expectedCreated;ulong expectedBase;if(args.Length!=7||!new[]{"ball-bind","ball-select","ball-current","ball-observe","ball-restore","ball-recover"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out expectedCreated)||!ulong.TryParse(args[5],out expectedBase))throw new Stop("error","Invalid ball-selector request.");
  // Fail before opening the game, reading its journal, or allocating/writing.
  // Legacy bridge generation remains solely for exact recovery validation.
  if(BallSelectorPolicy.ActivationDisabled(args[0]))throw new Stop("unsupported","Custom ball switching is temporarily disabled after a player-loading failure. Use the current installed ball.");
  string action=args[0],file=Path.GetFullPath(args[2]),logical=args[6],expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));bool write=new[]{"ball-select","ball-current","ball-restore","ball-recover"}.Contains(action);
  using(var gate=new Mutex(false,"Local\\2kLauncherBallSelector")){bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","Another ball-selector operation is running.");
   var games=Process.GetProcessesByName("NBA2K27");try{
    var j=BallLoad(file);if(games.Length==0){if(j!=null&&File.Exists(file))File.Delete(file);return new{phase="no-game",message="Waiting for NBA 2K27.",writes=0};}
    if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");var game=games[0];string image=Path.GetFullPath(game.MainModule.FileName);if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the configured folder.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();if(action!="ball-bind"&&action!="ball-recover"&&(pid!=game.Id||expectedCreated!=created||expectedBase!=basis))throw new Stop("ended","The owned ball-selector session ended.");
    if(write)BallEac();handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access live ball selection. Game and launcher must use the same permission level.");Identity();bool same=BallSame(j,game.Id,image);
    if(action=="ball-recover"){
     if(j==null)return new{phase="restored",message="No ball-selector recovery was needed.",writes=0};if(!same)throw new Stop("conflict","A ball-selector recovery belongs to another running game. Close the game before retrying.");return BallRestoreOwned(file,j);
    }
    if(action=="ball-bind"){if(j!=null&&(!same||j.schema!=2||j.phase!="applied"||D(Read(j.control,4))!=0))throw new Stop("conflict","Ball-selector recovery is pending. Restart the launcher before playing.");BallBuild(j);return new{phase="bound",message="Live Blacktop ball selection is ready.",pid=game.Id,created=created.ToString(),basis=basis.ToString(),writes=0};}
    if(j!=null&&!same)throw new Stop("ended","The owned ball-selector session ended.");
    if(action=="ball-restore"){if(j==null)return new{phase="restored",message="Native ball behavior is restored.",writes=0};return BallRestoreOwned(file,j);}
    if(j==null){if(action!="ball-select")return new{phase="restored",message="Native ball behavior is active.",writes=0};BallBuild();if(BallScope()!="selection")throw new Stop("waiting","Return to Blacktop player selection to prepare this ball.");uint index;uint model=BallResolve(logical,out index);ulong stub=BallAllocate(),control=stub+0x1000;byte[] stubBytes=BallCode(control,basis),patch=BallSelectorPolicy.Patch(basis+BallSelectorPolicy.HookRva,stub);bool hookAttempted=false;
     try{BallWrite(stub,stubBytes,false);BallWrite(control,new byte[16],false);uint ignored;if(!VirtualProtectEx(handle,(IntPtr)(long)stub,(UIntPtr)0x1000,0x20,out ignored)||!VirtualProtectEx(handle,(IntPtr)(long)control,(UIntPtr)0x1000,0x04,out ignored))throw new Stop("write-failed","Ball selector memory protection could not be finalized.");j=new BallJournal{pid=game.Id,created=created,basis=basis,image=image,stub=stub,control=control,original=BallSelectorPolicy.Original,patch=patch,stubBytes=stubBytes,modelId=model,logicalPath=logical,phase="prepared"};BallSave(file,j);BallEac();Identity();BallBuild();hookAttempted=true;foreach(var site in BallLifetimeSites(j))BallWrite(basis+site.Item1,site.Item3,true);BallWrite(basis+BallSelectorPolicy.HookRva,patch,true);j.phase="applied";BallSave(file,j);BallWrite(control,BitConverter.GetBytes(((ulong)index<<32)|model),false);return BallStatus(j,6);}
     catch{if(!hookAttempted)VirtualFreeEx(handle,(IntPtr)(long)stub,UIntPtr.Zero,0x8000);throw;}
    }
    if(j.phase!="applied")throw new Stop("conflict","An earlier ball selection was interrupted. Restart the game before retrying.");BallBuild(j);if(!Read(j.stub,j.stubBytes.Length).SequenceEqual(j.stubBytes))throw new Stop("conflict","Ball selector code changed outside this launcher. Restart the game before retrying.");
    if(j.schema!=2)throw new Stop("conflict","This ball override belongs to an older launcher. Restart the game to use native ball loading.");
    if(!Read(basis+BallSelectorPolicy.HookRva,7).SequenceEqual(j.patch)||BallLifetimeSites(j).Any(s=>!Read(basis+s.Item1,5).SequenceEqual(s.Item3)))throw new Stop("conflict","A ball-loading hook changed outside this launcher. Restart the game.");
    if(action=="ball-observe")return BallStatus(j,0);
    uint nextIndex=0;uint next=action=="ball-current"?0:BallResolve(logical,out nextIndex);BallWrite(j.control,BitConverter.GetBytes(((ulong)nextIndex<<32)|next),false);j.modelId=next;j.logicalPath=action=="ball-current"?"current":logical;BallSave(file,j);return BallStatus(j,1);
   }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var game in games)game.Dispose();}
  }finally{if(locked)gate.ReleaseMutex();}}
 }
}
