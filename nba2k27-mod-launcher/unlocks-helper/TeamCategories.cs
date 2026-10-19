using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

// Shared native category navigation, explicitly authorized across game modes.
// Four static data DWORDs only. No instructions, hooks, roster or menu-field edits.
// Keep the combined-list implementation separate as a research fallback.
internal static partial class FreestyleTeams {
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool VirtualProtectEx(IntPtr h,IntPtr at,UIntPtr n,uint protection,out uint previous);
 static uint[][] CategoryTables(){return new[]{FsPrevious,FsNext}.Select(rva=>{var b=Read(basis+rva,24);return Enumerable.Range(0,6).Select(i=>D(b,i*4)).ToArray();}).ToArray();}
 static uint CategoryPage(){var r=Area(basis+CategoryPolicy.Page,4096,0x1000000,false);if((ulong)r.Allocation.ToInt64()!=basis||(r.Protect!=2&&r.Protect!=4))throw new Stop("conflict","Unexpected category-table protection. No further writes attempted.");return r.Protect;}
 static uint ProtectCategories(uint next){
  if(next!=2&&next!=4)throw new Stop("error","Unexpected category page protection rejected.");
  Identity();uint previous;if(!VirtualProtectEx(handle,(IntPtr)(long)(basis+CategoryPolicy.Page),(UIntPtr)4096,next,out previous))throw new Stop("write-failed","Could not change category-table protection.");return previous;
 }
 static void SaveCategory(string file,CategoryJournal j){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";Plain(temp);
  if(File.Exists(temp))File.Delete(temp);byte[] b=Encoding.UTF8.GetBytes(json.Serialize(j));
  using(var stream=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){stream.Write(b,0,b.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static CategoryJournal LoadCategory(string file){Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>8192)throw new Stop("error","Invalid category session record.");var j=json.Deserialize<CategoryJournal>(File.ReadAllText(file));CategoryPolicy.Validate(j);return j;}
 static void WriteCategory(uint rva,uint value){
  if(!CategoryPolicy.Changes.Any(c=>c[0]==rva&&(c[1]==value||c[2]==value)))throw new Stop("error","Unexpected category write rejected.");
  Identity();var b=BitConverter.GetBytes(value);UIntPtr done;
  if(!WriteProcessMemory(handle,(IntPtr)(long)(basis+rva),b,(UIntPtr)4,out done)||done.ToUInt64()!=4||D(Read(basis+rva,4))!=value)throw new Stop("write-failed","Category write could not be verified.");
 }
 static void RestoreCategories(string file,CategoryJournal j){
  // Unlike heap menu objects, these static image tables live for this exact
  // process. Safe conditional rollback must remain possible after changing modes.
  var current=CategoryTables();if(!CategoryPolicy.Owned(current))throw new Stop("conflict","Category tables changed outside this session. Close the game; no forced restoration.");
  Build(current);uint previous=CategoryPage();
  if(previous==2){if(ProtectCategories(4)!=2)throw new Stop("conflict","Category page changed during restoration.");}
  try{
   foreach(var c in CategoryPolicy.Changes.Reverse()){
    uint value=D(Read(basis+c[0],4));if(value!=c[1]&&value!=c[2])throw new Stop("conflict","Category entry changed during restoration.");
    if(value!=c[1])WriteCategory(c[0],c[1]);
   }
   if(!CategoryPolicy.Equal(CategoryTables(),CategoryPolicy.Original))throw new Stop("write-failed","Original categories could not be verified.");
  }finally{ProtectCategories(2);}
  if(CategoryPage()!=2)throw new Stop("write-failed","Category page protection could not be restored.");
  j.phase="restored";SaveCategory(file,j);
 }
 static object RunCategory(string[] args){
  int pid;long exactCreated;ulong exactBase;
  if(args.Length!=6||!new[]{"category-bind","category-inspect","category-apply","category-observe","category-restore"}.Contains(args[0])||!int.TryParse(args[3],out pid)||!long.TryParse(args[4],out exactCreated)||!ulong.TryParse(args[5],out exactBase))throw new Stop("error","Invalid category unlock request.");
  bool write=args[0]=="category-apply"||args[0]=="category-restore";string file=Path.GetFullPath(args[2]),expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  string mutexName;using(var sha=SHA256.Create())mutexName="Local\\2kLauncherCategories-"+BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(file.ToUpperInvariant()))).Replace("-","");
  using(var gate=new Mutex(false,mutexName)){
   bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","A category check is already running.");
    var games=Process.GetProcessesByName("NBA2K27");
    try{
     if(games.Length==0)return new{phase="no-game",message="Waiting for NBA 2K27.",writes=0};
     if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");
     var game=games[0];string image=Path.GetFullPath(game.MainModule.FileName);if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the launcher game folder.");
     created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(args[0]!="category-bind"&&(pid!=game.Id||exactCreated!=created||exactBase!=basis))throw new Stop("ended","The launcher-managed game session ended.");
     if(write){foreach(var process in Process.GetProcesses()){using(process){if(process.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Use the launcher's without-EAC session for unlocks.");}}}
     handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero)throw new Stop("error","Cannot access NBA 2K27. Use the same permission level for the game and launcher.");
     Identity();SelectFreestyleBuild();var j=LoadCategory(file);bool same=CategoryPolicy.Same(j,game.Id,created,basis,image);var tables=CategoryTables();
     if(!CategoryPolicy.Owned(tables)||(!same&&!CategoryPolicy.Equal(tables,CategoryPolicy.Original)))throw new Stop("conflict","Team category navigation was changed outside this session. Restart the game before testing.");
     Build(tables);uint protection=CategoryPage();
     if(args[0]=="category-restore"){
      if(!same)return new{phase="ended",message="Nothing owned by this session to restore.",writes=0};
      RestoreCategories(file,j);return new{phase="restored",message="Original team categories restored."};
     }
     if(protection!=2||(same&&new[]{"prepared","protecting","writing","failed"}.Contains(j.phase)))throw new Stop("conflict","An earlier category update was interrupted. Close the game before retrying.");
     if(args[0]=="category-bind")return new{phase="bound",message="Waiting for the Freestyle team picker to initialize the category unlock.",writes=0,pid=game.Id,created=created.ToString(),basis=basis.ToString()};
     if(same&&j.phase=="applied"){
      if(!CategoryPolicy.Equal(tables,CategoryPolicy.Enabled))throw new Stop("conflict","Category navigation changed after applying. No automatic overwrite.");
      return new{phase="applied",message="WNBA category unlocked for this session. Team availability depends on the mode.",writes=0};
     }
     if(args[0]=="category-observe")return new{phase="ended",message="No active category session.",writes=0};
     // Initialize only after verifying the known mixed Freestyle roster. Once
     // enabled, the shared category order remains available for this game session.
     var p=Snapshot(true);
     if(!FreestylePolicy.Stock(p.mask,p.category)||p.otherCategory>=5)throw new Stop("conflict","The combined-list experiment is active. Restart with this category test build.");
     if(args[0]=="category-inspect")return new{phase="inspect",message="Native WNBA category test is ready. No memory changed.",writes=0,teams=p.count,wnbaClubs=15};
     if(!CategoryPolicy.Equal(tables,CategoryPolicy.Original))throw new Stop("conflict","Original categories are not available.");
     j=new CategoryJournal{pid=game.Id,created=created,basis=basis,image=image,phase="prepared"};SaveCategory(file,j);
     bool touched=false;
     try{
      Build();var again=Snapshot(true);if(!p.Exact(again)||!p.rows.SequenceEqual(again.rows))throw new Stop("waiting","Team selection changed before applying; waiting.");
      j.phase="protecting";SaveCategory(file,j);touched=true;
      if(ProtectCategories(4)!=2)throw new Stop("conflict","Category page changed before applying.");
      try{
       if(!p.Exact(Snapshot(false))||!CategoryPolicy.Equal(CategoryTables(),CategoryPolicy.Original))throw new Stop("waiting","Category data changed before applying.");
       j.phase="writing";SaveCategory(file,j);
       foreach(var c in CategoryPolicy.Changes){if(D(Read(basis+c[0],4))!=c[1])throw new Stop("conflict","Category data changed during update.");WriteCategory(c[0],c[2]);j.writes++;}
      }finally{ProtectCategories(2);}
      if(CategoryPage()!=2||!CategoryPolicy.Equal(CategoryTables(),CategoryPolicy.Enabled))throw new Stop("write-failed","Category update could not be verified.");
      Build(CategoryPolicy.Enabled);var after=Snapshot(true);if(!p.Exact(after)||!p.rows.SequenceEqual(after.rows))throw new Stop("write-failed","The picker changed during the category test.");
      j.phase="applied";SaveCategory(file,j);return new{phase="applied",message="WNBA category unlocked for this session. Team availability depends on the mode.",writes=4,bytesChanged=16,hooks=0,codeWrites=0};
     }catch{
      if(touched){try{RestoreCategories(file,j);}catch{j.phase="failed";SaveCategory(file,j);throw new Stop("write-failed","Category restoration could not be verified. Close the game before retrying.");}}
      else{j.phase="restored";SaveCategory(file,j);throw;}
      throw new Stop("write-failed","Category test interrupted; original navigation restored. Restart the game before retrying.");
     }
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var process in games)process.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
}
