using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Collections.Generic;
using System.Runtime.InteropServices;

// Isolated experiment, NOT dispatched by the production launcher/helper.
// Substitute getter results in the existing native load/release/config calls.
// No extra loader calls, settings writes, resource pointer swaps or render hook.
internal static class BallRequestExperimentPolicy {
 internal const uint LoadSite=0x2E40A4,ReleaseSite=0x2E4432,ActorSite=0x40FEEA4;
 internal static readonly uint[] Sites={ReleaseSite,LoadSite,ActorSite};
 internal static uint Target(uint site){return site==ActorSite?0x257C030U:0x257C040U;}
 internal static int Offset(uint site){return site==LoadSite?0:site==ReleaseSite?0x400:0x800;}
 internal static byte[] Stub(ulong control,ulong basis,uint site){
  var b=new List<byte>();var labels=new Dictionary<string,int>();var jumps=new List<Tuple<int,string>>();
  Action<byte[]> add=v=>b.AddRange(v);Action<string> label=n=>labels[n]=b.Count;
  Action<byte[],string> jump=(op,n)=>{add(op);int at=b.Count;add(new byte[4]);jumps.Add(Tuple.Create(at,n));};
  Action<ulong> rax=at=>{add(new byte[]{0x48,0xB8});add(BitConverter.GetBytes(at));};
  // Preserve every register except RAX (the getter return), including flags.
  add(new byte[]{0x9C,0x51,0x52,0xB9});add(BitConverter.GetBytes(0xCB5691BEU));
  rax(basis+BallSelectorPolicy.SettingsRva);add(new byte[]{0x48,0x8B,0x00,0x48,0x3D,0,0,1,0});jump(new byte[]{0x0F,0x82},"done");
  add(new byte[]{0x8B,0x88,0xA4,0x0D,0,0,0x48,0xBA});add(BitConverter.GetBytes(control));
  if(site==ReleaseSite){
   add(new byte[]{0xFF,0x42,0x10,0x83,0x7A,0x1C,0});jump(new byte[]{0x0F,0x84},"done");
   add(new byte[]{0x8B,0x4A,0x08,0xFF,0x4A,0x1C});jump(new byte[]{0x0F,0x85},"done");
   add(new byte[]{0xC7,0x42,0x08,0,0,0,0});jump(new byte[]{0xE9},"done");
  }else{
   add(new byte[]{0xFF,0x42,site==LoadSite?(byte)0x0C:(byte)0x14});
   add(new byte[]{0x83,0xB8,0x04,0x0A,0,0,6});jump(new byte[]{0x0F,0x85},"done");
   rax(basis+BallSelectorPolicy.BlacktopStageRva);add(new byte[]{0x8B,0x00,0x83,0xE8,1,0x83,0xF8,2});jump(new byte[]{0x0F,0x87},"done");
   if(site==LoadSite){
    add(new byte[]{0x83,0x7A,0x1C,0});jump(new byte[]{0x0F,0x85},"reuse");
    add(new byte[]{0x8B,0x02,0x85,0xC0});jump(new byte[]{0x0F,0x84},"done");
    add(new byte[]{0x89,0x42,0x08});label("reuse");
    add(new byte[]{0x8B,0x4A,0x08,0xFF,0x42,0x1C});
   }else{
    add(new byte[]{0x83,0x7A,0x1C,0});jump(new byte[]{0x0F,0x84},"done");
    add(new byte[]{0x8B,0x42,4,0x3D});add(BitConverter.GetBytes(500));jump(new byte[]{0x0F,0x83},"done");
    add(new byte[]{0x48,0x6B,0xC0,0x30,0x52,0x48,0xBA});add(BitConverter.GetBytes(basis+BallSelectorPolicy.RendererRva));
    add(new byte[]{0x48,0x01,0xD0,0x5A,0x48,0x83,0x78,0x38,0});jump(new byte[]{0x0F,0x84},"done");
    add(new byte[]{0x8B,0x4A,0x08,0x89,0x4A,0x18});
   }
  }
  label("done");add(new byte[]{0x89,0xC8,0x5A,0x59,0x9D,0xC3});
  foreach(var f in jumps){var v=BitConverter.GetBytes(labels[f.Item2]-(f.Item1+4));for(int i=0;i<4;i++)b[f.Item1+i]=v[i];}
  if(b.Count>=0x400)throw new Exception("Experiment stub too large");return b.ToArray();
 }
 internal static byte[] Code(ulong control,ulong basis){var code=new byte[0xC00];foreach(uint s in Sites){var b=Stub(control,basis,s);Array.Copy(b,0,code,Offset(s),b.Length);}return code;}
}

