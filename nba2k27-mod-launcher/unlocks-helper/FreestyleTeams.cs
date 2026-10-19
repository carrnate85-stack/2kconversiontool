using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;

// Freestyle-only data unlock. No code hooks, native calls, remote allocations,
// roster edits or game files. The game owns/reset these two session fields.
internal static partial class FreestyleTeams {
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,int pid);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h,IntPtr at,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32.dll")] static extern bool WriteProcessMemory(IntPtr h,IntPtr at,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr h,out long created,out long exit,out long kernel,out long user);
 [StructLayout(LayoutKind.Sequential)] struct Region {public IntPtr Address,Allocation;public uint AllocationProtect;public ushort Partition;public UIntPtr Size;public uint State,Protect,Type;}
 [DllImport("kernel32.dll")] static extern UIntPtr VirtualQueryEx(IntPtr h,IntPtr at,out Region r,UIntPtr n);
 static IntPtr handle;static ulong basis;static long created;static JavaScriptSerializer json=new JavaScriptSerializer();
 sealed class Stop:Exception {internal string phase;internal Stop(string p,string s):base(s){phase=p;}}
 sealed class Picker {
  internal ulong root,owner,table,mask;internal int count,mode,alternateMode;internal uint category,otherCategory;
  internal byte[] selected,otherSelected,setup,maskWindow,categoryWindow,rows;
  internal bool Context(Picker p){return root==p.root&&owner==p.owner&&table==p.table&&count==p.count&&mode==p.mode&&alternateMode==p.alternateMode&&otherCategory==p.otherCategory&&setup.SequenceEqual(p.setup);}
  internal bool Exact(Picker p){return Context(p)&&mask==p.mask&&category==p.category&&selected.SequenceEqual(p.selected)&&otherSelected.SequenceEqual(p.otherSelected)&&maskWindow.SequenceEqual(p.maskWindow)&&categoryWindow.SequenceEqual(p.categoryWindow);}
 }
 static byte[] Read(ulong at,int n){
  if(at<0x10000||at>0x7fffffffffffUL||n<1||n>1024*FreestylePolicy.Stride)throw new Stop("waiting","Waiting for Freestyle team data.");
  byte[] b=new byte[n];UIntPtr done;
  if(!ReadProcessMemory(handle,(IntPtr)(long)at,b,(UIntPtr)n,out done)||done.ToUInt64()!=(ulong)n)throw new Stop("waiting","Waiting for readable Freestyle team data.");return b;
 }
 static uint D(byte[] b,int o=0){return BitConverter.ToUInt32(b,o);}static ulong Q(byte[] b,int o=0){return BitConverter.ToUInt64(b,o);}
 static void Identity(){long c,e,k,u;if(!GetProcessTimes(handle,out c,out e,out k,out u)||e!=0||c!=created)throw new Stop("ended","Game session ended.");}
 static Region Area(ulong at,ulong n,uint type,bool writable){
  Region r;
  if(at<0x10000||at>0x7fffffffffffUL||n>0x10000000||VirtualQueryEx(handle,(IntPtr)(long)at,out r,(UIntPtr)Marshal.SizeOf(typeof(Region)))==UIntPtr.Zero||r.State!=0x1000||r.Type!=type||(r.Protect&0x101)!=0||(r.Protect&0xee)==0||at+n>(ulong)r.Address.ToInt64()+r.Size.ToUInt64()||(writable&&r.Protect!=4&&r.Protect!=8))throw new Stop("waiting","Waiting for owned Freestyle data.");
  return r;
 }
 static uint FsMasks=0x7a54228;
 static uint FsPrevious=0x7a2beb0;
 static uint FsNext=0x7a2bed0;
 static uint FsCategory=0xb155950;
 static uint FsMask=0xb1564e0;
 static uint FsModeTable=0xac73660;
 static uint FsModeCurrent=0xac73e28;
 static uint FsModeAlternate=0xac73e24;
 static uint FsSetup=0xb156508;
 static uint FsPicker=0xb164df0;
 static uint FsVtable=0x7af5768;
 static uint FsRosterRoot=0x9a43eb8;
 static uint FsSelected=0xb155948;
 static uint FsOtherSelected=0xb155ea0;
 static uint FsMaskWindow=0xb1564d8;
 static bool SelectFreestyleBuild(){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664)throw new Stop("unsupported","Invalid game header.");
  bool updated=D(b,pe+8)==0x6AA2013D&&D(b,pe+80)==0x36558000;
  if(!updated&&(D(b,pe+8)!=0x6A974D13||D(b,pe+80)!=0x368C3000))throw new Stop("unsupported","This game update is not supported by the Freestyle unlock.");
  FsMasks=updated?0x7A58198U:0x7a54228U;
  FsPrevious=updated?0x7A2FE20U:0x7a2beb0U;
  FsNext=updated?0x7A2FE40U:0x7a2bed0U;
  FsCategory=updated?0xB159C30U:0xb155950U;
  FsMask=updated?0xB15A7C0U:0xb1564e0U;
  FsModeTable=updated?0xAC77660U:0xac73660U;
  FsModeCurrent=updated?0xAC77E28U:0xac73e28U;
  FsModeAlternate=updated?0xAC77E24U:0xac73e24U;
  FsSetup=updated?0xB15A7E8U:0xb156508U;
  FsPicker=updated?0xB1690D0U:0xb164df0U;
  FsVtable=updated?0x7AF96C8U:0x7af5768U;
  FsRosterRoot=updated?0x9A47EB8U:0x9a43eb8U;
  FsSelected=updated?0xB159C28U:0xb155948U;
  FsOtherSelected=updated?0xB15A180U:0xb155ea0U;
  FsMaskWindow=updated?0xB15A7B8U:0xb1564d8U;
  CategoryPolicy.SelectBuild(updated);return updated;
 }
 static void Build(uint[][] expectedCycles=null){
  bool updated=SelectFreestyleBuild();
  foreach(var f in new[]{new{r=updated?0x20F086BU:0x20ee71bU,b=updated?"488B0546769507488B80A8000000":"488B0596579507488B80A8000000"},new{r=updated?0x35F9020U:0x35f67a0U,b=updated?"5657534883EC304889CE8B05000CB607":"5657534883EC304889CE8B05A0F1B507"},new{r=updated?0x27BC130U:0x27b9f70U,b="4157415641554154565755534883EC78"}}){
   if(BitConverter.ToString(Read(basis+f.r,f.b.Length/2)).Replace("-","")!=f.b)throw new Stop("unsupported","Freestyle team code differs from the supported build.");
  }
  var masks=Read(basis+FsMasks,40);for(int i=0;i<5;i++)if(Q(masks,i*8)!=FreestylePolicy.Pages[i])throw new Stop("conflict","The game's team filters were changed by another tool.");
  if(expectedCycles==null)expectedCycles=new[]{new uint[]{4,0,1,3,2,5},new uint[]{1,2,4,3,0,5}};
  if(!Read(basis+FsPrevious,24).SequenceEqual(expectedCycles[0].SelectMany(BitConverter.GetBytes))||!Read(basis+FsNext,24).SequenceEqual(expectedCycles[1].SelectMany(BitConverter.GetBytes)))throw new Stop("conflict","The game's team category controls were changed by another tool.");
  foreach(ulong at in new[]{basis+FsCategory,basis+FsMask})if((ulong)Area(at,8,0x1000000,true).Allocation.ToInt64()!=basis)throw new Stop("waiting","Waiting for session-owned team filters.");
 }
 static int Mode(uint rva){int index=BitConverter.ToInt32(Read(basis+rva,4),0);if(index<0||index>=128||D(Read(basis+FsModeTable+(ulong)index*64,4))!=0xf54d5d12)throw new Stop("outside","Waiting for Freestyle team selection.");return index;}
 static Picker Snapshot(bool deep){
  Identity();var p=new Picker{mode=Mode(FsModeCurrent),alternateMode=Mode(FsModeAlternate)};
  p.setup=Read(basis+FsSetup,24);
  if(D(p.setup)!=6||D(p.setup,12)!=1||D(p.setup,16)!=1||D(p.setup,20)!=4)throw new Stop("outside","Waiting for Freestyle team selection.");
  if(Q(Read(basis+FsPicker,8))!=basis+FsVtable)throw new Stop("waiting","Waiting for the Freestyle picker.");
  p.root=Q(Read(basis+FsRosterRoot,8));Area(p.root,0xb0,0x20000,false);
  p.owner=Q(Read(p.root+0xa8,8));var owner=Area(p.owner,0x90,0x20000,false);
  var header=Read(p.owner+0x80,16);p.count=(int)D(header);p.table=Q(header,8);
  if(p.count<1||p.count>1024)throw new Stop("waiting","Waiting for the mixed NBA/WNBA roster.");
  var table=Area(p.table,(ulong)(p.count*FreestylePolicy.Stride),0x20000,false);
  if(owner.Allocation!=table.Allocation)throw new Stop("waiting","The roster table is changing.");
  p.categoryWindow=Read(basis+FsSelected,20);p.selected=p.categoryWindow.Take(8).ToArray();p.category=D(p.categoryWindow,8);
  var other=Read(basis+FsOtherSelected,12);p.otherSelected=other.Take(8).ToArray();p.otherCategory=D(other,8);
  foreach(var selected in new[]{p.selected,p.otherSelected}){ulong ptr=Q(selected);if(ptr<p.table||ptr>=p.table+(ulong)p.count*FreestylePolicy.Stride||(ptr-p.table)%FreestylePolicy.Stride!=0)throw new Stop("waiting","Waiting for initialized team choices.");}
  p.maskWindow=Read(basis+FsMaskWindow,24);p.mask=Q(p.maskWindow,8);
  if(deep){p.rows=Read(p.table,p.count*FreestylePolicy.Stride);try{FreestylePolicy.VerifyTeams(p.rows,p.count);}catch(InvalidOperationException e){throw new Stop("waiting",e.Message);}}
  if(Mode(FsModeCurrent)!=p.mode||Mode(FsModeAlternate)!=p.alternateMode||!Read(basis+FsSetup,24).SequenceEqual(p.setup)||Q(Read(basis+FsRosterRoot,8))!=p.root||Q(Read(p.root+0xa8,8))!=p.owner||!Read(p.owner+0x80,16).SequenceEqual(header))throw new Stop("waiting","The game is changing modes; waiting.");
  Identity();return p;
 }
 static void Plain(string file){for(string p=Path.GetFullPath(file);p!=null;p=Path.GetDirectoryName(p)){if((File.Exists(p)||Directory.Exists(p))&&(File.GetAttributes(p)&FileAttributes.ReparsePoint)!=0)throw new Stop("error","Linked unlock storage is not supported.");}}
 static void Save(string file,FreestyleJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);
  if(File.Exists(temp))File.Delete(temp);byte[] b=Encoding.UTF8.GetBytes(json.Serialize(j));
  using(var stream=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){stream.Write(b,0,b.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static FreestyleJournal Load(string file){Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>8192)throw new Stop("error","Unlock session data is invalid.");var j=json.Deserialize<FreestyleJournal>(File.ReadAllText(file));FreestylePolicy.Validate(j);return j;}
 static bool Owns(FreestyleJournal j,Picker p){return j.phase=="applied"&&j.root==p.root&&j.owner==p.owner&&j.table==p.table&&j.count==p.count;}
 static void Write(uint rva,byte[] value){
  // A closed allowlist makes unrelated game data unreachable from this helper.
  if(!((rva==FsMask&&value.SequenceEqual(BitConverter.GetBytes(FreestylePolicy.CombinedMask)))||(rva==FsCategory&&value.SequenceEqual(BitConverter.GetBytes(5U)))))throw new Stop("error","Unexpected unlock write rejected.");
  Identity();UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)(basis+rva),value,(UIntPtr)value.Length,out done)||done.ToUInt64()!=(ulong)value.Length||!Read(basis+rva,value.Length).SequenceEqual(value))throw new Stop("write-failed","Freestyle update could not be verified. Restart the game before retrying.");
 }
 static object Run(string[] args){
  int pid;long exactCreated;ulong exactBase;
  if(args.Length!=6||!new[]{"bind","inspect","apply","blacktop-check"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out exactCreated)||!ulong.TryParse(args[5],out exactBase))throw new Stop("error","Invalid unlock request.");
  bool write=args[0]=="apply";string file=Path.GetFullPath(args[2]),expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  string mutexName;using(var sha=SHA256.Create())mutexName="Local\\2kLauncherFreestyle-"+BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(file.ToUpperInvariant()))).Replace("-","");
  using(var gate=new Mutex(false,mutexName)){
   bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","An unlock check is already running.");
    var games=Process.GetProcessesByName("NBA2K27");
    try{
     if(games.Length==0)return new{phase="no-game",message="Waiting for NBA 2K27.",writes=0};
     if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");
     var game=games[0];string image=Path.GetFullPath(game.MainModule.FileName);if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the launcher game folder.");
     created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(args[0]!="bind"&&(pid!=game.Id||exactCreated!=created||exactBase!=basis))throw new Stop("ended","The launcher-managed game session ended.");
     if(write){foreach(var process in Process.GetProcesses()){using(process){if(process.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Use the launcher's without-EAC session for unlocks.");}}}
     handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access NBA 2K27. Use the same permission level for the game and launcher.");
     Identity();
     if(args[0]=="blacktop-check"){
      BtBuild(image);
      if(Q(Read(basis+BtSlot,8))!=basis+BtOriginal||!Read(basis+BcTable,24).SequenceEqual(BcOriginal))throw new Stop("conflict","Blacktop callback or category dispatch is not stock.");
      return new{phase="verified",message="Blacktop unlock build and native routes verified. No memory changed.",writes=0,pid=game.Id};
     }
     Build();
     if(args[0]=="bind")return new{phase="bound",message="Waiting for Freestyle team selection.",writes=0,pid=game.Id,created=created.ToString(),basis=basis.ToString()};
     var j=Load(file);bool same=FreestylePolicy.Same(j,game.Id,created,basis,image);
     if(same&&new[]{"prepared","mask-written","failed"}.Contains(j.phase))throw new Stop("conflict","An earlier unlock was interrupted. Restart the game before retrying.");
     Picker p;
     try{p=Snapshot(false);}catch(Stop e){if(e.phase=="outside"&&same&&j.phase=="applied"&&write){j.phase="outside";Save(file,j);}throw;}
     if(same&&Owns(j,p)&&FreestylePolicy.Applied(p.mask,p.category))return new{phase="applied",message="WNBA teams are available in Freestyle.",writes=0,eligible=j.eligible};
     if(args[0]=="inspect"){
      p=Snapshot(true);return new{phase="inspect",message="Freestyle roster verified. No memory changed.",writes=0,eligible=FreestylePolicy.VerifyTeams(p.rows,p.count),category=p.category,mask=p.mask.ToString("X"),count=p.count};
     }
     if(!FreestylePolicy.Stock(p.mask,p.category))throw new Stop("conflict","Freestyle team selection was changed outside this unlock. Restart the game to use the saved setting.");
     p=Snapshot(true);if(!FreestylePolicy.Stock(p.mask,p.category))throw new Stop("waiting","Team selection changed; waiting.");
     int eligible=FreestylePolicy.VerifyTeams(p.rows,p.count);
     j=new FreestyleJournal{pid=game.Id,created=created,basis=basis,image=image,root=p.root,owner=p.owner,table=p.table,count=p.count,originalCategory=p.category,eligible=eligible,phase="prepared"};
     // Persist the exact session before the first write, then revalidate everything.
     Save(file,j);
     bool attempted=false;
     try{
      Build();var before=Snapshot(true);if(!p.Exact(before)||!p.rows.SequenceEqual(before.rows))throw new Stop("waiting","Team selection changed before applying; waiting.");
      attempted=true;Write(FsMask,BitConverter.GetBytes(FreestylePolicy.CombinedMask));
      p.mask=FreestylePolicy.CombinedMask;BitConverter.GetBytes(p.mask).CopyTo(p.maskWindow,8);
      var afterMask=Snapshot(true);if(!p.Exact(afterMask)||!p.rows.SequenceEqual(afterMask.rows))throw new Stop("write-failed","The picker changed during the unlock. Restart the game before retrying.");
      j.phase="mask-written";Save(file,j);
      if(!p.Exact(Snapshot(false)))throw new Stop("write-failed","The picker changed during the unlock. Restart the game before retrying.");
      Write(FsCategory,BitConverter.GetBytes(5U));p.category=5;BitConverter.GetBytes(5U).CopyTo(p.categoryWindow,8);
      var after=Snapshot(true);if(!p.Exact(after)||!p.rows.SequenceEqual(after.rows))throw new Stop("write-failed","The picker changed during the unlock. Restart the game before retrying.");
      j.phase="applied";Save(file,j);
      return new{phase="applied",message="WNBA teams are available in Freestyle.",writes=2,eligible=eligible};
     }catch{
      j.phase=attempted?"failed":"outside";Save(file,j);
      if(attempted)throw new Stop("write-failed","The unlock could not be verified. Restart the game before retrying; no further writes will be attempted.");throw;
     }
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var p in games)p.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
 public static int Main(string[] args){try{if(args.Length>0&&args[0]=="ball-assets")return RunBallAssetService(args);Console.WriteLine(json.Serialize(args.Length>0&&args[0].StartsWith("rules-",StringComparison.Ordinal)?RunRuleData(args):args.Length>0&&args[0].StartsWith("crowd-",StringComparison.Ordinal)?RunCrowd(args):args.Length>0&&args[0].StartsWith("presentation-",StringComparison.Ordinal)?RunPresentation(args):args.Length>0&&args[0].StartsWith("game-rules-",StringComparison.Ordinal)?RunGameRules(args):args.Length>0&&args[0].StartsWith("ball-",StringComparison.Ordinal)?RunBall(args):args.Length>0&&args[0].StartsWith("debug-",StringComparison.Ordinal)?RunDebug(args):args.Length>0&&args[0]=="blacktop-watch"?RunBlacktop(args):args.Length>0&&args[0].StartsWith("category-",StringComparison.Ordinal)?RunCategory(args):Run(args)));return 0;}catch(Stop e){Console.WriteLine(json.Serialize(new{phase=e.phase,message=e.Message}));return 0;}catch(Exception){Console.WriteLine(json.Serialize(new{phase="error",message="Unlock helper stopped safely. Restart the game before retrying."}));return 1;}}
}
