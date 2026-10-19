using System;
using System.Diagnostics;
using System.Linq;

// Developer-only, read-only capture. Not included in the packaged helper.
internal static partial class FreestyleTeams {
 internal static int InspectLivePresentation(string[] args){
  try{
   int pid;
   if(args.Length!=1||!int.TryParse(args[0],out pid)||pid<=0)throw new Exception("Exact NBA 2K27 PID required.");
   using(var game=Process.GetProcessById(pid)){
    if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected process.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
    handle=OpenProcess(0x410,false,pid);if(handle==IntPtr.Zero)throw new Exception("Read-only process access unavailable.");
    Identity();PresentationBuild();var first=PresentationRead();var second=PresentationRead();
    if(first.owner!=second.owner||!first.values.SequenceEqual(second.values))throw new Exception("Presentation state changed during inspection.");
    Console.WriteLine(json.Serialize(new{pid=pid,readOnly=true,writes=0,owner=first.owner.ToString("X"),values=first.values,
     current=first.current.ToString("X8"),alternate=first.alternate.ToString("X8"),stage=first.stage,active=first.active,kind=first.kind,
     selection=first.Selection,canApply=first.CanApply}));
   }
   return 0;
  }catch(Exception e){Console.WriteLine(json.Serialize(new{error=e.Message,readOnly=true,writes=0}));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
}
internal static class LivePresentationDiagnosticEntry {public static int Main(string[] args){return FreestyleTeams.InspectLivePresentation(args);}}
