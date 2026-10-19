using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Threading;
using System.Web.Script.Serialization;

// Standalone developer snapshot, never packaged with the launcher. This file
// has no game-write import or routine and requests only QUERY_INFORMATION/READ.
internal static class LiveCrowdDiagnostics {
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,int pid);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
 [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr handle,IntPtr address,byte[] bytes,UIntPtr length,out UIntPtr done);
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr handle,out long created,out long exit,out long kernel,out long user);
 static IntPtr handle;
 static ulong basis;
 static long created;
 const ulong OwnerRva=0xAA51F50, WindowOffset=0x2C48;
 const string GetterHex="4883EC28E85700E1FF837808007F1E803D7A134108007515488B05B16F3908803801751C4883C428E9C39B3E01488B05EC0941080FB680F90C00004883C428C30FB680492C00004883C428C3";
 static byte[] Read(ulong address,int length){
  if(address<0x10000||address>0x7fffffffffffUL-(ulong)length||length<1||length>4096)throw new Exception("Snapshot address is outside the read bounds.");
  var bytes=new byte[length];UIntPtr done;
  if(!ReadProcessMemory(handle,(IntPtr)(long)address,bytes,(UIntPtr)length,out done)||done.ToUInt64()!=(ulong)length)throw new Exception("Snapshot memory is not readable.");
  return bytes;
 }
 static void Identity(){long c,e,k,u;if(!GetProcessTimes(handle,out c,out e,out k,out u)||c!=created||e!=0)throw new Exception("Exact game session has ended.");}
 static uint Mode(ulong rva){uint index=BitConverter.ToUInt32(Read(basis+rva,4),0);if(index>=128)throw new Exception("Mode is initializing.");return BitConverter.ToUInt32(Read(basis+0xAC77660+(ulong)index*64,4),0);}
 static void VerifyLoad(ulong rva,int offset){if(!Read(basis+rva,7).SequenceEqual(new byte[]{15,182,128,(byte)(offset&255),(byte)(offset>>8),0,0}))throw new Exception("Candidate crowd getter layout differs.");}
 static object Capture(){
  Identity();ulong owner=BitConverter.ToUInt64(Read(basis+OwnerRva,8),0);
  bool valid=owner>=0x10000&&owner<=0x7fffffffffffUL-WindowOffset-16;
  byte? tag=null,alternateCrowd=null;byte[] window=null;string readError=null;
  if(valid){try{tag=Read(owner,1)[0];window=Read(owner+WindowOffset,16);alternateCrowd=Read(owner+0xCF9,1)[0];}catch(Exception e){readError=e.Message;}}
  uint current=Mode(0xAC77E28),alternate=Mode(0xAC77E24),stage=BitConverter.ToUInt32(Read(basis+0xAEB69D0+0xEA8,4),0);
  byte active=Read(basis+0xAACC310,1)[0];ulong kindPointer=BitConverter.ToUInt64(Read(basis+0xAACB9A0,8),0);byte kind=Read(kindPointer,1)[0];
  byte[] gameWindow=Read(kindPointer+0xCF8,16);
  // The court-setup routine at 0x1910E40 consumes this small override
  // object before its front-row/drone/DJ/MC GameData setters. Read only:
  // presence is not proof that the routine will run for this Blacktop court.
  ulong courtSetup=BitConverter.ToUInt64(Read(basis+0xA9CAEB0,8),0);
  byte courtSetupActive=Read(basis+0xA9CB670,1)[0];
  ulong actorOverride=0;byte[] actorOverrideWindow=null;string actorOverrideError=null;
  if(courtSetup>=0x10000&&courtSetup<=0x7fffffffffffUL-0x40){
   try{actorOverride=BitConverter.ToUInt64(Read(courtSetup+0x38,8),0);
    if(actorOverride>=0x10000&&actorOverride<=0x7fffffffffffUL-24)actorOverrideWindow=Read(actorOverride,24);
   }catch(Exception e){actorOverrideError=e.Message;}
  }
  ulong ruleOverrides=BitConverter.ToUInt64(Read(basis+0xAACAFF0,8),0);
  ulong preloadTable=BitConverter.ToUInt64(Read(basis+0xAACAFE8,8),0);
  uint preloadPass=BitConverter.ToUInt32(Read(basis+0xAACAFF8,4),0),preloadIndex=BitConverter.ToUInt32(Read(basis+0xAACAFFC,4),0);
  ulong? derivedRow=null;uint? derivedTag=null;byte[] derivedFront=null;
  var candidateRows=new System.Collections.Generic.List<object>();
  // Diagnostic candidates only, not approved write addresses. The offline
  // initializer copies 310 records with stride C810; inspect a bounded subset.
  if(preloadTable>=0x10000&&preloadTable<=0x7fffffffffffUL-0xF24368){
   foreach(uint index in new uint[]{0,175,294,295,296,297,298,299,300,301,302,303,304,305,306,307,308,309}){
    ulong row=preloadTable+(ulong)index*0xC810+8;
    try{candidateRows.Add(new {index=index,row=row.ToString("X"),tag=BitConverter.ToUInt32(Read(row,4),0),front=Read(row+0x7428,24)});}catch(Exception e){candidateRows.Add(new {index=index,error=e.Message});}
   }
  }
  if(preloadTable>=0x10000&&preloadIndex<512&&preloadTable<=0x7fffffffffffUL-((ulong)preloadIndex*0xC810+0x7450)){
   derivedRow=preloadTable+(ulong)preloadIndex*0xC810+8;
   try{derivedTag=BitConverter.ToUInt32(Read(derivedRow.Value,4),0);derivedFront=Read(derivedRow.Value+0x7428,24);}catch(Exception){}
  }
  uint? ruleOverrideTag=null;byte[] ambientOverrideWindow=null,frontRowOverrideWindow=null;
  string ruleOverrideError=null;
  if(ruleOverrides>=0x10000&&ruleOverrides<=0x7fffffffffffUL-0x7448){
   try{ruleOverrideTag=BitConverter.ToUInt32(Read(ruleOverrides,4),0);
    ambientOverrideWindow=Read(ruleOverrides+0x73F8,24);
    frontRowOverrideWindow=Read(ruleOverrides+0x7428,24);
   }catch(Exception e){ruleOverrideError=e.Message;}
  }
  bool stable=owner==BitConverter.ToUInt64(Read(basis+OwnerRva,8),0);
  Identity();
  return new { owner=owner.ToString("X"),ownerValid=valid,ownerTag=tag,window=window,crowd=window==null?(byte?)null:window[1],alternateCrowd=alternateCrowd,readError=readError,
   globalAmbient=window==null?(byte?)null:window[3],globalFrontRowActors=window==null?(byte?)null:window[5],
   gameOwner=kindPointer.ToString("X"),gameWindow=gameWindow,gameCrowd=gameWindow[1],gameAmbient=gameWindow[3],gameFrontRowActors=gameWindow[5],
   courtSetup=courtSetup.ToString("X"),courtSetupActive=courtSetupActive,
   actorOverride=actorOverride.ToString("X"),actorOverrideWindow=actorOverrideWindow,actorOverrideError=actorOverrideError,
   ruleOverrides=ruleOverrides.ToString("X"),ruleOverrideTag=ruleOverrideTag,ambientOverrideWindow=ambientOverrideWindow,
   preloadTable=preloadTable.ToString("X"),preloadPass=preloadPass,preloadIndex=preloadIndex,derivedRow=derivedRow.HasValue?derivedRow.Value.ToString("X"):null,derivedTag=derivedTag,derivedFront=derivedFront,
   preloadCandidateRows=candidateRows,
   frontRowOverrideWindow=frontRowOverrideWindow,ruleOverrideError=ruleOverrideError,
   current=current.ToString("X8"),alternate=alternate.ToString("X8"),stage=stage,active=active,kind=kind,
   selection=current==0x92DF06DE&&alternate==current&&stage==3&&active==0&&kind==0,ownerStable=stable };
 }
 public static int Main(string[] args){
  var json=new JavaScriptSerializer();
  try{
   int pid;long expectedBirth;
   if(args.Length!=3||!int.TryParse(args[0],out pid)||pid<=0||!long.TryParse(args[2],out expectedBirth))throw new Exception("Exact PID, game image and process birth time required.");
   foreach(var candidate in Process.GetProcesses()){using(candidate){if(candidate.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Exception("Anti-cheat is running; snapshot cancelled.");}}
   using(var game=Process.GetProcessById(pid)){
    if(!game.ProcessName.Equals("NBA2K27",StringComparison.OrdinalIgnoreCase)||!Path.GetFullPath(game.MainModule.FileName).Equals(Path.GetFullPath(args[1]),StringComparison.OrdinalIgnoreCase))throw new Exception("Unexpected game process or image.");
    created=game.StartTime.ToUniversalTime().ToFileTimeUtc();if(created!=expectedBirth)throw new Exception("Process identity changed.");
    basis=(ulong)game.MainModule.BaseAddress.ToInt64();handle=OpenProcess(0x410U,false,pid);if(handle==IntPtr.Zero)throw new Exception("Read-only process access unavailable.");
    Identity();var header=Read(basis,4096);int pe=BitConverter.ToInt32(header,60);
    if(header[0]!=77||header[1]!=90||pe<64||pe>3800||BitConverter.ToUInt32(header,pe)!=0x4550||BitConverter.ToUInt16(header,pe+4)!=0x8664||BitConverter.ToUInt32(header,pe+8)!=0x6AA2013D||BitConverter.ToUInt32(header,pe+80)!=0x36558000)throw new Exception("Unsupported game build; no settings read.");
    byte[] getter=Enumerable.Range(0,GetterHex.Length/2).Select(i=>Convert.ToByte(GetterHex.Substring(i*2,2),16)).ToArray();
    if(!Read(basis+0x26BAF80,getter.Length).SequenceEqual(getter))throw new Exception("Crowd getter signature differs; snapshot cancelled.");
    VerifyLoad(0x26BB7F4,0xCFB);VerifyLoad(0x26BB800,0x2C4B);
    VerifyLoad(0x26BC034,0xCFD);VerifyLoad(0x26BC040,0x2C4D);
    if(!Read(basis+0x1910FCE,7).SequenceEqual(new byte[]{0x48,0x8B,0x05,0xDB,0x9E,0x0B,0x09})||
       !Read(basis+0x1910FDB,6).SequenceEqual(new byte[]{0x22,0x0D,0x8F,0xA6,0x0B,0x09})||
       !Read(basis+0x19110EF,9).SequenceEqual(new byte[]{0x0F,0xB6,0x4F,0x01,0xBA,0xA1,0xC8,0x34,0x72}))throw new Exception("Court-setup override signature differs.");
    if(!Read(basis+0x24C8D79,7).SequenceEqual(new byte[]{0x48,0x8B,0x35,0x70,0x22,0x60,0x08})||
       !Read(basis+0x3B90E68,7).SequenceEqual(new byte[]{0x0F,0xB6,0x87,0xF9,0x73,0,0})||
       !Read(basis+0x3B91032,7).SequenceEqual(new byte[]{0x0F,0xB6,0x87,0x29,0x74,0,0}))throw new Exception("Rule-override source signature differs.");
    var first=Capture();Thread.Sleep(300);var second=Capture();
    Console.WriteLine(json.Serialize(new { readOnly=true,writes=0,pid=pid,created=created.ToString(),buildVerified=true,getterVerified=true,first=first,second=second }));
   }
   return 0;
  }catch(Exception e){Console.WriteLine(json.Serialize(new { readOnly=true,writes=0,error=e.Message }));return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}}
 }
}
