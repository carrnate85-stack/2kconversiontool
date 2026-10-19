using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;

// Reads Windows session volume; --duck temporarily mutes only verified game sessions.
// Never writes volume levels, Jukebox settings, master output or game memory.
class GameVolume {
 [ComImport,Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class Enumerator {}
 [ComImport,Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IEnum {
  [PreserveSig]int EnumAudioEndpoints(int flow,uint state,out IDevices collection);
 }
 [ComImport,Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IDevices {
  [PreserveSig]int GetCount(out uint count); [PreserveSig]int Item(uint index,out IDevice device);
 }
 [ComImport,Guid("D666063F-1587-4E43-81F1-B948E807363F"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IDevice {
  [PreserveSig]int Activate(ref Guid iid,uint clsctx,IntPtr parameters,[MarshalAs(UnmanagedType.IUnknown)]out object value);
 }
 [ComImport,Guid("77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IManager {
  [PreserveSig]int GetAudioSessionControl(IntPtr guid,uint flags,out IntPtr control);
  [PreserveSig]int GetSimpleAudioVolume(IntPtr guid,uint flags,out IntPtr volume);
  [PreserveSig]int GetSessionEnumerator(out ISessions sessions);
 }
 [ComImport,Guid("E2F5BB11-0570-40CA-ACDD-3AA01277DEE8"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface ISessions {
  [PreserveSig]int GetCount(out int count); [PreserveSig]int GetSession(int index,out IControl control);
 }
 [ComImport,Guid("F4B1A599-7266-4319-A8CA-E70ACB11E8CD"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IControl {}
 [ComImport,Guid("BFB7FF88-7239-4FC9-8FA2-07C950BE9C6D"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IControl2 {
  [PreserveSig]int GetState(out int state); [PreserveSig]int GetDisplayName(out IntPtr name);
  [PreserveSig]int ReservedDisplayName(IntPtr name,IntPtr context); [PreserveSig]int GetIconPath(out IntPtr path); [PreserveSig]int ReservedIconPath(IntPtr path,IntPtr context);
  [PreserveSig]int GetGroupingParam(out Guid grouping); [PreserveSig]int ReservedGrouping(IntPtr grouping,IntPtr context);
  [PreserveSig]int RegisterNotification(IntPtr client); [PreserveSig]int UnregisterNotification(IntPtr client);
  [PreserveSig]int GetSessionIdentifier(out IntPtr id); [PreserveSig]int GetSessionInstanceIdentifier(out IntPtr id); [PreserveSig]int GetProcessId(out uint pid);
 }
 [ComImport,Guid("87CE5498-68D6-44E5-9215-6DA47EF883D8"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)] interface IVolume {
  // The volume setter slot is never called: the user's volume stays unchanged.
  [PreserveSig]int ReservedVolume(float level,IntPtr context); [PreserveSig]int GetMasterVolume(out float level);
  [PreserveSig]int SetMute([MarshalAs(UnmanagedType.Bool)]bool mute,ref Guid context); [PreserveSig]int GetMute([MarshalAs(UnmanagedType.Bool)]out bool mute);
 }
 static volatile bool stop;
#if AUDIO_TEST
 static bool? testMute;
#endif
 static void Emit(string value){try{Console.WriteLine(value);}catch(IOException){stop=true;}}
 static void Release(object value){if(value!=null&&Marshal.IsComObject(value))Marshal.ReleaseComObject(value);}
 sealed class Sample {public float Level;public bool Muted;public int State;}
 sealed class MuteLease : IDisposable {
  readonly Func<bool> read;readonly Action<bool> write;readonly Action release;
  internal readonly bool OriginalMute;bool owns,disposed;
  internal MuteLease(Func<bool> get,Action<bool> set,Action free){read=get;write=set;release=free;OriginalMute=read();}
  internal void Apply(){if(!OriginalMute){owns=true;write(true);}}
  internal bool Observe(){bool current=read();if(owns&&!current)owns=false;return owns?OriginalMute:current;}
  internal void Restore(){if(owns){if(read())write(OriginalMute);owns=false;}}
  public void Dispose(){if(disposed)return;disposed=true;release();}
 }
 static string Instance(IControl2 control){IntPtr text;Marshal.ThrowExceptionForHR(control.GetSessionInstanceIdentifier(out text));try{return Marshal.PtrToStringUni(text);}finally{Marshal.FreeCoTaskMem(text);}}
 static string Read(string gamePath,uint diagnosticPid=0,Dictionary<string,MuteLease> leases=null){
  var pids=new HashSet<uint>();
  foreach(var process in Process.GetProcessesByName("NBA2K27"))using(process){
   try{if(String.Equals(Path.GetFullPath(process.MainModule.FileName),gamePath,StringComparison.OrdinalIgnoreCase))pids.Add((uint)process.Id);}catch{}
  }
  if(diagnosticPid>0)pids.Add(diagnosticPid);
  if(pids.Count==0)return "{\"event\":\"volume\",\"found\":false}";
  object enumerator=null;IDevices devices=null;var samples=new List<Sample>();
  try{
   enumerator=new Enumerator();Marshal.ThrowExceptionForHR(((IEnum)enumerator).EnumAudioEndpoints(0,1,out devices));uint count;
   Marshal.ThrowExceptionForHR(devices.GetCount(out count));if(count>128)throw new Exception("Unexpected audio endpoint count.");
   for(uint d=0;d<count;d++){
    IDevice device=null;object manager=null;ISessions sessions=null;
    try{
     Marshal.ThrowExceptionForHR(devices.Item(d,out device));Guid iid=typeof(IManager).GUID;
     Marshal.ThrowExceptionForHR(device.Activate(ref iid,23,IntPtr.Zero,out manager));
     Marshal.ThrowExceptionForHR(((IManager)manager).GetSessionEnumerator(out sessions));int n;
     Marshal.ThrowExceptionForHR(sessions.GetCount(out n));if(n<0||n>4096)continue;
     for(int i=0;i<n;i++){
      IControl control=null;bool retained=false;
      try{
       if(sessions.GetSession(i,out control)<0)continue;var c=(IControl2)control;uint pid;int state;
       if(c.GetProcessId(out pid)<0||!pids.Contains(pid)||c.GetState(out state)<0||state==2)continue;
       float level;bool muted;var volume=(IVolume)control;
       if(volume.GetMasterVolume(out level)<0||volume.GetMute(out muted)<0||float.IsNaN(level)||level<0||level>1)continue;
#if AUDIO_TEST
       if(testMute.HasValue){Guid testContext=Guid.Empty;Marshal.ThrowExceptionForHR(volume.SetMute(testMute.Value,ref testContext));muted=testMute.Value;}
#endif
       if(leases!=null){
        string identity=Instance(c);if(String.IsNullOrEmpty(identity)||identity.Length>4096)throw new Exception("Invalid game audio session identity.");
        MuteLease lease;
        if(!leases.TryGetValue(identity,out lease)){
         if(leases.Count>=64)throw new Exception("Too many game audio sessions.");
         IControl owned=control;IVolume ownedVolume=volume;Guid context=new Guid("75C18C26-3154-4A83-9273-1842F7D7F371");
         lease=new MuteLease(delegate{bool flag;Marshal.ThrowExceptionForHR(ownedVolume.GetMute(out flag));return flag;},delegate(bool flag){Marshal.ThrowExceptionForHR(ownedVolume.SetMute(flag,ref context));},delegate{Release(owned);});
         leases.Add(identity,lease);retained=true;lease.Apply();Emit("{\"event\":\"game-audio-held\"}");
        }
        // Our temporary mute must not be mirrored back onto the intro video.
        muted=lease.Observe();
       }
       samples.Add(new Sample{Level=level,Muted=muted,State=state});
      }catch(COMException){if(leases!=null)throw;}finally{if(!retained)Release(control);}
     }
    }catch(COMException){if(leases!=null)throw;}finally{Release(sessions);Release(manager);Release(device);}
   }
  }finally{Release(devices);Release(enumerator);}
  if(samples.Count==0)return "{\"event\":\"volume\",\"found\":false}";
  bool active=samples.Exists(s=>s.State==1),mute=false;float value=1;
  // Prefer active sessions; if several outputs differ, use the quieter level.
  foreach(var sample in samples){if(active&&sample.State!=1)continue;value=Math.Min(value,sample.Level);mute|=sample.Muted;}
  return "{\"event\":\"volume\",\"found\":true,\"volume\":"+value.ToString("R",CultureInfo.InvariantCulture)+",\"muted\":"+(mute?"true":"false")+"}";
 }
 static void Restore(Dictionary<string,MuteLease> leases){
  if(leases==null)return;
  foreach(var lease in leases.Values){
   bool restored=false;
   try{for(int attempt=0;attempt<5&&!restored;attempt++){try{lease.Restore();restored=true;}catch{Thread.Sleep(100);}}}
   finally{lease.Dispose();}
   Emit(restored?"{\"event\":\"game-audio-restored\"}":"{\"event\":\"game-audio-restore-failed\"}");
  }
 }
 static int SelfTest(){
  bool muted=false;int changes=0;var lease=new MuteLease(()=>muted,v=>{muted=v;changes++;},()=>{});
  lease.Apply();if(!muted||lease.Observe())return 1;lease.Restore();if(muted||changes!=2)return 2;lease.Restore();if(changes!=2)return 3;
  muted=true;changes=0;lease=new MuteLease(()=>muted,v=>{muted=v;changes++;},()=>{});lease.Apply();if(!lease.Observe())return 4;lease.Restore();if(!muted||changes!=0)return 5;
  muted=false;lease=new MuteLease(()=>muted,v=>muted=v,()=>{});lease.Apply();muted=false;lease.Observe();lease.Restore();if(muted)return 6;
  Console.WriteLine("PASS: mute, original mute, idempotent restoration and user unmute ownership");return 0;
 }
 [MTAThread] static int Main(string[] args){try{
  if(args.Length==1&&args[0]=="--self-test")return SelfTest();
#if AUDIO_TEST
  uint testPid;if(args.Length==3&&args[0]=="--test-mute"&&uint.TryParse(args[1],out testPid)&&testPid>0&&(args[2]=="on"||args[2]=="off")){testMute=args[2]=="on";Console.WriteLine(Read("",testPid));return 0;}
#endif
  uint probePid;if(args.Length==2&&args[0]=="--probe-pid"&&uint.TryParse(args[1],out probePid)&&probePid>0){Console.WriteLine(Read("",probePid));return 0;}
  if(args.Length==2&&args[0]=="--probe"){Console.WriteLine(Read(Path.GetFullPath(args[1])));return 0;}
  int pid;bool duck=args.Length==3&&args[0]=="--duck";uint targetPid=0;
#if AUDIO_TEST
  bool testing=args.Length==3&&args[0]=="--duck-test"&&uint.TryParse(args[2],out targetPid)&&targetPid>0;
  duck|=testing;
#else
  bool testing=false;
#endif
  if(args.Length!=3||(!duck&&args[0]!="--watch")||!int.TryParse(args[1],out pid)||pid<=0)return 64;
  string gamePath=testing?"":Path.GetFullPath(args[2]);if(!testing&&!String.Equals(Path.GetFileName(gamePath),"NBA2K27.exe",StringComparison.OrdinalIgnoreCase))return 64;
  using(var parent=Process.GetProcessById(pid)){
   ThreadPool.QueueUserWorkItem(delegate{try{while(Console.ReadLine()!=null){}}catch(IOException){}finally{stop=true;}});
   var clock=Stopwatch.StartNew();string previous=null;
   var leases=duck?new Dictionary<string,MuteLease>():null;
   try{while(!stop&&!parent.HasExited&&clock.ElapsedMilliseconds<615000){
    string row;try{row=Read(gamePath,targetPid,leases);}catch{if(duck)throw;row="{\"event\":\"volume\",\"found\":false}";}
    if(row!=previous){Emit(row);previous=row;}
    for(int i=0;i<(duck?4:10)&&!stop;i++)Thread.Sleep(50);
   }}finally{Restore(leases);}
  }return 0;
 }catch(Exception e){Console.Error.WriteLine(e.Message);return 1;}}
}
