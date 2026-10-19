using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Threading;
using System.Web.Script.Serialization;

// Owns two static arena-ID entries for this helper's lifetime. No code hooks,
// game calls, loaded-level edits, or game-folder writes. EOF restores ownership.
class ArenaTool {
 [DllImport("kernel32",SetLastError=true)] static extern IntPtr OpenProcess(uint a,bool i,int p);
 [DllImport("kernel32")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32")] static extern bool ReadProcessMemory(IntPtr h,IntPtr p,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32")] static extern bool WriteProcessMemory(IntPtr h,IntPtr p,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32")] static extern bool VirtualProtectEx(IntPtr h,IntPtr p,UIntPtr n,uint prot,out uint old);
 [StructLayout(LayoutKind.Sequential)] struct Region {public IntPtr Address,Allocation;public uint AllocationProtect;public ushort Partition;public UIntPtr Size;public uint State,Protect,Type;}
 [DllImport("kernel32")] static extern UIntPtr VirtualQueryEx(IntPtr h,IntPtr p,out Region r,UIntPtr n);
 static readonly byte[] Stock={0xb9,3,0,0,0xba,3,0,0,0x8b,3,0,0,0xc5,2,0,0,0xc5,2,0,0};
 static readonly JavaScriptSerializer Json=new JavaScriptSerializer();
 static uint RosterRootRva=0x9A43EB8;
 static uint MappingRva=0x78771F0;
 static uint BlacktopRva=0xAEB29D0;
 static uint BlacktopVtable=0x78AABA8;
 static uint ModeCurrent=0xAC73E28;
 static uint ModeAlternate=0xAC73E24;
 static uint ModeTable=0xAC73660;
 static void SelectBuild(bool updated){
  RosterRootRva=updated?0x9A47EB8U:0x9A43EB8U;
  MappingRva=updated?0x787AFD0U:0x78771F0U;
  BlacktopRva=updated?0xAEB69D0U:0xAEB29D0U;
  BlacktopVtable=updated?0x78AEA48U:0x78AABA8U;
  ModeCurrent=updated?0xAC77E28U:0xAC73E28U;
  ModeAlternate=updated?0xAC77E24U:0xAC73E24U;
  ModeTable=updated?0xAC77660U:0xAC73660U;
 }
 static bool inspectOnly;static volatile bool ended;static volatile string desired="default";
 static Process game;static IntPtr handle;static ulong basis;static byte[] owned;
 sealed class ArenaRecord {public ulong Address,HeaderAddress,RosterRoot,RosterOwner,Allocation;public byte[] Header,Original,Replacement;}
 static void CheckDirectory(ArenaRecord record){
  Need(Q(Read(basis+RosterRootRva,8))==record.RosterRoot&&Q(Read(record.RosterRoot+0xA8,8))==record.RosterOwner,"Active roster changed. Select the arena again before starting the match.");
  Need(Read(record.HeaderAddress,16).SequenceEqual(record.Header),"Arena directory changed; close the game.");
 }
 static ArenaRecord template;
 static byte[] TemplateFields(string file,string floor){
  Need(!String.IsNullOrEmpty(file)&&file.Length<38&&!String.IsNullOrEmpty(floor)&&floor.Length<40&&
   file.All(c=>c=='_'||(c>='a'&&c<='z')||(c>='0'&&c<='9'))&&floor==file+"_floor","Invalid arena template filenames.");
  var bytes=new byte[78];Array.Copy(System.Text.Encoding.ASCII.GetBytes(file),bytes,file.Length);Array.Copy(System.Text.Encoding.ASCII.GetBytes(floor),0,bytes,38,floor.Length);return bytes;
 }
 static void WriteFields(ArenaRecord record,byte[] expected,byte[] next){
  Need(Selection(),"Return to player selection before changing arena records.");
  CheckDirectory(record);
  Need(BitConverter.ToUInt16(Read(record.Address+0x792,2),0)==700,"Arena record ownership changed; close the game.");
  Need(Read(record.Address+0x7E,78).SequenceEqual(expected),"Arena fields changed outside this launcher; close the game.");
  Region r;ulong at=record.Address+0x7E;Need(VirtualQueryEx(handle,(IntPtr)(long)at,out r,(UIntPtr)Marshal.SizeOf(typeof(Region)))!=UIntPtr.Zero&&r.State==0x1000&&r.Type==0x20000&&(ulong)r.Allocation.ToInt64()==record.Allocation&&r.Protect==4&&at+78<=(ulong)r.Address.ToInt64()+r.Size.ToUInt64(),"Arena record is not writable private memory.");
  Need(Selection(),"Player selection ended before arena preparation.");UIntPtr done;
  Need(WriteProcessMemory(handle,(IntPtr)(long)at,next,(UIntPtr)78,out done)&&done.ToUInt64()==78&&Read(at,78).SequenceEqual(next),"Arena record write uncertain; close the game.");
 }
 static bool RestoreTemplate(){
  if(template==null)return true;if(game.HasExited){template=null;return true;}if(!Selection())return false;
  CheckDirectory(template);var current=Read(template.Address+0x7E,78);
  if(!current.SequenceEqual(template.Original))WriteFields(template,template.Replacement,template.Original);
  else Need(Read(template.HeaderAddress,16).SequenceEqual(template.Header),"Arena directory changed; close the game.");
  template=null;return true;
 }
 static string last="";static long revision;static Dictionary<string,Dictionary<string,object>> choices;
 static void Need(bool ok,string why){if(!ok)throw new Exception(why);}
 static uint D(byte[] b,int o=0){return BitConverter.ToUInt32(b,o);} static ulong Q(byte[] b,int o=0){return BitConverter.ToUInt64(b,o);}
 static byte[] Read(ulong at,int n){Need(n>0&&n<=8192&&at>=65536,"Invalid arena read.");var b=new byte[n];UIntPtr got;Need(ReadProcessMemory(handle,(IntPtr)(long)at,b,(UIntPtr)n,out got)&&got.ToUInt64()==(ulong)n,"Arena data is not ready.");return b;}
 static void Emit(string phase,string message,string selected){string row=Json.Serialize(new{phase,message,selected});if(row!=last){Console.WriteLine(row);Console.Out.Flush();last=row;}}
 static void Anchor(uint rva,string hex){var b=Enumerable.Range(0,hex.Length/2).Select(i=>Convert.ToByte(hex.Substring(i*2,2),16)).ToArray();Need(Read(basis+rva,b.Length).SequenceEqual(b),"Game build changed; arena picker disabled.");}
 static void Page(){Region p;Need(VirtualQueryEx(handle,(IntPtr)(long)(basis+MappingRva),out p,(UIntPtr)Marshal.SizeOf(typeof(Region)))!=UIntPtr.Zero&&p.State==0x1000&&p.Type==0x1000000&&p.Allocation.ToInt64()==(long)basis&&p.Protect==2,"Arena mapping ownership changed.");}
 static void Write(byte[] expected,byte[] next){
  Page();Need(Read(basis+MappingRva,20).SequenceEqual(expected),"Arena mapping changed outside this launcher.");uint old;
  Need(VirtualProtectEx(handle,(IntPtr)(long)(basis+MappingRva),(UIntPtr)8,4,out old),"Cannot prepare arena mapping.");
  try {Need(old==2,"Unexpected mapping protection.");Need(Read(basis+MappingRva,20).SequenceEqual(expected),"Arena mapping changed.");UIntPtr got;
   Need(WriteProcessMemory(handle,(IntPtr)(long)(basis+MappingRva),next,(UIntPtr)8,out got)&&got.ToUInt64()==8,"Arena write could not be verified; close the game.");
   Need(Read(basis+MappingRva,20).SequenceEqual(next),"Arena readback failed; close the game.");
  }finally{uint ignored;Need(VirtualProtectEx(handle,(IntPtr)(long)(basis+MappingRva),(UIntPtr)8,old,out ignored),"Close the game: arena page restoration failed.");}Page();
 }
 static void Restore(){if(handle!=IntPtr.Zero&&owned!=null&&!game.HasExited){var now=Read(basis+MappingRva,20);if(!now.SequenceEqual(Stock))Write(owned,Stock);owned=null;}}
 static void Close(){try{Restore();
   // EOF must not rewrite a template referenced by the currently loaded court.
   // Keep this tiny helper alive until safe selection or game exit; no hooks.
   int stable=0;while(template!=null&&!game.HasExited){
    bool selection=false;try{selection=Selection();}catch{ }
    stable=selection?stable+1:0;if(stable>=4){RestoreTemplate();break;}Thread.Sleep(250);
   }
  }finally{template=null;owned=null;if(handle!=IntPtr.Zero)CloseHandle(handle);handle=IntPtr.Zero;if(game!=null)game.Dispose();game=null;}}
 static bool Selection(){if(Q(Read(basis+BlacktopRva,8))!=basis+BlacktopVtable||D(Read(basis+BlacktopRva+0xEA8,4))!=3)return false;
  foreach(uint at in new uint[]{ModeCurrent,ModeAlternate}){uint i=D(Read(basis+at,4));if(i>=128||D(Read(basis+ModeTable+i*64,4))!=0x92DF06DE)return false;}return true;}
 static void Connect(string root){
  var games=Process.GetProcessesByName("NBA2K27");Need(games.Length==1,"Waiting for NBA 2K27.");game=games[0];
  Need(String.Equals(Path.GetFullPath(game.MainModule.FileName),Path.GetFullPath(Path.Combine(root,"NBA2K27.exe")),StringComparison.OrdinalIgnoreCase),"Game folder differs.");
  Need(!Process.GetProcesses().Any(p=>{try{return p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase);}finally{p.Dispose();}}),"Use the launcher's without-EAC session.");
  handle=OpenProcess(inspectOnly?0x410U:0x438U,false,game.Id);Need(handle!=IntPtr.Zero,"Run game and launcher at the same permission level.");basis=(ulong)game.MainModule.BaseAddress.ToInt64();
  var head=Read(basis,4096);int pe=(int)D(head,60);Need(pe>=64&&pe<3800&&D(head,pe)==0x4550,"Invalid game header.");
  bool updated=D(head,pe+8)==0x6AA2013D&&D(head,pe+80)==0x36558000;
  Need(updated||(D(head,pe+8)==0x6A974D13&&D(head,pe+80)==0x368C3000),"Unsupported game build.");SelectBuild(updated);
  if(updated){
   Anchor(0x32BD8EE,"488d0ddbd65b04");Anchor(0x32BE367,"488d0d62cc5b04");Anchor(0x32BF419,"488d0db0bb5b04");Anchor(0x32BF7CE,"488d0dfbb75b04");Anchor(0x22DFAC0,"8b058ab97c07c3");Anchor(0x2EE8EDC,"0fb7889207000039ce490f45c5");
  }else{
  Anchor(0x32BB44E,"488d0d9bbd5b04");Anchor(0x32BBEC7,"488d0d22b35b04");Anchor(0x32BCF79,"488d0d70a25b04");Anchor(0x32BD32E,"488d0dbb9e5b04");Anchor(0x22DD970,"8b05da9a7c07c3");Anchor(0x2EE6A3C,"0fb7889207000039ce490f45c5");
  }
  Page();Need(Read(basis+MappingRva,20).SequenceEqual(Stock),"Another arena override is active. Close the test or restart the game.");
 }
 static ArenaRecord ValidateArena(uint wanted,string file){
  // The active roster owns this directory. Its table location varies with roster contents.
  ulong root=Q(Read(basis+RosterRootRva,8)),owner=Q(Read(root+0xA8,8));Region region;
  Need(VirtualQueryEx(handle,(IntPtr)(long)owner,out region,(UIntPtr)Marshal.SizeOf(typeof(Region)))!=UIntPtr.Zero&&region.State==0x1000&&region.Type==0x20000&&(region.Protect&0x101)==0,"Active roster ownership is unavailable.");
  ulong allocation=(ulong)region.Allocation.ToInt64();
  Need(owner==allocation+0x50,"Active roster header layout changed.");
  ulong header=owner+0x60;byte[] h=Read(header,16);ulong count=Q(h),table=Q(h,8);
  Need(count>=1&&count<=2048&&table>=header+16&&table<0x7FFFFFFF0000UL,"Arena directory bounds are invalid.");
  ulong bytes=count*0x1278;
  Need(VirtualQueryEx(handle,(IntPtr)(long)table,out region,(UIntPtr)Marshal.SizeOf(typeof(Region)))!=UIntPtr.Zero&&region.State==0x1000&&region.Type==0x20000&&(region.Protect&0x101)==0&&(ulong)region.Allocation.ToInt64()==allocation&&table+bytes<=(ulong)region.Address.ToInt64()+region.Size.ToUInt64(),"Arena table is outside its active roster allocation.");
  int known=0,matches=0;ArenaRecord candidate=null;
  for(ulong i=0;i<count;i++){
   var row=Read(table+i*0x1278,0x794);uint id=BitConverter.ToUInt16(row,0x792);string fn=System.Text.Encoding.ASCII.GetString(row,0x7E,38).Split('\0')[0];
   if(id==700&&fn=="arena_700_int")known++;
   if(id==wanted&&fn==file){matches++;candidate=new ArenaRecord{Address=table+i*0x1278,HeaderAddress=header,Header=h,RosterRoot=root,RosterOwner=owner,Allocation=allocation,Original=row.Skip(0x7E).Take(78).ToArray()};}
  }
  Need(known==1&&matches==1,"Selected arena record could not be verified in the active roster.");
  CheckDirectory(candidate);return candidate;
 }
 static int Main(string[] args){
  try{Need(args.Length==2||args.Length==3,"Arena helper arguments missing.");var catalog=Json.Deserialize<Dictionary<string,object>>(File.ReadAllText(args[1]));choices=new Dictionary<string,Dictionary<string,object>>();
   foreach(var x in (System.Collections.IEnumerable)catalog["choices"]){var c=(Dictionary<string,object>)x;choices.Add((string)c["id"],c);}
   if(args.Length==3){inspectOnly=true;Connect(args[0]);var c=choices[args[2]];ValidateArena(Convert.ToUInt32(c["arenaId"]),(string)c["file"]);Emit("verified","Read-only arena preflight passed.",args[2]);return 0;}
   var input=new Thread(()=>{try{string line;while((line=Console.ReadLine())!=null){if(line.Length>128)continue;if(choices.ContainsKey(line)){desired=line;Interlocked.Increment(ref revision);}}}finally{ended=true;}});input.IsBackground=true;input.Start();
   long applied=-1;string previous="";
   while(!ended){string id=desired;long rev=Interlocked.Read(ref revision);
    try{
     if(game!=null&&game.HasExited){Close();applied=-1;}
     if(id=="default"){Restore();bool restored=RestoreTemplate();Emit(restored?"applied":"pending",restored?"Uses the game's Day / Night choice.":"Default mapping restored. Leave the match to restore the temporary arena record.",id);applied=rev;}
     else{
      var c=choices[id];Need(!c.ContainsKey("disabled")||!(bool)c["disabled"],"This arena needs further investigation.");
      if(handle==IntPtr.Zero){try{Connect(args[0]);}catch{Close();throw;}applied=-1;}
      if(applied!=rev||previous!=id){
       if(!Selection()){Emit("pending","Return to player selection to prepare this arena.",id);Thread.Sleep(500);continue;}
       Need(RestoreTemplate(),"Return to player selection before changing arenas.");
       uint target=Convert.ToUInt32(c["arenaId"]);bool usesTemplate=c.ContainsKey("templateArenaId");
       var record=ValidateArena(target,usesTemplate?"arena_700_int":(string)c["file"]);Need(Selection(),"Selection changed; retrying.");
       if(!usesTemplate&&c.ContainsKey("floorFile"))Need(System.Text.Encoding.ASCII.GetString(record.Original,38,40).Split('\0')[0]==(string)c["floorFile"],"Selected arena floor differs from the verified roster.");
       if(usesTemplate){
        Need(target==700&&System.Text.Encoding.ASCII.GetString(record.Original,38,40).Split('\0')[0]=="arena_700_int_floor","Summer League template differs.");
        var entries=new HashSet<string>(File.ReadLines(Path.Combine(args[0],"manifest")).Select(line=>line.Split(',')[0]),StringComparer.OrdinalIgnoreCase);
        string file=(string)c["file"],floor=(string)c["floorFile"];
        Need(Convert.ToUInt32(c["templateArenaId"])==700&&entries.Contains("levels/"+file+".iff")&&entries.Contains("levels/"+floor+".iff"),"Arena template assets are missing.");
        record.Replacement=TemplateFields(file,floor);template=record;WriteFields(record,record.Original,record.Replacement);
       }
       var next=(byte[])Stock.Clone();Array.Copy(BitConverter.GetBytes(target),0,next,0,4);Array.Copy(BitConverter.GetBytes(target),0,next,4,4);
       var expected=owned??Stock;owned=next;try{Write(expected,next);}catch{owned=Read(basis+MappingRva,20).SequenceEqual(expected)?expected:next;throw;}
       applied=rev;previous=id;
      }Emit("applied","Ready for the next Blacktop match.",id);
     }
    }catch(Exception e){Emit("error",e.Message,id);if(handle==IntPtr.Zero&&game!=null){game.Dispose();game=null;}Thread.Sleep(1000);}
    Thread.Sleep(500);
   }return 0;
  }catch(Exception e){Emit("error",e.Message,desired);return 1;}
  finally{try{Close();}catch(Exception e){Emit("error","Close the game: "+e.Message,desired);}}
 }
}
