using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;

// Standalone bounded helper. No hooks, native game calls, allocations in the
// game, file overrides, or cache resets. Only the verified cached property.
internal static class ScoreboardTool {
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,int pid);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool ReadProcessMemory(IntPtr h,IntPtr p,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool WriteProcessMemory(IntPtr h,IntPtr p,byte[] b,UIntPtr n,out UIntPtr done);
 [StructLayout(LayoutKind.Sequential)] struct Region { public IntPtr Address,Allocation;public uint AllocationProtect;public ushort Partition;public UIntPtr Size;public uint State,Protect,Type; }
 [DllImport("kernel32.dll")] static extern UIntPtr VirtualQueryEx(IntPtr h,IntPtr p,out Region r,UIntPtr n);
 const uint Stock=0x39264490,Simple=0xD86A8DAB,Name=0xFE11D138,Resource=0x45E79640;
 static readonly Dictionary<string,uint> Choices=ScoreboardCatalog.Choices;
 static readonly Dictionary<uint,string> Files=ScoreboardCatalog.Files;
 static uint ActiveRva=0xA907560;
 static uint OtherActiveRva=0xA8E2660;
 static uint ResourcesRva=0xBE01608;
 static uint ResourceVtable=0x807F878;
 static uint ResourceMethod=0x506F510;
 static uint PackagesRva=0xA8F89A0;
 static void SelectBuild(bool updated){
  ActiveRva=updated?0xA90B540U:0xA907560U;
  OtherActiveRva=updated?0xA8E6640U:0xA8E2660U;
  ResourcesRva=updated?0xBE05908U:0xBE01608U;
  ResourceVtable=updated?0x80837B8U:0x807F878U;
  ResourceMethod=updated?0x5071E10U:0x506F510U;
  PackagesRva=updated?0xA8FC980U:0xA8F89A0U;
 }
 static IntPtr handle;static ulong basis;static List<KeyValuePair<ulong,byte[]>> snapshots=new List<KeyValuePair<ulong,byte[]>>();
 sealed class Pending:Exception { public Pending(string s):base(s){} }
 static uint D(byte[] b,int o=0){return BitConverter.ToUInt32(b,o);} static ulong Q(byte[] b,int o=0){return BitConverter.ToUInt64(b,o);}
 static byte[] Read(ulong at,int n,bool remember=false){
  if(at<0x10000||at>0x7FFFFFFFFFFFUL||n<1||n>1024*1024)throw new Exception("Invalid scoreboard address.");
  var b=new byte[n];UIntPtr done;if(!ReadProcessMemory(handle,(IntPtr)(long)at,b,(UIntPtr)n,out done)||done.ToUInt64()!=(ulong)n)throw new Pending("Waiting for stable game data.");
  if(remember)snapshots.Add(new KeyValuePair<ulong,byte[]>(at,b));return b;
 }
 static void Anchor(uint rva,string hex){var expected=Enumerable.Range(0,hex.Length/2).Select(i=>Convert.ToByte(hex.Substring(i*2,2),16)).ToArray();if(!Read(basis+rva,expected.Length).SequenceEqual(expected))throw new Exception("This game build is not supported by the scoreboard picker.");}
 static bool Inactive(){return Q(Read(basis+ActiveRva,8))==0&&Q(Read(basis+OtherActiveRva,8))==0;}
 static void Stable(){foreach(var s in snapshots)if(!Read(s.Key,s.Value.Length).SequenceEqual(s.Value))throw new Pending("Waiting for stable scoreboard data.");}
 static void ValidateResource(uint wanted){
  ulong context=Q(Read(basis+ResourcesRva,8,true));if(context==0)throw new Pending("Waiting for game resources.");
  var header=Read(context,0xB0,true);uint count=D(header,0xA0);ulong buckets=Q(header,0xA8);
  if(count==0||count>1048576||(count&(count-1))!=0)throw new Exception("Unrecognized resource table.");
  ulong first=Q(Read(buckets+8*(0x988E62C4U&(count-1)),8,true)),at=first;var seen=new HashSet<ulong>();ulong registry=0;
  while(at!=0&&seen.Add(at)){
   if(seen.Count>512)throw new Exception("Resource table exceeds safety bound.");
   var row=Read(at,64,true);ulong owner=Q(row,0x28);
   if(D(row)==0x988E62C4&&D(row,4)==0x7DE5F718&&(row[0x23]&2)==0&&owner!=0){
    var o=Read(owner,0xB8,true);
    if(D(o,0x54)==0xDCE5273B){
     if(registry!=0||Q(o)!=basis+ResourceVtable||Q(o,0x80)==0||Q(Read(Q(o)+0xC8,8))!=basis+ResourceMethod)throw new Exception("Resource ownership could not be verified.");
     registry=Q(row,0x18);
    }
   }at=Q(row,0x38);
  }
  if(registry==0)throw new Pending("Waiting for scoreboard resources.");
  if(D(Read(registry,4,true))!=1525)throw new Exception("Scoreboard registry version differs.");
  var data=Read(registry,4+1525*260,true);int matches=0;bool original=false;
  for(int i=0;i<1525;i++){int p=4+i*260;uint id=D(data,p);if(id!=wanted&&id!=Stock)continue;
   string filename=System.Text.Encoding.Unicode.GetString(data,p+4,256).Split('\0')[0].ToLowerInvariant();
   if(id==Stock)original=filename==Files[Stock];if(id==wanted&&filename==Files[wanted])matches++;
  }if(!original||matches!=1)throw new Exception("Scoreboard resource mapping differs; restart the game without research tests.");
 }
 static object Run(string[] args){
  if(args.Length!=4||!Choices.ContainsKey(args[1])||(args[0]!="apply"&&args[0]!="check"))throw new Exception("Invalid scoreboard request.");
  uint wanted=Choices[args[1]];var games=Process.GetProcessesByName("NBA2K27");
  if(games.Length==0)throw new Pending("Saved — waiting for NBA 2K27.");if(games.Length!=1)throw new Exception("More than one NBA 2K27 process is running.");
  using(var game=games[0]){
   string expected=Path.GetFullPath(Path.Combine(args[2],"NBA2K27.exe"));
   if(!String.Equals(Path.GetFullPath(game.MainModule.FileName),expected,StringComparison.OrdinalIgnoreCase))throw new Exception("The running game does not match the launcher game folder.");
   bool write=args[0]=="apply";
   if(write&&Process.GetProcesses().Any(p=>{try{return p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase);}finally{p.Dispose();}}))throw new Exception("Use the launcher's without-EAC session for scoreboard changes.");
   handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Exception("Cannot access the game. Run the launcher and game at the same permission level.");
   basis=(ulong)game.MainModule.BaseAddress.ToInt64();var dos=Read(basis,4096);int pe=(int)D(dos,0x3C);
   if(pe<64||pe>3800||D(dos,pe)!=0x4550)throw new Exception("Invalid game header.");
   bool updated=D(dos,pe+8)==0x6AA2013D&&D(dos,pe+80)==0x36558000;
   if(!updated&&(D(dos,pe+8)!=0x6A974D13||D(dos,pe+80)!=0x368C3000))throw new Exception("This game update is not supported by the scoreboard picker.");SelectBuild(updated);
   if(updated){
   Anchor(0x1181DBD,"488d3db4ab7709");Anchor(0x1181DD0,"4883c6184881fe000c0000");
   Anchor(0x104568C,"488b0dad0f8a09");Anchor(0x1123F3F,"488935fa757e09");
   Anchor(0x104521A,"4c8d6e18448b4e18");Anchor(0x1025680,"4d8b49504939c9740e45390175f24d85c97404418b5108");
   Anchor(0x11783A9,"8138f01035c675ef4885c074068b4008894120");Anchor(0x104200B,"8b57208993e8020000");
   Anchor(0x1046BA9,"83b9e8020000007417488dbee0020000e8d2608b024889c14889fae817640204");
   Anchor(0x118AD48,"e85391f9ff");Anchor(0x118B8A0,"56574883ec484c89c64889cf4c894c2440488d4c2440e8d507ee03");
   }else{
   Anchor(0x118172D,"488d3d64727709");Anchor(0x1181740,"4883c6184881fe000c0000");
   Anchor(0x1044FFC,"488b0d5dd68909");Anchor(0x11238AF,"488935aa3c7e09");
   Anchor(0x1044B8A,"4c8d6e18448b4e18");Anchor(0x1024FF0,"4d8b49504939c9740e45390175f24d85c97404418b5108");
   Anchor(0x1177D19,"8138f01035c675ef4885c074068b4008894120");Anchor(0x104197B,"8b57208993e8020000");
   Anchor(0x1046519,"83b9e8020000007417488dbee0020000e8e23e8b024889c14889fae8a7410204");
   Anchor(0x118A6B8,"e85391f9ff");Anchor(0x118B210,"56574883ec484c89c64889cf4c894c2440488d4c2440e865");
   }
   ValidateResource(wanted);
   ulong table=basis+PackagesRva;var slots=Read(table,128*24,true);ulong package=0;
   for(int i=0;i<128;i++)if(D(slots,i*24)==0x0A2ADB76){if(package!=0)throw new Exception("Blacktop package is not unique.");if(slots[i*24+16]!=0)throw new Pending("Waiting for Blacktop to finish loading.");package=Q(slots,i*24+8);}
   if(package==0)throw new Pending("Enter Blacktop selection to prepare this choice.");
   var ph=Read(package,32,true);ulong storage=Q(ph,8),node=Q(ph);uint used=D(ph,16),capacity=D(ph,20);
   if(used==0||used>capacity||capacity>300)throw new Exception("Blacktop package layout differs.");
   var visited=new HashSet<ulong>();byte[] cached=null;
   while(node!=0){
    if(node<storage||node>=storage+used*0x88UL||(node-storage)%0x88!=0||!visited.Add(node))throw new Exception("Invalid package tree.");
    cached=Read(node,0x88,true);uint id=D(cached);if(id==Stock)break;node=Q(cached,Stock<id?0x78:0x80);
   }if(node==0)throw new Pending("Waiting for the Blacktop scoreboard entry.");
   ulong sentinel=node+8,at=Q(cached,0x58);var fields=new Dictionary<ulong,byte[]>();visited.Clear();
   while(at!=0&&at!=sentinel){
    if(!visited.Add(at)||visited.Count>128)throw new Exception("Invalid scoreboard property list.");
    var b=Read(at,96,true);fields.Add(at,b);
    at=Q(b,0x50);
   }
   if(at!=sentinel)throw new Exception("Blacktop scoreboard configuration could not be verified.");
   uint current;var changes=ScoreboardProfile.Plan(fields,wanted,out current);
   Stable();ulong active=Q(Read(basis+ActiveRva,8));
   if(changes.Count==0){
    bool running=active!=0;bool matched=!running||D(Read(active+0x28,4))==wanted;
    return new {phase=matched?"applied":"pending",message=matched?(running?"Active in this match":"Ready for the next match"):"Pending — next Blacktop match",pid=game.Id,resource=current};
   }
   if(!Inactive())throw new Pending("Pending — leave the match to change scoreboard.");
   foreach(var change in changes){ulong field=change.Address;Region region;if(VirtualQueryEx(handle,(IntPtr)(long)field,out region,(UIntPtr)Marshal.SizeOf(typeof(Region)))==UIntPtr.Zero||region.State!=0x1000||region.Type!=0x20000||region.Protect!=4||field%4!=0||field+96>(ulong)region.Address.ToInt64()+region.Size.ToUInt64())throw new Exception("Scoreboard property is not writable private memory.");}
   if(!write)return new {phase="ready",message="Safe preflight passed; no changes made",pid=game.Id,resource=current};
   string receipt=Path.GetFullPath(args[3]);Directory.CreateDirectory(Path.GetDirectoryName(receipt));
   byte[] record=System.Text.Encoding.UTF8.GetBytes(new JavaScriptSerializer().Serialize(new {schema="scoreboard-picker-rollback-v2",pid=game.Id,started=game.StartTime.ToUniversalTime().Ticks,changes=changes.Select(c=>new {address=c.Address,before=Convert.ToBase64String(c.Before.Take(12).ToArray()),after=Convert.ToBase64String(c.After)}).ToArray()}));
   using(var f=new FileStream(receipt,FileMode.Create,FileAccess.Write,FileShare.None)){f.Write(record,0,record.Length);f.Flush(true);}
   Stable();if(!Inactive())throw new Pending("Pending — a match started before the change.");
   foreach(var change in changes){
    if(!Inactive()||!Read(change.Address,96).SequenceEqual(change.Before))throw new Exception("Scoreboard state changed during preparation. Close the game before retrying.");
    UIntPtr written;bool ok=WriteProcessMemory(handle,(IntPtr)(long)change.Address,change.After,(UIntPtr)12,out written);
    var expectedBytes=(byte[])change.Before.Clone();Array.Copy(change.After,expectedBytes,12);
    if(!ok||written.ToUInt64()!=12||!Read(change.Address,96).SequenceEqual(expectedBytes))throw new Exception("Scoreboard change could not be verified. Close the game to clear this session change.");
   }
   return new {phase="applied",message="Ready for the next match",pid=game.Id,resource=wanted};
  }
 }
 static object SerializedRun(string[] args){using(var gate=new System.Threading.Mutex(false,"Local\\2kLauncherScoreboardPreparation")){bool locked=false;try{try{locked=gate.WaitOne(0);}catch(System.Threading.AbandonedMutexException){locked=true;}if(!locked)throw new Pending("Waiting for scoreboard preparation.");return Run(args);}finally{if(locked)gate.ReleaseMutex();}}}
 static int Main(string[] args){try{Console.WriteLine(new JavaScriptSerializer().Serialize(SerializedRun(args)));return 0;}catch(Pending e){Console.WriteLine(new JavaScriptSerializer().Serialize(new {phase="pending",message=e.Message}));return 0;}catch(Exception e){Console.WriteLine(new JavaScriptSerializer().Serialize(new {phase="error",message=e.Message}));return 1;}finally{if(handle!=IntPtr.Zero)CloseHandle(handle);}}
}
