using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

internal static class GameRulesPolicy {
 internal const uint HookRva=0x3913DA2;
 internal const uint ContinueRva=0x3913DAE;
 internal const uint IncludeRva=0x3913D50;
 internal const uint ItemId=0xB79AF8BA;
 internal static readonly byte[] Original={0x84,0xC0,0x75,0xAA,0x42,0x8B,0x84,0x3B,0x90,0x00,0x00,0x00};
 internal static byte[] RelativeJump(ulong from,ulong to){long delta=(long)to-(long)(from+5);if(delta<int.MinValue||delta>int.MaxValue)throw new InvalidOperationException("Game Rules trampoline is too far from the game code.");return new[]{(byte)0xE9}.Concat(BitConverter.GetBytes((int)delta)).ToArray();}
 internal static byte[] Patch(ulong hook,ulong stub){return RelativeJump(hook,stub).Concat(Enumerable.Repeat((byte)0x90,Original.Length-5)).ToArray();}
 internal static byte[] Stub(ulong at,ulong basis){
  var b=new byte[32];int p=0;
  Action<byte[]> add=value=>{Buffer.BlockCopy(value,0,b,p,value.Length);p+=value.Length;};
  add(new byte[]{0x84,0xC0,0x0F,0x85});int first=p;add(new byte[4]);
  add(new byte[]{0x42,0x8B,0x84,0x3B,0x90,0x00,0x00,0x00,0x3D});add(BitConverter.GetBytes(ItemId));
  add(new byte[]{0x0F,0x84});int second=p;add(new byte[4]);
  ulong tail=at+(ulong)p;add(RelativeJump(tail,basis+ContinueRva));
  ulong include=basis+IncludeRva;
  Buffer.BlockCopy(BitConverter.GetBytes(checked((int)((long)include-(long)(at+(ulong)first+4)))),0,b,first,4);
  Buffer.BlockCopy(BitConverter.GetBytes(checked((int)((long)include-(long)(at+(ulong)second+4)))),0,b,second,4);
  if(p!=b.Length)throw new InvalidOperationException("Unexpected Game Rules trampoline size.");return b;
 }
}

