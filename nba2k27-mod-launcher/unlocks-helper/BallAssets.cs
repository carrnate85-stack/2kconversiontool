using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Diagnostics;
using System.Threading;
using System.Runtime.InteropServices;

// Data-only ball substitution, shared by the launcher and isolated live trial.
// Keep the normal model ID, native requests, reference counts and renderer intact.
internal static class BallAssetPolicy {
 internal const uint Normal=0xCB5691BE,Default=0xDC1BFEC5,References=0xAC76B98;
 internal const string NormalFile="ball_gameplay_official_NBA_NBA_Official.iff";
 internal static bool MainMenu(uint mode){return mode==0x6509837B||mode==0x079964C0;}
 internal static uint Canonical(uint id){return id==0||id==Default?Normal:id;}
 internal static bool Selection(uint a,uint b,uint stage,byte active,byte kind){return a==0x92DF06DE&&b==a&&stage==3&&active==0&&kind==0;}
 internal static bool QuietMenu(uint a,uint b,uint stage,byte active,byte kind){return active==0&&kind==0&&(Selection(a,b,stage,active,kind)||(MainMenu(a)&&a==b));}
 internal static bool Unloaded(int refs,byte[] resource){return refs==0&&resource!=null&&resource.Length==48&&resource.All(v=>v==0);}
 internal static bool MayWrite(bool quiet,int refs,byte[] resource){return quiet&&Unloaded(refs,resource);}
 internal static bool ObservedLoad(int refs,byte[] resource){return refs>0||(resource!=null&&resource.Any(v=>v!=0));}
 internal static byte[] Replacement(byte[] record,string filename){
  if(record==null||record.Length!=BallSelectorPolicy.RecordStride||BitConverter.ToUInt32(record,4)!=Normal)throw new Exception("Unexpected normal ball record");
  if(filename==null||filename.Length>127||!Regex.IsMatch(filename,@"^ball_[a-zA-Z0-9_.-]+\.iff$")||filename.Contains(".."))throw new Exception("Invalid ball asset filename");
  int at=BallSelectorPolicy.RecordAssetOffset,end=Array.IndexOf(record,(byte)0,at);
  if(end<at||Encoding.ASCII.GetString(record,at,end-at)!=NormalFile)throw new Exception("Normal ball asset was changed outside this launcher");
  int length=Math.Max(end-at,filename.Length)+1;
  if(at+length>record.Length||record.Skip(end).Take(at+length-end).Any(v=>v!=0))throw new Exception("Ball filename has no verified free space");
  var next=(byte[])record.Clone();Array.Clear(next,at,length);Array.Copy(Encoding.ASCII.GetBytes(filename),0,next,at,filename.Length);return next;
 }
 internal static int FieldLength(byte[] original,byte[] replacement){
  int a=Array.IndexOf(original,(byte)0,BallSelectorPolicy.RecordAssetOffset),b=Array.IndexOf(replacement,(byte)0,BallSelectorPolicy.RecordAssetOffset);
  if(a<BallSelectorPolicy.RecordAssetOffset||b<BallSelectorPolicy.RecordAssetOffset)throw new Exception("Unterminated ball filename");return Math.Max(a,b)-BallSelectorPolicy.RecordAssetOffset+1;
 }
 internal static bool Current(string logical){return String.Equals(logical,"current",StringComparison.OrdinalIgnoreCase)||String.Equals(logical,"balls/"+NormalFile,StringComparison.OrdinalIgnoreCase)||String.Equals(logical,"balls/ball_gameplay_official_default.iff",StringComparison.OrdinalIgnoreCase);}
 internal static bool ValidChoice(string logical){return logical=="current"||(logical!=null&&logical.Length<=160&&!logical.Contains("..")&&Regex.IsMatch(logical,@"^balls/ball_[a-zA-Z0-9_.-]+\.iff$"));}
 internal static bool RestoreDue(bool used,bool changed,bool stopping,bool recovering,bool mainMenu){return used||changed||stopping||recovering||mainMenu;}
 internal static bool RestoreNow(bool used,bool changed,bool stopping,bool recovering,bool mainMenu,bool quiet,int refs,byte[] resource){return RestoreDue(used,changed,stopping,recovering,mainMenu)&&MayWrite(quiet,refs,resource);}
}

