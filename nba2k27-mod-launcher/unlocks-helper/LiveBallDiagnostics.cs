using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;

// Developer-only entry point. Opens the exact process with read/query rights;
// never dispatched by the packaged helper and never calls any write path.
internal static partial class FreestyleTeams {
 const uint BallDiagnosticRenderer=0xAA7B078;
 static object BallDiagnosticPlayers(){
  byte[] table=Read(basis+0xAACB9B0,30*0x50);var rows=new List<object>();
  for(int i=0;i<30;i++){
   int offset=i*0x50;ulong vt=Q(table,offset),manager=Q(table,offset+0x10),roster=Q(table,offset+0x30);
   if(manager==0&&roster==0)continue;
   rows.Add(new{slot=i,wrapperRva=(vt-basis).ToString("X"),manager=manager.ToString("X"),roster=roster.ToString("X"),
    actor=BallDiagnosticAttempt(()=>{ulong character=Q(Read(manager+0x98,8));return new{managerRva=(Q(Read(manager,8))-basis).ToString("X"),character=character.ToString("X"),characterRva=(Q(Read(character,8))-basis).ToString("X"),scale=BitConverter.ToSingle(Read(character+0xBCC,4),0),rosterManager=Q(Read(roster+0x518,8)).ToString("X")};})});
  }
  return new{input=BitConverter.ToSingle(Read(basis+0xA39319C,4),0),tableStable=table.SequenceEqual(Read(basis+0xAACB9B0,table.Length)),rows=rows};
 }
 static object BallDiagnosticAttempt(Func<object> read){try{return read();}catch(Exception e){return new{error=e.Message};}}
 static object BallDiagnosticModels(){
  ulong registry=Q(Read(basis+BallSelectorPolicy.RegistryRva,8));
  uint count=D(Read(basis+BallSelectorPolicy.RegistryRva+8,4));
  if(registry<0x10000||count<1||count>1024)throw new Exception("Ball registry not ready.");
  ulong holder=Q(Read(basis+BallSelectorPolicy.RecordHolderRva,8));
  if(holder<0x10000)throw new Exception("Ball records not ready.");
  ulong records=Q(Read(holder+0x18,8))+BallSelectorPolicy.RecordOffset;
  var rows=new List<object>();byte[] entries=Read(registry,(int)count*8);
  for(int i=0;i<count;i++){
   uint model=D(entries,i*8),index=D(entries,i*8+4);
   // The exact-build lookup rejects indices >= 500.
   if(index>=500){rows.Add(new{modelId=model.ToString("X8"),index=index,error="Out-of-range record index"});continue;}
   byte[] record=Read(records+index*BallSelectorPolicy.RecordStride,BallSelectorPolicy.RecordStride);
   string asset=Encoding.ASCII.GetString(record,BallSelectorPolicy.RecordAssetOffset,record.Length-BallSelectorPolicy.RecordAssetOffset).Split('\0')[0];
   rows.Add(new{modelId=model.ToString("X8"),index=index,recordMatches=D(record,4)==model,asset=asset,
    // Preserve raw resource-slot bytes, without claiming that a pointer alone
    // proves the mesh is loaded or visible.
    resourceSlot=BitConverter.ToString(Read(basis+BallDiagnosticRenderer+0x28+index*48,48))});
  }
  return rows;
 }
 static object BallDiagnosticSample(){
  Identity();
  var modes=new[]{BallSelectorPolicy.ModeCurrentRva,BallSelectorPolicy.ModeAlternateRva}.Select(r=>{
   uint index=D(Read(basis+r,4));return new{index=index,hash=index<128?D(Read(basis+BallSelectorPolicy.ModeTableRva+index*64,4)).ToString("X8"):"invalid"};
  }).ToArray();
  object renderer=BallDiagnosticAttempt(()=>{
   ulong manager=basis+BallDiagnosticRenderer;byte[] header=Read(manager,0x28);
   int count=(int)D(header,8),capacity=(int)D(header,12);ulong queue=Q(Read(manager+0x5de8,8));
   if(count<0||capacity<0||count>capacity||capacity>8192)throw new Exception("Unstable or invalid render queue bounds.");
   var packets=new List<object>();
   if(count>0){
    if(queue<0x10000)throw new Exception("Render queue unavailable.");
    byte[] data=Read(queue,count*0xa0);
    for(int i=0;i<count;i++){
     int offset=i*0xa0;double[] lengths=new double[3];
     for(int row=0;row<3;row++)for(int col=0;col<3;col++){
      double value=BitConverter.ToSingle(data,offset+row*16+col*4);lengths[row]+=value*value;
     }
     packets.Add(new{modelId=D(data,offset+0x48).ToString("X8"),resourceSlot=Q(data,offset+0x40).ToString("X"),
      basisLengths=lengths.Select(v=>Double.IsNaN(v)||Double.IsInfinity(v)?"invalid":Math.Sqrt(v).ToString("R",System.Globalization.CultureInfo.InvariantCulture)).ToArray()});
    }
   }
   ulong vtable=Q(header);byte[] methods=Read(vtable,27*8);
   return new{enabled=header[0x20],count=count,capacity=capacity,queue=queue.ToString("X"),vtableRva=(vtable-basis).ToString("X"),
    methods=Enumerable.Range(0,27).Select(i=>new{offset=(i*8).ToString("X"),rva=(Q(methods,i*8)-basis).ToString("X")}).ToArray(),
    headerStable=header.SequenceEqual(Read(manager,0x28))&&queue==Q(Read(manager+0x5de8,8)),packets=packets};
  });
  Identity();
  return new{time=DateTime.UtcNow.ToString("o"),modes=modes,stage=D(Read(basis+BallSelectorPolicy.BlacktopStageRva,4)),
   scope=BallDiagnosticAttempt(()=>BallScope()),gameData=BallDiagnosticAttempt(()=>new{
    active=Read(basis+0xAACC310,1)[0],kind=Read(Q(Read(basis+0xAACB9A0,8)),1)[0],
    mode=D(Read(Q(Read(basis+0xAACB9A0,8))+0xA04,4))}),
   hook=BitConverter.ToString(Read(basis+BallSelectorPolicy.HookRva,7)),renderer=renderer};
 }
 internal static int InspectLiveBall(string[] args){
  try{
   int pid;if(args.Length!=2||!int.TryParse(args[0],out pid)||pid<=0)throw new Exception("Usage: LiveBallDiagnostics <exact PID> <ball journal path>");
   using(var game=Process.GetProcessById(pid)){
    if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected process.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
    handle=OpenProcess(0x410,false,pid);if(handle==IntPtr.Zero)throw new Exception("Read-only process access unavailable.");
    Identity();if(!SelectFreestyleBuild())throw new Exception("This diagnostic requires the September game build.");
    json.MaxJsonLength=16*1024*1024;
    // Verify the actual renderer reference, not just an assumed data address.
    byte[] lea=Read(basis+0x21BCC3A,7);
    if(lea[0]!=0x48||lea[1]!=0x8d||lea[2]!=0x0d||0x21BCC41L+BitConverter.ToInt32(lea,3)!=BallDiagnosticRenderer)throw new Exception("Unexpected ball renderer layout.");
    object journal=BallDiagnosticAttempt(()=>{
     var saved=BallLoad(Path.GetFullPath(args[1]));
     if(saved==null)return new{present=false};
     bool same=BallSame(saved,pid,game.MainModule.FileName);
     return new{present=true,sameProcess=same,schema=saved.schema,phase=saved.phase,selection=saved.logicalPath,
      savedModel=saved.modelId.ToString("X8"),liveModel=same?D(Read(saved.control,4)).ToString("X8"):null,
      requestedModel=same&&saved.schema==2?D(Read(saved.control+8,4)).ToString("X8"):null,
      loadHook=BitConverter.ToString(Read(basis+BallSelectorPolicy.LoadHookRva,5)),releaseHook=BitConverter.ToString(Read(basis+BallSelectorPolicy.ReleaseHookRva,5))};
    });
    object modelsBefore=BallDiagnosticAttempt(()=>BallDiagnosticModels());
    var samples=new List<object>();
    for(int i=0;i<30;i++){samples.Add(BallDiagnosticSample());if(i<29)Thread.Sleep(50);}
    Console.WriteLine(json.Serialize(new{schema="ball-lifecycle-readonly-v1",pid=pid,created=created.ToString(),basis=basis.ToString("X"),
     readOnly=true,writes=0,journal=journal,modelsBefore=modelsBefore,samples=samples,modelsAfter=BallDiagnosticAttempt(()=>BallDiagnosticModels()),players=BallDiagnosticAttempt(()=>BallDiagnosticPlayers())}));
   }
   return 0;
  }catch(Exception e){Console.WriteLine(json.Serialize(new{error=e.Message,readOnly=true,writes=0}));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
}
internal static class LiveBallDiagnosticEntry {public static int Main(string[] args){return FreestyleTeams.InspectLiveBall(args);}}