internal static partial class FreestyleTeams {
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool VirtualFreeEx(IntPtr process,IntPtr address,UIntPtr size,uint freeType);
 sealed class GameRulesJournal {public int schema=1,pid;public long created;public ulong basis,stub;public string image,phase;public uint timestamp=0x6AA2013D,size=0x36558000;public byte[] original,patch,stubBytes;}
 static void GameRulesBuild(){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664||D(b,pe+8)!=0x6AA2013D||D(b,pe+80)!=0x36558000)throw new Stop("unsupported","Game Rules does not support this game update.");
  if(!Read(basis+GameRulesPolicy.HookRva,GameRulesPolicy.Original.Length).SequenceEqual(GameRulesPolicy.Original))throw new Stop("unsupported","The Game Rules menu code differs from the supported build.");
 }
 static void GameRulesEac(){foreach(var p in Process.GetProcesses())using(p){if(p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase)||p.ProcessName.Equals("start_protected_game",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Game Rules requires offline/non-EAC Play. No menu code was changed.");}}
 static void GameRulesSave(string file,GameRulesJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);if(File.Exists(temp))File.Delete(temp);
  byte[] bytes=System.Text.Encoding.UTF8.GetBytes(json.Serialize(j));using(var stream=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){stream.Write(bytes,0,bytes.Length);stream.Flush(true);}if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static GameRulesJournal GameRulesLoad(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>16384)throw new Stop("error","Invalid Game Rules recovery data.");var j=json.Deserialize<GameRulesJournal>(File.ReadAllText(file));
  if(j==null||j.schema!=1||j.original==null||j.patch==null||j.stubBytes==null||!j.original.SequenceEqual(GameRulesPolicy.Original)||j.patch.Length!=GameRulesPolicy.Original.Length||j.stubBytes.Length!=32||j.timestamp!=0x6AA2013D||j.size!=0x36558000)throw new Stop("error","Invalid Game Rules recovery data.");return j;
 }
 static void GameRulesWrite(ulong at,byte[] bytes,bool executable){
  uint old=0;if(executable&&!VirtualProtectEx(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length,0x40,out old))throw new Stop("write-failed","Game Rules could not protect its exact code range.");
  try{UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)at,bytes,(UIntPtr)bytes.Length,out done)||done.ToUInt64()!=(ulong)bytes.Length||!Read(at,bytes.Length).SequenceEqual(bytes)||!FlushInstructionCache(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length))throw new Stop("write-failed","Game Rules code update could not be verified.");}
  finally{if(executable&&old!=0){uint ignored;VirtualProtectEx(handle,(IntPtr)(long)at,(UIntPtr)bytes.Length,old,out ignored);}}
 }
 static ulong GameRulesAllocate(){
  // Explicit nearby hints keep all rel32 branches inside their signed 32-bit range.
  foreach(long offset in new long[]{0x40000000,0x50000000,0x60000000,0x70000000,-0x10000000,-0x20000000}){
   long candidate=(long)basis+offset;if(candidate<0x10000)continue;IntPtr p=VirtualAllocEx(handle,(IntPtr)candidate,(UIntPtr)0x1000,0x3000,0x40);if(p!=IntPtr.Zero){ulong value=(ulong)p.ToInt64();try{GameRulesPolicy.RelativeJump(basis+GameRulesPolicy.HookRva,value);GameRulesPolicy.RelativeJump(value+27,basis+GameRulesPolicy.ContinueRva);return value;}catch{VirtualFreeEx(handle,p,UIntPtr.Zero,0x8000);}}
  }throw new Stop("waiting","Waiting for a safe nearby Game Rules code slot.");
 }
 static bool GameRulesSame(GameRulesJournal j,int pid,string image){return j!=null&&j.pid==pid&&j.created==created&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);}
 static object RunGameRules(string[] args){
  int pid;long expectedCreated;ulong expectedBase;if(args.Length!=6||!new[]{"game-rules-bind","game-rules-apply","game-rules-observe","game-rules-restore"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out expectedCreated)||!ulong.TryParse(args[5],out expectedBase))throw new Stop("error","Invalid Game Rules request.");
  string action=args[0],file=Path.GetFullPath(args[2]),expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));bool write=action=="game-rules-apply"||action=="game-rules-restore";
  using(var gate=new Mutex(false,"Local\\2kLauncherGameRules")){bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","Another Game Rules operation is running.");
   var games=Process.GetProcessesByName("NBA2K27");try{
    var j=GameRulesLoad(file);if(games.Length==0){if(j!=null&&j.phase!="ended"){j.phase="ended";GameRulesSave(file,j);}return new{phase="no-game",message="Waiting for NBA 2K27.",writes=0};}
    if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");var game=games[0];string image=Path.GetFullPath(game.MainModule.FileName);if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the configured folder.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();if(action!="game-rules-bind"&&(pid!=game.Id||expectedCreated!=created||expectedBase!=basis))throw new Stop("ended","The owned Game Rules session ended.");
    GameRulesEac();handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access Game Rules. Game and launcher must use the same permission level.");Identity();
    if(action=="game-rules-bind"){GameRulesBuild();return new{phase="bound",message="Preparing the stock Game Rules menu.",pid=game.Id,created=created.ToString(),basis=basis.ToString(),writes=0};}
    bool same=GameRulesSame(j,game.Id,image);if(same&&!new[]{"prepared","applied","restored","ended"}.Contains(j.phase))throw new Stop("conflict","An earlier Game Rules update was interrupted. Restart the game before retrying.");
    if(action!="game-rules-apply"&&(!same||j.phase!="applied"))return new{phase="restored",message="No active owned Game Rules change.",writes=0};
    ulong hook=basis+GameRulesPolicy.HookRva;
    if(same&&j.phase=="applied"){
     if(!Read(hook,j.patch.Length).SequenceEqual(j.patch)||!Read(j.stub,j.stubBytes.Length).SequenceEqual(j.stubBytes))throw new Stop("conflict","Game Rules code changed outside this launcher. Restart the game before retrying.");
     if(action!="game-rules-restore")return new{phase="applied",message="Game Rules is unlocked. Reopen Pause or Options to refresh.",writes=0};
     GameRulesEac();Identity();GameRulesWrite(hook,j.original,true);j.phase="restored";GameRulesSave(file,j);VirtualFreeEx(handle,(IntPtr)(long)j.stub,UIntPtr.Zero,0x8000);return new{phase="restored",message="Game Rules menu returned to stock.",writes=1};
    }
    if(action!="game-rules-apply")throw new Stop("conflict","No owned Game Rules session to observe.");GameRulesBuild();ulong stub=GameRulesAllocate();byte[] stubBytes,patch;
    bool hookAttempted=false;try{stubBytes=GameRulesPolicy.Stub(stub,basis);patch=GameRulesPolicy.Patch(hook,stub);GameRulesWrite(stub,stubBytes,false);j=new GameRulesJournal{pid=game.Id,created=created,basis=basis,image=image,stub=stub,original=GameRulesPolicy.Original,patch=patch,stubBytes=stubBytes,phase="prepared"};GameRulesSave(file,j);GameRulesEac();Identity();GameRulesBuild();hookAttempted=true;GameRulesWrite(hook,patch,true);j.phase="applied";GameRulesSave(file,j);return new{phase="applied",message="Game Rules is unlocked. Reopen Pause or Options to refresh.",writes=2};}
    catch{if(hookAttempted){if(j!=null){j.phase="failed";try{GameRulesSave(file,j);}catch{}}}else VirtualFreeEx(handle,(IntPtr)(long)stub,UIntPtr.Zero,0x8000);throw;}
   }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var game in games)game.Dispose();}
  }finally{if(locked)gate.ReleaseMutex();}}
 }
}
