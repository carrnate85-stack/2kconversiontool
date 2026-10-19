using System;
using System.Diagnostics;
using System.Linq;

// Read-only diagnostic entry point. Never dispatched by the production helper.
internal static partial class FreestyleTeams {
 internal static int InspectLiveSelection(string[] args){
  try {
   int pid;if(args.Length!=1||!int.TryParse(args[0],out pid))throw new Exception("Exact game PID required.");
   using(var game=Process.GetProcessById(pid)){
    if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected process.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
    handle=OpenProcess(0x410,false,pid);if(handle==IntPtr.Zero)throw new Exception("Read-only process access unavailable.");
    Identity();SelectFreestyleBuild();
    ulong root=Q(Read(basis+FsRosterRoot,8)),owner=Q(Read(root+0xa8,8));byte[] header=Read(owner+0x80,16);
    int count=(int)D(header);ulong table=Q(header,8);if(count<1||count>1024)throw new Exception("Roster not ready.");
    byte[] rows=Read(table,count*FreestylePolicy.Stride);
    var teams=Enumerable.Range(0,count).Select(i=>new{index=i,id=BitConverter.ToUInt16(rows,i*FreestylePolicy.Stride+0xf20),kind=D(rows,i*FreestylePolicy.Stride+0x123c)>>26,name=System.Text.Encoding.Unicode.GetString(rows,i*FreestylePolicy.Stride+0x2fa,48).Split('\0')[0]}).ToArray();
    var selected=new[]{FsSelected,FsOtherSelected}.Select(r=>{ulong p=Q(Read(basis+r,8));int index=p>=table&&p<table+(ulong)rows.Length&&(p-table)%FreestylePolicy.Stride==0?(int)((p-table)/FreestylePolicy.Stride):-1;return new{index=index,team=index<0?null:teams[index],category=D(Read(basis+r+8,4))};}).ToArray();
    var modes=new[]{FsModeCurrent,FsModeAlternate}.Select(r=>{uint i=D(Read(basis+r,4));return new{index=i,hash=i<128?D(Read(basis+FsModeTable+i*64,4)).ToString("X8"):"invalid"};}).ToArray();
    string chosen;try{chosen=BallResolve("balls/ball_gameplay_official_wnba_allstarw_threepoint.iff").ToString("X8");}catch(Exception e){chosen=e.Message;}
    Identity();if(!header.SequenceEqual(Read(owner+0x80,16)))throw new Exception("Roster changed during inspection.");
    Console.WriteLine(json.Serialize(new{pid=pid,readOnly=true,writes=0,teamCount=count,wnba=teams.Where(t=>t.kind==29).ToArray(),selected=selected,modes=modes,mask=Q(Read(basis+FsMask,8)).ToString("X"),cycles=CategoryTables(),blacktopStage=D(Read(basis+BallSelectorPolicy.BlacktopStageRva,4)),ballScope=BallScope(),ballHook=BitConverter.ToString(Read(basis+BallSelectorPolicy.HookRva,7)),selectedBallModel=chosen}));
   }return 0;
  }catch(Exception e){Console.WriteLine(json.Serialize(new{error=e.Message,readOnly=true,writes=0}));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
}
internal static class LiveSelectionDiagnosticEntry {public static int Main(string[] args){return FreestyleTeams.InspectLiveSelection(args);}}
