using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Threading;

internal static class BlacktopRosterPolicy {
 // The updated build retains stage 1 on Home/Quick Play after Blacktop exit.
 // Keep the old build's original Home/stage-0 boundary; no guessed offsets.
 internal static bool Menu(bool updated,uint a,uint b,uint stage,byte active,byte kind){
  if(a!=b||active!=0||kind!=0)return false;
  if(!updated)return a==0x6509837B&&stage==0;
  return (a==0x6509837B&&(stage==0||stage==1))||(a==0x079964C0&&stage==1);
 }
 internal static bool Release(bool armed,bool entered,bool expired){return armed&&(entered||expired);}
}

// Session worker: arm the stock roster-loader callback at verified inactive
// Home/Quick Play menus. Preserve it through entry until selection completes.
// The stock loader supplies the roster; never append or inject team pointers.
internal static partial class FreestyleTeams {
 static uint BtSlot=0x745e1d8, BtOriginal=0x2340ca0, BtLoader=0x23422a0, BtList=0xaa9e980;
 static uint BtRoot=0xaeb29d0,BtVtable=0x78aaba8,BtDelegate=0x9aa7730,BtDelegateVtable=0x745d4c8;
 static void SelectBlacktopBuild(bool updated){
  BtSlot=updated?0x7461d38U:0x745e1d8U;BtOriginal=updated?0x2342e00U:0x2340ca0U;BtLoader=updated?0x2344400U:0x23422a0U;BtList=updated?0xaaa2980U:0xaa9e980U;
  BtRoot=updated?0xaeb69d0U:0xaeb29d0U;BtVtable=updated?0x78aea48U:0x78aaba8U;BtDelegate=updated?0x9aab730U:0x9aa7730U;BtDelegateVtable=updated?0x7461028U:0x745d4c8U;
  BcTable=updated?0x73ba190U:0x73b6630U;BcLabel=updated?0xac6c6b0U:0xac686b0U;BcContinue=updated?0x22e0a4bU:0x22de8fbU;
  var targets=updated?new uint[]{0x22e0a38,0x22e0a46,0x22e0a3f,0x22e0ac4,0x22e0a9a,0x22e0aa3}:new uint[]{0x22de8e8,0x22de8f6,0x22de8ef,0x22de974,0x22de94a,0x22de953};
  BcOriginal=targets.SelectMany(v=>BitConverter.GetBytes(unchecked((int)(v-BcTable)))).ToArray();
 }
 static volatile bool btStop;
 static void BtEmit(string phase,string message){Console.WriteLine(json.Serialize(new{phase=phase,message=message}));Console.Out.Flush();}
 static uint BtMode(uint rva){uint i=D(Read(basis+rva,4));return i<128?D(Read(basis+FsModeTable+i*64,4)):0;}
 static void BtWrite(ulong at,byte[] value){
  if(!(at==basis+BtSlot&&value.Length==8&&(Q(value)==basis+BtOriginal||Q(value)==basis+BtLoader)))throw new Stop("error","Unexpected Blacktop write rejected.");
  Identity();UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)at,value,(UIntPtr)value.Length,out done)||done.ToUInt64()!=(ulong)value.Length||!Read(at,value.Length).SequenceEqual(value))throw new Stop("write-failed","Blacktop unlock write could not be verified. Close the game.");
 }
 static void BtCallback(uint from,uint to){
  Identity();if(Q(Read(basis+BtSlot,8))!=basis+from)throw new Stop("conflict","Blacktop callback changed externally.");
  var area=Area(basis+BtSlot,8,0x1000000,false);
  if((ulong)area.Allocation.ToInt64()!=basis||area.Protect!=2)throw new Stop("conflict","Unexpected callback protection.");
  uint old;if(!VirtualProtectEx(handle,(IntPtr)(long)(basis+BtSlot),(UIntPtr)8,4,out old))throw new Stop("write-failed","Cannot prepare Blacktop callback.");
  try{BtWrite(basis+BtSlot,BitConverter.GetBytes(basis+to));}
  finally{uint ignored;if(!VirtualProtectEx(handle,(IntPtr)(long)(basis+BtSlot),(UIntPtr)8,old,out ignored))throw new Stop("write-failed","Close the game: callback protection restoration failed.");}
 }
 static bool BtBuild(string image){
  bool updated=SelectFreestyleBuild();SelectBlacktopBuild(updated);
  uint stamp=updated?0x6aa2013dU:0x6a974d13U,sizeOfImage=updated?0x36558000U:0x368c3000U;
  var head=Read(basis,4096);int pe=(int)D(head,60);
  if(head[0]!=77||head[1]!=90||pe<64||pe>3800||D(head,pe)!=0x4550||D(head,pe+8)!=stamp||D(head,pe+80)!=sizeOfImage)throw new Stop("unsupported","Blacktop unlock does not support this game build.");
  // Compare bounded native entry/loader ranges with the validated executable.
  using(var f=File.OpenRead(image))using(var r=new BinaryReader(f)){
   var disk=r.ReadBytes(4096);if(!disk.SequenceEqual(head)){
    // Loaded PE headers may contain loader changes; check identity fields only.
    if(disk.Length!=4096||D(disk,60)!=(uint)pe||D(disk,pe+8)!=stamp||D(disk,pe+80)!=sizeOfImage)throw new Stop("unsupported","Executable identity differs.");
   }
   int count=BitConverter.ToUInt16(disk,pe+6),start=pe+24+BitConverter.ToUInt16(disk,pe+20);
   if(count<1||count>96||start+count*40>4096)throw new Stop("unsupported","Executable section table rejected.");
   var spans=updated?new[]{new uint[]{BtOriginal,16},new uint[]{BtLoader,26},new uint[]{0x32c51d0,0xcb},new uint[]{0x22e09f0,0xfc},new uint[]{0x27bc3d0,0x80}}:new[]{new uint[]{BtOriginal,16},new uint[]{BtLoader,26},new uint[]{0x32c2d30,0xcb},new uint[]{0x22de8a0,0xfc},new uint[]{0x22ded00,0x3e},new uint[]{0x32bba50,0x220},new uint[]{0x27ba210,0x80},new uint[]{0x3008720,0x17}};
   foreach(var span in spans){
    bool found=false;for(int i=0;i<count;i++){int at=start+i*40;uint va=D(disk,at+12),size=D(disk,at+16),offset=D(disk,at+20);
     if(span[0]>=va&&(ulong)span[0]+span[1]<=(ulong)va+size){f.Position=(long)offset+span[0]-va;var bytes=r.ReadBytes((int)span[1]);if(bytes.Length!=(int)span[1]||!bytes.SequenceEqual(Read(basis+span[0],bytes.Length)))throw new Stop("unsupported","Native Blacktop entry differs from the supported executable.");found=true;break;}}
    if(!found)throw new Stop("unsupported","Missing native Blacktop entry.");
   }
  }
  if(Read(basis+BtOriginal,1)[0]!=0xc3||Q(Read(basis+BtDelegate,8))!=basis+BtDelegateVtable)throw new Stop("unsupported","Blacktop delegate is not verified.");
  return updated;
 }
 static bool BtCanArmMenu(bool updated,uint a,uint b,uint stage){
  if(!BlacktopRosterPolicy.Menu(updated,a,b,stage,0,0))return false;
  if(!updated)return true; // Preserve the older build's existing boundary.
  try{
   // These updated-build inactive fields were verified during crowd/ball exit
   // research. A menu hash or stale stage alone is not sufficient to arm.
   byte active=Read(basis+0xAACC310,1)[0];ulong owner=Q(Read(basis+0xAACB9A0,8));
   Area(owner,1,0x20000,false);byte kind=Read(owner,1)[0];
   return BlacktopRosterPolicy.Menu(true,a,b,stage,active,kind)&&
    BtMode(FsModeCurrent)==a&&BtMode(FsModeAlternate)==b&&D(Read(basis+BtRoot+0xea8,4))==stage&&
    Read(basis+0xAACC310,1)[0]==active&&Q(Read(basis+0xAACB9A0,8))==owner&&Read(owner,1)[0]==kind;
  }catch(Stop e){if(e.phase!="waiting")throw;return false;}
 }
 static object RunBlacktop(string[] args){
  long earliest;if(args.Length!=3||!long.TryParse(args[2],out earliest)||earliest<=0)throw new Stop("error","Invalid Blacktop session request.");
  string image=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));bool armed=false;Process game=null;
  btStop=false;var input=new Thread(()=>{try{Console.OpenStandardInput().ReadByte();}catch{}btStop=true;});input.IsBackground=true;input.Start();
  try{
   DateTime deadline=DateTime.UtcNow.AddSeconds(120);BtEmit("waiting","Waiting for a new launcher-managed game session.");
   while(!btStop&&DateTime.UtcNow<deadline){
    var games=Process.GetProcessesByName("NBA2K27");
    if(games.Length==1&&games[0].StartTime.ToUniversalTime().ToFileTimeUtc()>=earliest){game=games[0];break;}
    foreach(var p in games)p.Dispose();Thread.Sleep(250);
   }
   if(game==null)return new{phase="ended",message="Blacktop unlock stopped."};
   if(!String.Equals(Path.GetFullPath(game.MainModule.FileName),image,StringComparison.OrdinalIgnoreCase))throw new Stop("error","Game folder does not match.");
   foreach(var p in Process.GetProcesses()){using(p){if(p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Use the launcher without-EAC session.");}}
   created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();handle=OpenProcess(0x438,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access the game.");
   Identity();bool updated=BtBuild(image);if(Q(Read(basis+BtSlot,8))!=basis+BtOriginal)throw new Stop("conflict","Blacktop callback is already modified.");
   // Dispatch is static image data; labels are populated later by UI startup.
   if(!Read(basis+BcTable,24).SequenceEqual(BcOriginal))throw new Stop("conflict","An existing WNBA category test must finish before starting this launcher.");
   DateTime entryDeadline=DateTime.MinValue;
   while(!btStop&&!game.HasExited){
    Identity();uint a=BtMode(FsModeCurrent),b=BtMode(FsModeAlternate);
    // Stages 1/2 announce entry before the selection screen has finished loading.
    // Do not undo the roster-loader callback during that transition.
    uint stage=D(Read(basis+BtRoot+0xea8,4));
    bool menu=BtCanArmMenu(updated,a,b,stage);
    bool entered=BlacktopPresentationPolicy.RosterEntryComplete(a,b,stage);
    if(BlacktopRosterPolicy.Release(armed,entered,DateTime.UtcNow>=entryDeadline)){BtCallback(BtLoader,BtOriginal);armed=false;}
    if(menu&&!armed&&BtCanArmMenu(updated,a,b,stage)){
     BcRestore();BtCallback(BtOriginal,BtLoader);armed=true;entryDeadline=DateTime.UtcNow.AddSeconds(60);
     BtEmit("ready","WNBA roster loader armed from "+(a==0x079964C0?"Quick Play":"Home")+" for the next Blacktop entry.");
    }
    if(!armed){try{BcTick();}catch(Stop e){if(e.phase!="waiting")throw;BcRestore();}}
    Thread.Sleep(100);
   }
   return new{phase="ended",message="Blacktop unlock session ended."};
  }finally{
   if(handle!=IntPtr.Zero){try{if(game!=null&&!game.HasExited){try{BcRestore();}finally{if(armed&&Q(Read(basis+BtSlot,8))==basis+BtLoader)BtCallback(BtLoader,BtOriginal);}}}finally{CloseHandle(handle);handle=IntPtr.Zero;}}
   if(game!=null)game.Dispose();
  }
 }
}