internal static partial class FreestyleTeams {
 sealed class BallAssetJournal {
  public int schema=1,pid;public long birth;public ulong imageBase,holder,records,address,allocation;public uint index,sourceModel;
  public string image,logical,phase;public bool used;public byte[] original,replacement;
 }
 sealed class BallAssetSnapshot {
  public ulong holder,records,address,allocation,settings;public uint index,current,alternate,stage,mode,nativeBall;public byte active,kind;
  public int references;public byte[] record,resource;
  public bool selection {get{return BallAssetPolicy.Selection(current,alternate,stage,active,kind);}}
  public bool quiet {get{return BallAssetPolicy.QuietMenu(current,alternate,stage,active,kind);}}
  public bool unloaded {get{return BallAssetPolicy.Unloaded(references,resource);}}
 }
 static uint AssetMode(uint rva){uint i=D(Read(basis+rva,4));return i<128?D(Read(basis+BallSelectorPolicy.ModeTableRva+i*64UL,4)):0;}
 static void AssetBuild(){
  BallBuild();
  // Reject all sites used by the rejected getter experiment as well.
  foreach(var p in new[]{Tuple.Create(0x2E40A4U,0x257C040U),Tuple.Create(0x2E4432U,0x257C040U),Tuple.Create(0x40FEEA4U,0x257C030U)})
   if(!Read(basis+p.Item1,5).SequenceEqual(BallSelectorPolicy.Call(basis+p.Item1,basis+p.Item2)))throw new Exception("Previous ball experiment is still installed; restart the game");
  // Verified native filename field and default -> NBA identity normalization.
  if(!Read(basis+0x21BFED8,7).SequenceEqual(new byte[]{0x48,0x81,0xC7,0x9C,0,0,0})||
     !Read(basis+0x21BE198,6).SequenceEqual(new byte[]{0x81,0xFF,0xC5,0xFE,0x1B,0xDC})||
     !Read(basis+0x21BE1A0,8).SequenceEqual(new byte[]{0xC7,0x44,0x24,0x48,0xBE,0x91,0x56,0xCB}))throw new Exception("Ball asset layout differs from the verified build");
  if(!Read(basis+0x31436C0,8).SequenceEqual(new byte[]{0x48,0x8D,0x05,0xA1,0x5C,0x6B,0x04,0xC3})||
     !Read(basis+0x31436D0,10).SequenceEqual(new byte[]{0x48,0x89,0xD0,0xC7,0x02,0xC0,0x64,0x99,0x07,0xC3})||
     !Read(basis+0x77F9368,Encoding.ASCII.GetByteCount("NBA_MainMenu_PlayNow\0")).SequenceEqual(Encoding.ASCII.GetBytes("NBA_MainMenu_PlayNow\0")))throw new Exception("Play Now exit-menu identity differs from the verified build");
 }
 static BallAssetSnapshot AssetSnapshot(){
  Identity();var s=new BallAssetSnapshot();s.holder=Q(Read(basis+BallSelectorPolicy.RecordHolderRva,8));s.records=Q(Read(s.holder+0x18,8))+BallSelectorPolicy.RecordOffset;
  uint count=D(Read(basis+BallSelectorPolicy.RegistryRva+8,4));ulong table=Q(Read(basis+BallSelectorPolicy.RegistryRva,8));
  if(count<1||count>1024||table<0x10000)throw new Stop("waiting","Waiting for the ball registry.");byte[] entries=Read(table,(int)count*8);int matches=0;
  for(int i=0;i<count;i++)if(D(entries,i*8)==BallAssetPolicy.Normal){s.index=D(entries,i*8+4);matches++;}
  if(matches!=1||s.index>=500)throw new Exception("Normal ball registry identity is not unique");
  s.address=s.records+s.index*(ulong)BallSelectorPolicy.RecordStride;var region=Area(s.address,BallSelectorPolicy.RecordStride,0x20000,true);s.allocation=(ulong)region.Allocation.ToInt64();
  s.record=Read(s.address,BallSelectorPolicy.RecordStride);if(D(s.record,4)!=BallAssetPolicy.Normal)throw new Exception("Normal ball record identity differs");
  s.references=BitConverter.ToInt32(Read(basis+BallAssetPolicy.References+0x18+s.index*4UL,4),0);
  s.resource=Read(basis+BallSelectorPolicy.RendererRva+0x28+s.index*48UL,48);
  s.settings=Q(Read(basis+BallSelectorPolicy.SettingsRva,8));s.kind=Read(s.settings,1)[0];s.active=Read(basis+0xAACC310,1)[0];s.mode=D(Read(s.settings+0xA04,4));s.nativeBall=D(Read(s.settings+0xDA4,4));
  s.current=AssetMode(BallSelectorPolicy.ModeCurrentRva);s.alternate=AssetMode(BallSelectorPolicy.ModeAlternateRva);s.stage=D(Read(basis+BallSelectorPolicy.BlacktopStageRva,4));
  if(s.holder!=Q(Read(basis+BallSelectorPolicy.RecordHolderRva,8))||s.records!=Q(Read(s.holder+0x18,8))+BallSelectorPolicy.RecordOffset||s.settings!=Q(Read(basis+BallSelectorPolicy.SettingsRva,8))||s.current!=AssetMode(BallSelectorPolicy.ModeCurrentRva)||s.alternate!=AssetMode(BallSelectorPolicy.ModeAlternateRva))throw new Stop("waiting","Ball context changed during observation.");
  Identity();return s;
 }
 static string AssetName(byte[] record){return Encoding.ASCII.GetString(record,BallSelectorPolicy.RecordAssetOffset,record.Length-BallSelectorPolicy.RecordAssetOffset).Split('\0')[0];}
 static object AssetReport(BallAssetSnapshot s,BallAssetJournal j){return new{phase=j==null?"unmodified":j.phase,selection=s.selection,quietMenu=s.quiet,gameMode=s.mode,current=s.current.ToString("X8"),alternate=s.alternate.ToString("X8"),stage=s.stage,active=s.active,kind=s.kind,nativeBall=s.nativeBall.ToString("X8"),canonicalBall=BallAssetPolicy.Canonical(s.nativeBall).ToString("X8"),targetModel=BallAssetPolicy.Normal.ToString("X8"),index=s.index,asset=AssetName(s.record),references=s.references,resourceEmpty=s.resource.All(v=>v==0),meshReady=Q(s.resource,16)!=0,canPrepare=s.selection&&s.unloaded&&BallAssetPolicy.Canonical(s.nativeBall)==BallAssetPolicy.Normal,canRestore=s.quiet&&s.unloaded};}
 static void AssetSave(string file,BallAssetJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+"."+Guid.NewGuid().ToString("N")+".tmp";Plain(temp);
  byte[] bytes=Encoding.UTF8.GetBytes(json.Serialize(j));using(var f=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){f.Write(bytes,0,bytes.Length);f.Flush(true);}if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static BallAssetJournal AssetReadJournal(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>16384)throw new Exception("Invalid asset journal size");var j=json.Deserialize<BallAssetJournal>(File.ReadAllText(file));
  if(j==null||j.schema!=1||j.pid<=0||j.birth<=0||j.imageBase<0x10000||String.IsNullOrEmpty(j.image)||!Path.IsPathRooted(j.image)||!BallAssetPolicy.ValidChoice(j.logical)||j.index>=500||j.address!=j.records+j.index*(ulong)BallSelectorPolicy.RecordStride||j.original==null||j.replacement==null||j.original.Length!=BallSelectorPolicy.RecordStride||j.replacement.Length!=j.original.Length||!new[]{"prepared","armed","restored"}.Contains(j.phase))throw new Exception("Invalid ball asset recovery journal");
  if(!BallAssetPolicy.Replacement(j.original,AssetName(j.replacement)).SequenceEqual(j.replacement))throw new Exception("Asset journal includes changes outside the filename");return j;
 }
 static bool AssetSame(BallAssetJournal j,int pid,string image){return j!=null&&j.pid==pid&&j.birth==created&&j.imageBase==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);}
 static BallAssetJournal AssetLoad(string file,int pid,string image){var j=AssetReadJournal(file);if(j!=null&&!AssetSame(j,pid,image))throw new Exception("Asset journal does not belong to this exact session");return j;}
 static void AssetOwned(BallAssetSnapshot s,BallAssetJournal j,byte[] expected){if(s.holder!=j.holder||s.records!=j.records||s.address!=j.address||s.allocation!=j.allocation||s.index!=j.index||!s.record.SequenceEqual(expected))throw new Exception("Ball record ownership changed; close the game. No replacement/restoration attempted");}
 static void AssetStable(BallAssetJournal j,bool restore){
  for(int i=0;i<4;i++){
   var s=AssetSnapshot();AssetOwned(s,j,restore?j.replacement:j.original);
   if(!BallAssetPolicy.MayWrite(restore?s.quiet:s.selection,s.references,s.resource)||(!restore&&BallAssetPolicy.Canonical(s.nativeBall)!=BallAssetPolicy.Normal))throw new Stop("waiting","Normal ball is loaded, referenced, or outside a safe menu; no asset change attempted.");
   if(i<3)Thread.Sleep(100);
  }
 }
 static void AssetWrite(BallAssetJournal j,bool restore){
  BallEac();AssetBuild();AssetStable(j,restore);Identity();var s=AssetSnapshot();byte[] expected=restore?j.replacement:j.original,next=restore?j.original:j.replacement;AssetOwned(s,j,expected);
  if(!BallAssetPolicy.MayWrite(restore?s.quiet:s.selection,s.references,s.resource)||(!restore&&BallAssetPolicy.Canonical(s.nativeBall)!=BallAssetPolicy.Normal))throw new Stop("waiting","Ball became busy before asset change.");
  int length=BallAssetPolicy.FieldLength(j.original,j.replacement);Area(j.address+BallSelectorPolicy.RecordAssetOffset,(ulong)length,0x20000,true);
  // The sole game-memory write in this implementation. No protection changes,
  // allocation, model/counter edits, engine calls or executable writes.
  BallWrite(j.address+BallSelectorPolicy.RecordAssetOffset,next.Skip(BallSelectorPolicy.RecordAssetOffset).Take(length).ToArray(),false);
  AssetOwned(AssetSnapshot(),j,next);
 }
