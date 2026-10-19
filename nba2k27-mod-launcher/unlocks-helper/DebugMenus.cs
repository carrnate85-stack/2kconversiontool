using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Threading;

// No code patches: three validated Boolean values in the current process only.
internal static class DebugFeaturePolicy {
 internal static readonly uint[] Keys={0x8EB3D32B,0x976369F2,0x5B4DD268};
 internal static void Count(int count){if(count<1||count>100000)throw new InvalidOperationException("Invalid feature count.");}
 internal static void Entry(uint packed,byte value){if((packed&31)!=11||((packed>>5)&0x3ffffff)!=1||value>1)throw new InvalidOperationException("Unexpected debug feature layout.");}
 internal static ulong Find(Func<ulong,int,byte[]> read,ulong table,int count,uint wanted){
  Count(count);int lo=0,hi=count-1;
  while(lo<=hi){int mid=lo+(hi-lo)/2;ulong p=BitConverter.ToUInt64(read(table+(ulong)mid*8,8),0);uint key=BitConverter.ToUInt32(read(p,4),0);
   if(key<wanted)lo=mid+1;else if(key>wanted)hi=mid-1;else{
    foreach(int neighbour in new[]{mid-1,mid+1})if(neighbour>=0&&neighbour<count){ulong q=BitConverter.ToUInt64(read(table+(ulong)neighbour*8,8),0);if(BitConverter.ToUInt32(read(q,4),0)==wanted)throw new InvalidOperationException("Duplicate debug feature key.");}
    return p;
   }
  }throw new InvalidOperationException("Waiting for debug feature registration.");
 }
}
internal static partial class FreestyleTeams {
 sealed class DebugSnapshot {
  public ulong manager,table;public int count;public ulong[] entries;public byte[] values;
  internal bool Same(DebugSnapshot other){return manager==other.manager&&table==other.table&&count==other.count&&entries.SequenceEqual(other.entries);}
 }
 sealed class DebugJournal {
  public int schema=1,pid;public long created;public ulong basis;public string image,phase;public uint timestamp=0x6AA2013D,size=0x36558000;public DebugSnapshot snapshot;
 }
 static void DebugBuild(){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664||D(b,pe+8)!=0x6AA2013D||D(b,pe+80)!=0x36558000)throw new Stop("unsupported","Debug menus do not support this game update.");
 }
 static void DebugEac(){foreach(var p in Process.GetProcesses())using(p){if(p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase)||p.ProcessName.Equals("start_protected_game",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Debug menus require offline/non-EAC Play. No debug writes were authorized.");}}
 static DebugSnapshot DebugRead(){
  Identity();var s=new DebugSnapshot();s.manager=Q(Read(basis+0xA167348,8));
  s.table=Q(Read(s.manager+0xF8,8));s.count=(int)D(Read(s.manager+0x110,4));
  try{DebugFeaturePolicy.Count(s.count);s.entries=DebugFeaturePolicy.Keys.Select(key=>DebugFeaturePolicy.Find(Read,s.table,s.count,key)).ToArray();
   s.values=new byte[3];for(int i=0;i<3;i++){var entry=Read(s.entries[i],9);if(D(entry)!=DebugFeaturePolicy.Keys[i])throw new InvalidOperationException("Feature map changed.");DebugFeaturePolicy.Entry(D(entry,4),entry[8]);s.values[i]=entry[8];}
  }catch(InvalidOperationException e){throw new Stop("waiting",e.Message);}
  if(Q(Read(basis+0xA167348,8))!=s.manager||Q(Read(s.manager+0xF8,8))!=s.table||(int)D(Read(s.manager+0x110,4))!=s.count)throw new Stop("waiting","Debug feature map is initializing.");
  Identity();return s;
 }
 static void DebugSave(string file,DebugJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);
  byte[] bytes=System.Text.Encoding.UTF8.GetBytes(json.Serialize(j));
  using(var stream=new FileStream(temp,FileMode.Create,FileAccess.Write,FileShare.None)){stream.Write(bytes,0,bytes.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static DebugJournal DebugLoad(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>8192)throw new Stop("error","Invalid debug recovery data.");
  var j=json.Deserialize<DebugJournal>(File.ReadAllText(file));
  if(j==null||j.schema!=1||j.snapshot==null||j.snapshot.entries==null||j.snapshot.values==null||j.snapshot.entries.Length!=3||j.snapshot.values.Length!=3||j.snapshot.values.Any(v=>v>1)||j.timestamp!=0x6AA2013D||j.size!=0x36558000)throw new Stop("error","Invalid debug recovery data.");
  return j;
 }
 static object RunDebug(string[] args){
  int pid;long expectedCreated;ulong expectedBase;
  if(args.Length!=6||!new[]{"debug-bind","debug-apply","debug-observe","debug-restore"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out expectedCreated)||!ulong.TryParse(args[5],out expectedBase))throw new Stop("error","Invalid debug menu request.");
  string action=args[0],file=Path.GetFullPath(args[2]),expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  bool write=action=="debug-apply"||action=="debug-restore";
  using(var gate=new Mutex(false,"Local\\2kLauncherDebugMenus")){
   bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","Another debug menu operation is running.");
    var games=Process.GetProcessesByName("NBA2K27");
    try{
     var j=DebugLoad(file);
     if(games.Length==0){if(j!=null&&j.phase!="ended"){j.phase="ended";DebugSave(file,j);}return new{phase="no-game",message="Waiting for NBA 2K27.",writes=0};}
     if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");
     var game=games[0];string image=Path.GetFullPath(game.MainModule.FileName);
     if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the configured folder.");
     created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(action!="debug-bind"&&(pid!=game.Id||expectedCreated!=created||expectedBase!=basis))throw new Stop("ended","The owned debug menu session ended.");
     DebugEac();handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access debug features. Game and launcher must use the same permission level.");
     Identity();DebugBuild();
     if(action=="debug-bind")return new{phase="bound",message="Waiting for debug features to initialize.",pid=game.Id,created=created.ToString(),basis=basis.ToString(),writes=0};
     bool same=j!=null&&j.pid==pid&&j.created==created&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);
     if(same&&!new[]{"applied","restored","ended"}.Contains(j.phase))throw new Stop("conflict","An earlier debug write was interrupted. Restart the game before retrying.");
     if(action!="debug-apply"&&(!same||j.phase!="applied"))return new{phase="restored",message="No active owned debug changes.",writes=0};
     var before=DebugRead();
     if(same&&j.phase=="applied"){
      if(!j.snapshot.Same(before)||before.values.Any(v=>v!=1))throw new Stop("conflict","Debug feature values changed outside this launcher. Restart the game before retrying.");
      if(action!="debug-restore")return new{phase="applied",message="Experimental debug menus enabled for this game session.",writes=0};
     }
     if(action=="debug-observe")throw new Stop("conflict","No owned debug session to observe.");
     bool restoring=action=="debug-restore";
     if(!restoring)j=new DebugJournal{pid=pid,created=created,basis=basis,image=image,snapshot=before,phase="prepared"};
     // Validate the full snapshot before journaling or attempting any write.
     DebugEac();DebugBuild();Identity();var check=DebugRead();if(!before.Same(check)||!before.values.SequenceEqual(check.values))throw new Stop("waiting","Debug features changed before writing; waiting.");
     if(restoring)j.phase="restoring";DebugSave(file,j);
     int writes=0;bool attempted=false;
     try{
      DebugEac();Identity();check=DebugRead();if(!before.Same(check)||!before.values.SequenceEqual(check.values))throw new Stop("waiting","Debug features changed before writing.");
      var expectedValues=(byte[])before.values.Clone();
      for(int i=0;i<3;i++){
       byte value=restoring?j.snapshot.values[i]:(byte)1;if(expectedValues[i]==value)continue;
       DebugEac();Identity();check=DebugRead();if(!before.Same(check)||!check.values.SequenceEqual(expectedValues))throw new Stop("conflict","Debug features changed during the operation.");
       UIntPtr done;attempted=true;ulong at=before.entries[i]+8;
       if(!WriteProcessMemory(handle,(IntPtr)(long)at,new[]{value},(UIntPtr)1,out done)||done.ToUInt64()!=1||Read(at,1)[0]!=value)throw new Stop("write-failed","Debug menu write could not be verified.");
       expectedValues[i]=value;writes++;
      }
      check=DebugRead();if(!before.Same(check)||!check.values.SequenceEqual(expectedValues))throw new Stop("write-failed","Debug menu readback changed.");
      j.phase=restoring?"restored":"applied";DebugSave(file,j);
      return new{phase=j.phase,message=restoring?"Debug menus disabled. Reopen Pause to refresh.":"Experimental debug menus enabled. Reopen Pause if needed.",writes=writes};
     }catch{j.phase=attempted?"failed":restoring?"applied":"restored";DebugSave(file,j);if(attempted)throw new Stop("write-failed","Debug menu update could not complete safely. Restart the game; recovery data was retained.");throw;}
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var game in games)game.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
}