internal static partial class FreestyleTeams {
 sealed class BallRequestJournal {public int schema=1,pid;public long created;public ulong basis,stub,control;public string logical,phase;public uint model,index;}
 static void SaveBallRequest(string file,BallRequestJournal j){Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);byte[] b=System.Text.Encoding.UTF8.GetBytes(json.Serialize(j));using(var f=new FileStream(temp,FileMode.Create,FileAccess.Write,FileShare.None)){f.Write(b,0,b.Length);f.Flush(true);}if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);}
 static byte[] BallRequestOriginal(uint site){return BallSelectorPolicy.Call(basis+site,basis+BallRequestExperimentPolicy.Target(site));}
 static byte[] BallRequestPatch(uint site,BallRequestJournal j){return BallSelectorPolicy.Call(basis+site,j.stub+(ulong)BallRequestExperimentPolicy.Offset(site));}
 static object BallRequestReferences(BallRequestJournal j,ulong settings){
  // Exact-image native load/release counter table, indexed by 21BE3B0.
  var ids=new HashSet<uint>();if(j!=null)ids.Add(j.model);if(settings>=0x10000)ids.Add(D(Read(settings+0xDA4,4)));ids.Add(0xCB5691BE);
  ulong registry=Q(Read(basis+BallSelectorPolicy.RegistryRva,8));uint count=D(Read(basis+BallSelectorPolicy.RegistryRva+8,4));
  if(registry<0x10000||count<1||count>1024)throw new Exception("Ball registry unavailable");
  byte[] entries=Read(registry,(int)count*8);var rows=new List<object>();
  for(int i=0;i<count;i++){uint model=D(entries,i*8),index=D(entries,i*8+4);if(index>=500||!ids.Contains(model))continue;rows.Add(new{model=model.ToString("X8"),index=index,references=BitConverter.ToInt32(Read(basis+0xAC76B98+0x18+(ulong)index*4,4),0)});}
  return rows;
 }
 static object BallRequestReport(BallRequestJournal j){
  var table=Read(basis+0xAACB9B0,30*0x50);var players=new List<object>();
  for(int i=0;i<30;i++){int off=i*0x50;ulong roster=Q(table,off+0x30),manager=Q(table,off+0x10);if(roster!=0||manager!=0)players.Add(new{slot=i,roster=roster.ToString("X"),manager=manager.ToString("X")});}
  byte[] state=j==null?new byte[32]:Read(j.control,32);ulong settings=Q(Read(basis+BallSelectorPolicy.SettingsRva,8));
  return new{phase=j==null?"uninstalled":j.phase,pid=j==null?0:j.pid,scope=BallScope(),gameMode=settings>=0x10000?D(Read(settings+0xA04,4)):uint.MaxValue,nativeBall=settings>=0x10000?D(Read(settings+0xDA4,4)).ToString("X8"):"unavailable",desired=D(state).ToString("X8"),captured=D(state,8).ToString("X8"),loadGetterCalls=D(state,12),releaseGetterCalls=D(state,16),actorGetterCalls=D(state,20),actorChoice=D(state,24).ToString("X8"),pendingReferences=D(state,28),meshReady=j!=null&&Q(Read(basis+BallSelectorPolicy.RendererRva+0x38+(ulong)j.index*48,8))!=0,nativeReferences=BallRequestReferences(j,settings),players=players,playerTableStable=table.SequenceEqual(Read(basis+0xAACB9B0,table.Length))};
 }
 static void WatchBallRequest(Process game,BallRequestJournal j,string journal){
  // Bounded passive observation. Never attaches a debugger or writes game memory.
  string output=journal+"."+DateTime.UtcNow.ToString("yyyyMMdd-HHmmss-fff")+".trace.jsonl";Plain(output);
  using(var stream=new FileStream(output,FileMode.CreateNew,FileAccess.Write,FileShare.Read))using(var writer=new StreamWriter(stream)){
   Action<object> record=value=>{writer.WriteLine(json.Serialize(value));writer.Flush();stream.Flush(true);};
   record(new{kind="start",time=DateTime.UtcNow.ToString("o"),pid=game.Id,created=created,basis=basis,readOnly=true});
   Console.WriteLine(json.Serialize(new{phase="watching",path=output,seconds=180,readOnly=true}));
   var clock=Stopwatch.StartNew();string previous=null;int sequence=0;
   while(clock.ElapsedMilliseconds<180000){
    if(game.HasExited){record(new{kind="exit",time=DateTime.UtcNow.ToString("o"),exitCode=game.ExitCode,exitCodeHex=unchecked((uint)game.ExitCode).ToString("X8")});Console.WriteLine(json.Serialize(new{phase="ended",path=output,exitCode=game.ExitCode}));return;}
    string snapshot;try{Identity();snapshot=json.Serialize(BallRequestReport(j));}catch(Exception e){snapshot=json.Serialize(new{readError=e.Message});}
    if(snapshot!=previous||sequence%25==0){record(new{kind="sample",time=DateTime.UtcNow.ToString("o"),sequence=sequence,state=json.DeserializeObject(snapshot)});previous=snapshot;}
    sequence++;System.Threading.Thread.Sleep(200);
   }
   record(new{kind="timeout",time=DateTime.UtcNow.ToString("o"),readOnly=true});Console.WriteLine(json.Serialize(new{phase="timeout",path=output}));
  }
 }
 internal static int RunBallRequestExperiment(string[] args){
  try{
   int pid;long stamp;ulong expectedBase;
   if(args.Length!=6||!new[]{"inspect","watch","install","restore"}.Contains(args[0])||!int.TryParse(args[1],out pid)||!long.TryParse(args[2],out stamp)||!ulong.TryParse(args[3],out expectedBase))throw new Exception("Usage: inspect|watch|install|restore PID creationTime decimalBase journal logicalPath");
   string file=Path.GetFullPath(args[4]);bool write=args[0]=="install"||args[0]=="restore";
   if(write)BallEac();
   using(var game=Process.GetProcessById(pid)){
    if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected process");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
    if(created!=stamp||basis!=expectedBase)throw new Exception("Session identity changed");
    handle=OpenProcess(write?0x438U:0x410U,false,pid);if(handle==IntPtr.Zero)throw new Exception("Process access unavailable");Identity();BallBuild();
    Plain(file);BallRequestJournal j=null;
    if(File.Exists(file)){
     if(new FileInfo(file).Length>4096)throw new Exception("Invalid experiment journal");j=json.Deserialize<BallRequestJournal>(File.ReadAllText(file));
     if(j==null||j.schema!=1||j.pid!=pid||j.created!=created||j.basis!=basis||j.control!=j.stub+0x1000||j.stub<0x10000||j.index>=500)throw new Exception("Experiment journal belongs to a different session");
     byte[] code=BallRequestExperimentPolicy.Code(j.control,basis);if(!Read(j.stub,code.Length).SequenceEqual(code))throw new Exception("Experiment bridge changed");
    }
    foreach(uint site in BallRequestExperimentPolicy.Sites){var live=Read(basis+site,5);if(!live.SequenceEqual(BallRequestOriginal(site))&&(j==null||!live.SequenceEqual(BallRequestPatch(site,j))))throw new Exception("Getter callsite has an unknown patch");}
    foreach(uint getter in new uint[]{0x257C030,0x257C040}){
     var code=Read(basis+getter,14);if(code[0]!=0x48||code[1]!=0x8B||code[2]!=0x05||basis+getter+7+(ulong)(long)BitConverter.ToInt32(code,3)!=basis+BallSelectorPolicy.SettingsRva||!code.Skip(7).SequenceEqual(new byte[]{0x8B,0x80,0xA4,0x0D,0,0,0xC3}))throw new Exception("Native settings getter changed");
    }
    if(args[0]=="inspect"){Console.WriteLine(json.Serialize(BallRequestReport(j)));return 0;}
    if(args[0]=="watch"){WatchBallRequest(game,j,file);return 0;}
    if(BallScope()!="selection")throw new Exception("Return to stable Blacktop player selection before modifying this test");
    if(args[0]=="restore"){
     if(j==null)throw new Exception("No owned test to restore");if(D(Read(j.control+28,4))!=0)throw new Exception("Native match still owns the selected asset; exit that match or close the game first");
     BallWrite(j.control,new byte[8],false);
     foreach(uint site in BallRequestExperimentPolicy.Sites){if(Read(basis+site,5).SequenceEqual(BallRequestPatch(site,j)))BallWrite(basis+site,BallRequestOriginal(site),true);}
     j.phase="restored";SaveBallRequest(file,j);Console.WriteLine(json.Serialize(new{phase="restored",writes=4}));return 0;
    }
    if(j!=null)throw new Exception("An experiment journal already exists; do not install twice");
    uint index;uint model=BallResolve(args[5],out index);ulong stub=BallAllocate();j=new BallRequestJournal{pid=pid,created=created,basis=basis,stub=stub,control=stub+0x1000,model=model,index=index,logical=args[5],phase="prepared"};
    byte[] bridge=BallRequestExperimentPolicy.Code(j.control,basis);BallWrite(stub,bridge,false);BallWrite(j.control,new byte[32],false);
    uint previous;if(!VirtualProtectEx(handle,(IntPtr)(long)stub,(UIntPtr)0x1000,0x20,out previous)||!VirtualProtectEx(handle,(IntPtr)(long)j.control,(UIntPtr)0x1000,4,out previous))throw new Exception("Could not protect test allocation");
    if(!FlushInstructionCache(handle,(IntPtr)(long)stub,(UIntPtr)bridge.Length))throw new Exception("Could not flush test code");
    SaveBallRequest(file,j);Identity();if(BallScope()!="selection")throw new Exception("Selection changed before publishing the test");
    foreach(uint site in BallRequestExperimentPolicy.Sites){if(!Read(basis+site,5).SequenceEqual(BallRequestOriginal(site)))throw new Exception("Callsite changed before publishing");BallWrite(basis+site,BallRequestPatch(site,j),true);}
    j.phase="installed";SaveBallRequest(file,j);BallWrite(j.control,BitConverter.GetBytes(((ulong)index<<32)|model),false);
    Console.WriteLine(json.Serialize(BallRequestReport(j)));return 0;
   }
  }catch(Exception e){Console.WriteLine(json.Serialize(new{phase="error",message=e.Message}));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
}
internal static class BallRequestExperimentEntry {public static int Main(string[] args){return FreestyleTeams.RunBallRequestExperiment(args);}}