#if BALL_ASSET_EXPERIMENT
 static void AssetTrace(Process game,string file,BallAssetJournal j){
  string trace=file+"."+DateTime.UtcNow.ToString("yyyyMMdd-HHmmss-fff")+".trace.jsonl";Plain(trace);
  using(var f=new FileStream(trace,FileMode.CreateNew,FileAccess.Write,FileShare.Read))using(var w=new StreamWriter(f)){
   Action<object> log=value=>{string line=json.Serialize(value);w.WriteLine(line);w.Flush();f.Flush(true);Console.WriteLine(line);Console.Out.Flush();};
   log(new{eventType="prepared",time=DateTime.UtcNow.ToString("o"),pid=game.Id,trace=trace,message="Data-only substitution ready. Start one Blacktop match; return to the main menu and wait for restored before another mode.",state=AssetReport(AssetSnapshot(),j)});
   var timer=Stopwatch.StartNew();bool used=false;string previous="";int samples=0;
   while(timer.ElapsedMilliseconds<180000){
    if(game.HasExited){log(new{eventType="game-exit",time=DateTime.UtcNow.ToString("o"),exitCode=game.ExitCode,message="Game exited; its temporary memory substitution is gone."});return;}
    var s=AssetSnapshot();AssetOwned(s,j,j.replacement);used|=BallAssetPolicy.ObservedLoad(s.references,s.resource);
    string state=json.Serialize(AssetReport(s,j));if(previous!=state||samples%25==0){log(new{eventType="sample",time=DateTime.UtcNow.ToString("o"),state=json.DeserializeObject(state)});previous=state;}
    if((used||BallAssetPolicy.MainMenu(s.current))&&s.quiet&&s.unloaded){AssetWrite(j,true);j.phase="restored";AssetSave(file,j);log(new{eventType="restored",time=DateTime.UtcNow.ToString("o"),observedNativeLoad=used,message="Original ball filename restored at a safe menu; next mode may now be tested.",state=AssetReport(AssetSnapshot(),j)});return;}
    Thread.Sleep(200);samples++;
   }
   var last=AssetSnapshot();if(last.quiet&&last.unloaded){AssetWrite(j,true);j.phase="restored";AssetSave(file,j);log(new{eventType="restored",reason="test-timeout"});}
   else log(new{eventType="timeout",message="Test observer stopped. Close the game before starting another mode; the filename was not restored while resources remain in use."});
  }
 }
 internal static int RunBallAssetExperiment(string[] args){
  try{
   int pid;long birth;ulong imageBase;
   if(args.Length!=6||!new[]{"inspect","prepare","restore"}.Contains(args[0])||!int.TryParse(args[1],out pid)||!long.TryParse(args[2],out birth)||!ulong.TryParse(args[3],out imageBase))throw new Exception("Usage: inspect|prepare|restore PID creationTime decimalBase journal selectedBallLogicalPath");
   bool write=args[0]!="inspect";string file=Path.GetFullPath(args[4]);if(write)BallEac();
   using(var gate=new Mutex(false,"Local\\2kLauncherBallAssetExperiment")){
    bool locked=false;try{
     if(write){try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Exception("A ball asset test is already running");}
     using(var game=Process.GetProcessById(pid)){
      if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected process");string image=game.MainModule.FileName;
      created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();if(created!=birth||basis!=imageBase)throw new Exception("Game session identity changed");
      handle=OpenProcess(write?0x438U:0x410U,false,pid);if(handle==IntPtr.Zero)throw new Exception("Process access unavailable");Identity();AssetBuild();
      var j=AssetLoad(file,pid,image);var s=AssetSnapshot();
      if(args[0]=="inspect"){if(j!=null)AssetOwned(s,j,j.phase=="restored"?j.original:j.replacement);Console.WriteLine(json.Serialize(AssetReport(s,j)));return 0;}
      if(args[0]=="restore"){if(j==null)throw new Exception("No owned asset substitution");if(j.phase!="restored"){if(s.record.SequenceEqual(j.original)){AssetOwned(s,j,j.original);}else AssetWrite(j,true);j.phase="restored";AssetSave(file,j);}Console.WriteLine(json.Serialize(new{phase="restored"}));return 0;}
      if(j!=null)throw new Exception("This test journal already exists; no duplicate preparation");
      if(!s.selection||!s.unloaded||BallAssetPolicy.Canonical(s.nativeBall)!=BallAssetPolicy.Normal)throw new Exception("Preflight refused: normal ball is cached/in use or Blacktop selection is not stable. No memory was changed.");
      uint sourceIndex;uint source=BallResolve(args[5],out sourceIndex);if(source==BallAssetPolicy.Normal)throw new Exception("Choose a different stock ball for this test");
      byte[] sourceRecord=Read(s.records+sourceIndex*(ulong)BallSelectorPolicy.RecordStride,BallSelectorPolicy.RecordStride);
      if(D(sourceRecord,4)!=source)throw new Exception("Selected ball record changed");string filename=AssetName(sourceRecord);
      if(!File.ReadLines(Path.Combine(Path.GetDirectoryName(image),"manifest")).Any(line=>line.Split(',')[0].Replace('\\','/').Equals("balls/"+filename,StringComparison.OrdinalIgnoreCase)))throw new Exception("Selected ball asset is not in the game manifest");
      j=new BallAssetJournal{pid=pid,birth=created,imageBase=basis,image=image,holder=s.holder,records=s.records,address=s.address,allocation=s.allocation,index=s.index,sourceModel=source,logical=args[5],phase="prepared",original=s.record,replacement=BallAssetPolicy.Replacement(s.record,filename)};
      AssetSave(file,j);AssetWrite(j,false);j.phase="armed";AssetSave(file,j);AssetTrace(game,file,j);return 0;
     }
    }finally{if(locked)gate.ReleaseMutex();}
   }
  }catch(Exception e){Console.WriteLine(json.Serialize(new{phase="error",message=e.Message,warning="If a substitution was prepared, close the game before another mode. Recovery journal retained."}));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
#endif
}
#if BALL_ASSET_EXPERIMENT
internal static class BallAssetExperimentEntry {public static int Main(string[] args){return FreestyleTeams.RunBallAssetExperiment(args);}}
#endif
