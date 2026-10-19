using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Threading;

internal static class BlacktopPresentationPolicy {
 internal static bool RosterEntryComplete(uint a,uint b,uint stage){return a==0x92DF06DE&&b==a&&stage==3;}
 internal static int[] Selected(int mask){if(mask<0||mask>7)throw new ArgumentException("Invalid removal selection");return new[]{0,1,2,5}.Where(i=>(mask&(i==0?1:i==1?2:4))!=0).ToArray();}
 internal static bool Selection(uint a,uint b,uint stage,byte active,byte kind){return a==0x92DF06DE&&b==a&&stage==3&&active==0&&kind==0;}
 internal static bool CanApply(uint a,uint b,uint stage,byte active,byte kind){return Selection(a,b,stage,active,kind);}
 internal static bool ResetToOriginal(ulong owner,ulong savedOwner,byte[] values,byte[] original){
  return owner==savedOwner&&values!=null&&original!=null&&values.SequenceEqual(original);
 }
 internal static bool CanReapply(bool selection,ulong owner,ulong savedOwner,byte[] values,byte[] original){
  return selection&&ResetToOriginal(owner,savedOwner,values,original);
 }
}
internal static partial class FreestyleTeams {
 sealed class PresentationJournal {
  public int schema=1,pid,mask;public long created;public ulong basis,owner;public string image,phase;
  public byte[] original,expected;
 }
 sealed class PresentationSnapshot {
  internal ulong owner;internal byte[] values;internal uint current,alternate,stage;internal byte active,kind;
  internal bool Selection {get{return BlacktopPresentationPolicy.Selection(current,alternate,stage,active,kind);}}
  internal bool CanApply {get{return BlacktopPresentationPolicy.CanApply(current,alternate,stage,active,kind);}}
 }
 static uint PresentationMode(uint at){uint index=D(Read(basis+at,4));if(index>=128)throw new Stop("waiting","Waiting for a stable game mode.");return D(Read(basis+0xAC77660+(ulong)index*64,4));}
 static PresentationSnapshot PresentationRead(){
  Identity();var s=new PresentationSnapshot();s.owner=Q(Read(basis+0xAA51F50,8));
  if(Read(s.owner,1)[0]!=0)throw new Stop("conflict","Presentation settings owner changed; no further writes.");
  Area(s.owner+0x2C4E,6,0x20000,true);s.values=Read(s.owner+0x2C4E,6);
  if(s.values.Any(v=>v>1))throw new Stop("conflict","Unexpected presentation flag layout.");
  s.current=PresentationMode(0xAC77E28);s.alternate=PresentationMode(0xAC77E24);
  s.stage=D(Read(basis+0xAEB69D0+0xEA8,4));s.active=Read(basis+0xAACC310,1)[0];s.kind=Read(Q(Read(basis+0xAACB9A0,8)),1)[0];
  if(Q(Read(basis+0xAA51F50,8))!=s.owner)throw new Stop("waiting","Presentation owner is initializing.");return s;
 }
 static void PresentationBuild(){
  DebugBuild();
  foreach(var g in new[]{new{rva=0x26BC454UL,off=0xCFE},new{rva=0x26BC874UL,off=0xCFF},new{rva=0x26BCC84UL,off=0xD00},new{rva=0x26BD8B4UL,off=0xD03}})
   if(!Read(basis+g.rva,7).SequenceEqual(new byte[]{15,182,128,(byte)(g.off&255),(byte)(g.off>>8),0,0}))throw new Stop("unsupported","Presentation flag signatures differ from the tested game build.");
 }
 static void PresentationSave(string file,PresentationJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);
  var bytes=System.Text.Encoding.UTF8.GetBytes(json.Serialize(j));using(var stream=new FileStream(temp,FileMode.Create,FileAccess.Write,FileShare.None)){stream.Write(bytes,0,bytes.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static PresentationJournal PresentationLoad(string file){
  Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>8192)throw new Stop("error","Invalid presentation recovery file.");
  var j=json.Deserialize<PresentationJournal>(File.ReadAllText(file));
  if(j==null||j.schema!=1||j.mask<0||j.mask>7||j.original==null||j.expected==null||j.original.Length!=6||j.expected.Length!=6||j.original.Any(v=>v>1)||j.expected.Any(v=>v>1))throw new Stop("error","Invalid presentation recovery data.");
  var expected=(byte[])j.original.Clone();foreach(int i in BlacktopPresentationPolicy.Selected(j.mask))expected[i]=1;
  if(!expected.SequenceEqual(j.expected))throw new Stop("error","Invalid presentation recovery values.");return j;
 }
 static object RunPresentation(string[] args){
  int pid,mask;long birth;ulong expectedBase;
  if(args.Length!=7||args[0]!="presentation-tick"||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out birth)||!ulong.TryParse(args[5],out expectedBase)||!int.TryParse(args[6],out mask)||mask<1||mask>7)throw new Stop("error","Invalid presentation request.");
  string file=Path.GetFullPath(args[2]),expectedImage=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  using(var gate=new Mutex(false,"Local\\2kLauncherBlacktopPresentation")){
   bool locked=false;try{
    try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","Another presentation operation is running.");
    var games=Process.GetProcessesByName("NBA2K27");try{
     if(games.Length==0)return new{phase="ended",message="Game ended; presentation changes cleared.",writes=0};
     if(games.Length!=1)throw new Stop("error","Expected one game process.");var game=games[0];
     string image=Path.GetFullPath(game.MainModule.FileName);created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(game.Id!=pid||created!=birth||basis!=expectedBase)throw new Stop("ended","Owned presentation session ended.");
     if(!String.Equals(image,expectedImage,StringComparison.OrdinalIgnoreCase))throw new Stop("error","Game folder differs.");
     DebugEac();handle=OpenProcess(0x438,false,pid);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access presentation flags. Match game and launcher permissions.");
     Identity();PresentationBuild();var j=PresentationLoad(file);var s=PresentationRead();
     bool same=j!=null&&j.pid==pid&&j.created==birth&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);
     if(same&&!new[]{"applied","restored"}.Contains(j.phase))throw new Stop("conflict","An interrupted presentation write needs a game restart.");
     bool owned=same&&j.phase=="applied";
     // Session-long policy: never restore or change masks inside a running game.
     // Process exit clears the memory; launcher shutdown only stops monitoring.
     if(owned&&j.mask!=mask)throw new Stop("conflict","Removal selection belongs to this game session. Restart the game to change it.");
     if(owned&&j.owner!=s.owner)throw new Stop("conflict","Presentation settings owner changed. Close the game before retrying.");
     bool unchanged=owned&&j.expected.SequenceEqual(s.values);
     bool reset=owned&&BlacktopPresentationPolicy.ResetToOriginal(s.owner,j.owner,s.values,j.original);
     if(owned&&!unchanged&&!reset)throw new Stop("conflict","Presentation flags changed outside the launcher. Close the game to clear the test.");
     if(unchanged)return new{phase="applied",message="Removal settings retained for the entire game session.",writes=0};
     if(reset&&!s.Selection)return new{phase="waiting",message="The game reset removal flags. Waiting for Blacktop player selection to reapply them safely.",writes=0};
     bool reapply=reset&&BlacktopPresentationPolicy.CanReapply(s.Selection,s.owner,j.owner,s.values,j.original);
     if(!s.CanApply)return new{phase="waiting",message="Waiting for a stable Blacktop player-selection screen.",writes=0};
     if(!reapply){var desired=(byte[])s.values.Clone();foreach(int i in BlacktopPresentationPolicy.Selected(mask))desired[i]=1;
      j=new PresentationJournal{pid=pid,created=birth,basis=basis,image=image,owner=s.owner,mask=mask,original=s.values,expected=desired,phase="prepared"};}
     else j.phase="prepared";
     PresentationSave(file,j);bool attempted=false;int writes=0;
     try{
      var current=(byte[])s.values.Clone();var target=j.expected;
      foreach(int i in BlacktopPresentationPolicy.Selected(j.mask)){
       if(current[i]==target[i])continue;
       DebugEac();Identity();var check=PresentationRead();
       if(check.owner!=s.owner||!check.values.SequenceEqual(current)||!check.CanApply)throw new Stop("conflict","Presentation context changed before writing.");
       UIntPtr done;attempted=true;ulong at=s.owner+0x2C4E+(ulong)i;
       if(!WriteProcessMemory(handle,(IntPtr)(long)at,new[]{target[i]},(UIntPtr)1,out done)||done.ToUInt64()!=1||Read(at,1)[0]!=target[i])throw new Stop("write-failed","Presentation write could not be verified.");
       current[i]=target[i];writes++;
      }
      var after=PresentationRead();if(after.owner!=s.owner||!after.values.SequenceEqual(target))throw new Stop("write-failed","Presentation readback changed.");
      j.phase="applied";PresentationSave(file,j);
      return new{phase=j.phase,message="Removal settings applied for the entire game session.",writes=writes};
     }catch{j.phase=attempted?"failed":"restored";PresentationSave(file,j);if(attempted)throw new Stop("write-failed","Presentation write stopped. Close game to clear changes; recovery retained.");throw;}
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var p in games)p.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
}
