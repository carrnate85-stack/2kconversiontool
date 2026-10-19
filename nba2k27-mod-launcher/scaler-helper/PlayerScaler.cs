using System;
using System.IO;
using System.Linq;
using System.Diagnostics;
using System.ComponentModel;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Web.Script.Serialization;

// Guarded native data writes only: one shared float plus native opt-in state for
// loaded coaches and arena characters. Already-loaded arena characters receive
// one cache seed; the game owns subsequent refreshes. No injection, code patches,
// thread suspension, game calls, model rebuilds, props or per-frame writes.
// Referee GENERIC characters already opt into this same shared input via
// description +0x7D mask 2. Do not apply a second percentage to their caches.
// Coaches use that same native bit; the game computes and maintains their cache.
internal static class PlayerScaler {
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr OpenProcess(uint access,bool inherit,int pid);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h,IntPtr at,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32.dll")] static extern bool WriteProcessMemory(IntPtr h,IntPtr at,byte[] b,UIntPtr n,out UIntPtr done);
 [DllImport("kernel32.dll")] static extern bool GetProcessTimes(IntPtr h,out long created,out long exit,out long kernel,out long user);
 [StructLayout(LayoutKind.Sequential)] struct Region {public IntPtr Address,Allocation;public uint AllocationProtect;public ushort Partition;public UIntPtr Size;public uint State,Protect,Type;}
 [DllImport("kernel32.dll")] static extern UIntPtr VirtualQueryEx(IntPtr h,IntPtr at,out Region r,UIntPtr n);
 static IntPtr handle;static ulong basis;static JavaScriptSerializer json=new JavaScriptSerializer();
 sealed class Stop:Exception {internal string phase;internal Stop(string p,string s):base(s){phase=p;}}
 static byte[] Read(ulong at,int n){
  if(at<0x10000||at>0x7fffffffffffUL||n<1||n>8192)throw new Stop("waiting","Waiting for initialized player data.");
  byte[] b=new byte[n];UIntPtr done;
  if(!ReadProcessMemory(handle,(IntPtr)(long)at,b,(UIntPtr)n,out done)||done.ToUInt64()!=(ulong)n)throw new Stop("waiting","Waiting for readable player data.");return b;
 }
 static uint D(byte[] b,int o=0){return BitConverter.ToUInt32(b,o);}static ulong Q(byte[] b,int o=0){return BitConverter.ToUInt64(b,o);}
 static void Write(ulong at,byte[] bytes){
  if((bytes.Length!=1&&bytes.Length!=4&&bytes.Length!=8)||at<0x10000||at>0x7fffffffffffUL)throw new Stop("write-failed","Scaler write request was rejected.");
  UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)at,bytes,(UIntPtr)bytes.Length,out done)||done.ToUInt64()!=(ulong)bytes.Length)throw new Stop("write-failed","Scaler write failed. Recovery data was retained.");
 }
 static long Creation(){long c,e,k,u;if(!GetProcessTimes(handle,out c,out e,out k,out u)||e!=0)throw new Stop("waiting","Game session ended or is not ready.");return c;}
 static void Plain(string file){for(string p=Path.GetFullPath(file);p!=null;p=Path.GetDirectoryName(p)){if((File.Exists(p)||Directory.Exists(p))&&(File.GetAttributes(p)&FileAttributes.ReparsePoint)!=0)throw new Stop("error","Linked scaler storage is not supported.");}}
 static void Save(string file,ScalerJournal journal){
  Plain(file);Directory.CreateDirectory(Path.GetDirectoryName(file));string temp=file+".tmp";
  Plain(temp);if(File.Exists(temp))File.Delete(temp);
  byte[] b=Encoding.UTF8.GetBytes(json.Serialize(journal));
  using(var stream=new FileStream(temp,FileMode.CreateNew,FileAccess.Write,FileShare.None)){stream.Write(b,0,b.Length);stream.Flush(true);}
  if(File.Exists(file))File.Replace(temp,file,null);else File.Move(temp,file);
 }
 static ScalerJournal Load(string file){Plain(file);if(!File.Exists(file))return null;if(new FileInfo(file).Length>262144)throw new Stop("error","Player size recovery data is invalid.");var j=json.Deserialize<ScalerJournal>(File.ReadAllText(file));ScalerPolicy.Validate(j);return j;}
 static object NoGame(string file,ScalerJournal j){if(j!=null&&j.phase!="ended"){j.phase="ended";Save(file,j);}return new{phase="no-game",message="Saved — waiting for NBA 2K27.",writes=0};}
 static bool Running(int pid){
  if(pid<1)return false;
  try{using(var process=Process.GetProcessById(pid))return !process.HasExited;}
  catch(ArgumentException){return false;}
  catch(InvalidOperationException){return false;}
 }
 static ulong InputRva=0xa38f19c;
 static ulong SettingsRva=0xaac79a0;
 static ulong PlayerTableRva=0xaac79b0;
 static ulong WrapperVtable=0x75bd1f8;
 static ulong ManagerVtable=0x681d088;
 static ulong CharacterVtable=0x6829528;
 static ulong StaffListRva=0xA37BFB0;
 static ulong ManagerNodeVtable=0x681CCC8;
 static ulong CoachVtable=0x6804E58;
 static ulong CoachDescriptionVtable=0x6804818;
 static bool PopulationSupported;
 static ulong GenericVtable,GenericDescriptionVtable,CheerleaderVtable,CheerleaderDescriptionVtable,ScaleMethodRva;
 static string activeBuild="6A974D13:368C3000";
 static void Build(){
  var b=Read(basis,4096);int pe=(int)D(b,60);
  if(b[0]!=77||b[1]!=90||pe<64||pe>3800||D(b,pe)!=0x4550||BitConverter.ToUInt16(b,pe+4)!=0x8664)throw new Stop("unsupported","This game update is not supported by Scaler. No memory changed.");
  bool updated=D(b,pe+8)==0x6AA2013D&&D(b,pe+80)==0x36558000;
  if(!updated&&(D(b,pe+8)!=0x6A974D13||D(b,pe+80)!=0x368C3000))throw new Stop("unsupported","This game update is not supported by Scaler. No memory changed.");
  activeBuild=updated?"6AA2013D:36558000":"6A974D13:368C3000";
  InputRva=updated?0xA39319CUL:0xa38f19cUL;
  SettingsRva=updated?0xAACB9A0UL:0xaac79a0UL;
  PlayerTableRva=updated?0xAACB9B0UL:0xaac79b0UL;
  WrapperVtable=updated?0x75C0D58UL:0x75bd1f8UL;
  ManagerVtable=updated?0x6820088UL:0x681d088UL;
  CharacterVtable=updated?0x682C528UL:0x6829528UL;
  StaffListRva=updated?0xA37FFB0UL:0xA37BFB0UL;
  ManagerNodeVtable=updated?0x681FCC8UL:0x681CCC8UL;
  CoachVtable=updated?0x6807E58UL:0x6804E58UL;
  CoachDescriptionVtable=updated?0x6807818UL:0x6804818UL;
  PopulationSupported=updated;
  GenericVtable=0x68180D8UL;GenericDescriptionVtable=0x6817E08UL;
  CheerleaderVtable=0x6801DE8UL;CheerleaderDescriptionVtable=0x6855FD8UL;
  ScaleMethodRva=0x6DE310UL;
  foreach(var a in new[]{new{r=updated?0x6DE44AU:0x6de34aU,b=updated?"C5FA10054A4DCB09":"C5FA10054A0ECB09"},new{r=updated?0x6DE76EU:0x6de66eU,b=updated?"C5FA1005264ACB09":"C5FA1005260BCB09"}}){if(BitConverter.ToString(Read(basis+a.r,8)).Replace("-","")!=a.b)throw new Stop("unsupported","Player size code differs from the supported build. No memory changed.");}
  Region r;ulong at=basis+InputRva;
  if(VirtualQueryEx(handle,(IntPtr)(long)at,out r,(UIntPtr)Marshal.SizeOf(typeof(Region)))==UIntPtr.Zero||r.State!=0x1000||r.Type!=0x1000000||(ulong)r.Allocation.ToInt64()!=basis||(r.Protect!=4&&r.Protect!=8)||at+4>(ulong)r.Address.ToInt64()+r.Size.ToUInt64())throw new Stop("waiting","Waiting for the initialized player size input.");
 }
 // Conservative table validation. Unknown wrappers/transitions are not empty.
 static bool Loaded(){
  ulong settings=Q(Read(basis+SettingsRva,8));var mode=Read(settings+0xa04,4);uint m=D(mode);
  if(m!=0&&m!=4&&m!=6)throw new Stop("waiting","Waiting for the main menu, Freestyle or Blacktop.");
  var table=Read(basis+PlayerTableRva,30*0x50);bool loaded=false;
  for(int i=0;i<30;i++){
   int o=i*0x50;ulong vt=Q(table,o),manager=Q(table,o+0x10),roster=Q(table,o+0x30);
   if(vt==0&&manager==0&&roster==0)continue;
   if(vt!=basis+WrapperVtable)throw new Stop("waiting","Waiting for a stable player table.");
   if(manager==0&&roster==0)continue;
   if(manager==0||roster==0||Q(Read(manager,8))!=basis+ManagerVtable)throw new Stop("waiting","Players are loading; waiting for stable data.");
   ulong character=Q(Read(manager+0x98,8));
   if(Q(Read(character,8))!=basis+CharacterVtable||Q(Read(roster+0x518,8))!=manager)throw new Stop("waiting","Waiting for verified player objects.");
   var scale=Read(character+0xbcc,8);float s=BitConverter.ToSingle(scale,0),inverse=BitConverter.ToSingle(scale,4);
   if(float.IsNaN(s)||float.IsInfinity(s)||s<=0.1||s>=10||Math.Abs(s*inverse-1)>0.001||float.IsNaN(inverse)||float.IsInfinity(inverse)||Q(Read(manager+0x98,8))!=character)throw new Stop("waiting","Waiting for stable player size data.");
   loaded=true;
  }
  if(Q(Read(basis+SettingsRva,8))!=settings||!Read(settings+0xa04,4).SequenceEqual(mode)||!Read(basis+PlayerTableRva,table.Length).SequenceEqual(table))throw new Stop("waiting","The game is changing modes; waiting.");
  return loaded||m!=0;
 }
 static bool SameCoach(CoachOptIn a,CoachOptIn b){return a.manager==b.manager&&a.actor==b.actor&&a.description==b.description;}
 static bool SameArena(ArenaCharacterOptIn a,ArenaCharacterOptIn b){return a.kind==b.kind&&a.manager==b.manager&&a.actor==b.actor&&a.description==b.description;}
 static float F(uint bits){return BitConverter.ToSingle(BitConverter.GetBytes(bits),0);}static uint B(float value){return BitConverter.ToUInt32(BitConverter.GetBytes(value),0);}
 static bool ScalePair(uint scale,uint inverse){float s=F(scale),i=F(inverse);return !float.IsNaN(s)&&!float.IsInfinity(s)&&!float.IsNaN(i)&&!float.IsInfinity(i)&&s>0.1f&&s<10f&&Math.Abs(s*i-1f)<0.001f;}
 static bool Near(uint a,uint b){return Math.Abs(F(a)-F(b))<0.0005f;}
 static bool OwnedArenaScale(ArenaCharacterOptIn c,uint scale,uint inverse){
  if(!ScalePair(scale,inverse)||!ScalePair(c.originalScale,c.originalInverse))return false;
  float ratio=F(scale)/F(c.originalScale);if(Math.Abs(ratio-1f)<0.0005f)return true;
  for(int p=95;p<100;p++)if(Math.Abs(ratio-F(ScalerPolicy.Target(p)))<0.0005f)return true;
  return false;
 }
 static ulong Description(ulong actor){
  ulong iface=actor+0x78,vt=Q(Read(iface,8)),getter=Q(Read(vt+0x158,8));byte[] code=Read(getter,8);
  if(code[0]!=0x48||code[1]!=0x8D||code[2]!=0x81||code[7]!=0xC3)throw new Stop("waiting","Arena characters are loading; waiting for stable data.");
  long description=(long)iface+BitConverter.ToInt32(code,3);if(description<0x10000)return 0;return (ulong)description;
 }
 static bool ArenaClass(string kind,ulong actor,ulong description){
  ulong actorVtable=kind=="generic"?GenericVtable:CheerleaderVtable,descriptionVtable=kind=="generic"?GenericDescriptionVtable:CheerleaderDescriptionVtable;
  if(Q(Read(actor,8))!=basis+actorVtable||Description(actor)!=description||Q(Read(description,8))!=basis+descriptionVtable)return false;
  ulong ifaceVtable=Q(Read(actor+0x78,8));return Q(Read(ifaceVtable+0x188,8))==basis+ScaleMethodRva;
 }
 static bool ActiveCoach(CoachOptIn c){
  try{return c!=null&&c.description==c.actor+0xF20&&Q(Read(c.manager,8))==basis+ManagerVtable&&Q(Read(c.manager+0x98,8))==c.actor&&Q(Read(c.actor,8))==basis+CoachVtable&&Q(Read(c.description,8))==basis+CoachDescriptionVtable&&Read(c.manager+0xB8,1)[0]==0;}
  catch(Stop){return false;}
 }
 sealed class CharacterScan {internal List<CoachOptIn> coaches=new List<CoachOptIn>();internal List<ArenaCharacterOptIn> arena=new List<ArenaCharacterOptIn>();}
 static bool ActiveArena(ArenaCharacterOptIn c){
  try{return c!=null&&PopulationSupported&&Q(Read(c.manager,8))==basis+ManagerVtable&&Q(Read(c.manager+0x98,8))==c.actor&&Read(c.manager+0xB8,1)[0]==0&&ArenaClass(c.kind,c.actor,c.description);}
  catch(Stop){return false;}
 }
 static CharacterScan Characters(){
  var found=new CharacterScan();ulong sentinel=basis+StaffListRva,node=Q(Read(sentinel+0x10,8)),previous=sentinel;
  for(int count=0;count<256&&node!=sentinel;count++){
   if(node<0x10000||Q(Read(node,8))!=basis+ManagerNodeVtable||Q(Read(node+8,8))!=previous)throw new Stop("waiting","Arena characters are loading; waiting for stable data.");
   ulong manager=node+0x78,actor=Q(Read(manager+0x98,8));
   if(actor>=0x10000&&Q(Read(manager,8))==basis+ManagerVtable&&Q(Read(actor,8))==basis+CoachVtable){
    var coach=new CoachOptIn{manager=manager,actor=actor,description=actor+0xF20};
    if(Q(Read(coach.description,8))!=basis+CoachDescriptionVtable||Read(manager+0xB8,1)[0]!=0)throw new Stop("waiting","Coaches are loading; waiting for stable data.");
    found.coaches.Add(coach);
   }else if(PopulationSupported&&actor>=0x10000&&Q(Read(manager,8))==basis+ManagerVtable&&Read(manager+0xB8,1)[0]==0){
    ulong actorVtable=Q(Read(actor,8));string kind=actorVtable==basis+GenericVtable?"generic":actorVtable==basis+CheerleaderVtable?"cheerleader":null;
    if(kind!=null){ulong description=Description(actor);if(!ArenaClass(kind,actor,description))throw new Stop("waiting","Arena characters are loading; waiting for stable data.");found.arena.Add(new ArenaCharacterOptIn{kind=kind,manager=manager,actor=actor,description=description});}
   }
   previous=node;node=Q(Read(node+0x10,8));
  }
  if(node!=sentinel||Q(Read(sentinel+8,8))!=previous)throw new Stop("waiting","Arena character list changed during inspection.");
  return found;
 }
 static int SyncCoaches(string file,ScalerJournal j,bool enabled,List<CoachOptIn> discovered){
  var tracked=new List<CoachOptIn>();
  foreach(var old in j.coaches)if(ActiveCoach(old)&&discovered.Any(c=>SameCoach(c,old)))tracked.Add(old);
  bool journalChanged=tracked.Count!=j.coaches.Length;
  if(!enabled){
   int restoreWrites=0;
   foreach(var coach in tracked){
    byte current=Read(coach.description+0x7D,1)[0],applied=(byte)(coach.originalFlag|2);
    if(current==applied){Write(coach.description+0x7D,new[]{coach.originalFlag});if(Read(coach.description+0x7D,1)[0]!=coach.originalFlag)throw new Stop("write-failed","Coach size flag readback failed. Recovery data was retained.");restoreWrites++;}
    else if(current!=coach.originalFlag)throw new Stop("conflict","A tracked coach size flag changed outside Scaler. No memory changed.");
   }
   j.coaches=new CoachOptIn[0];Save(file,j);return restoreWrites;
  }
  int writes=0;
  foreach(var coach in discovered){
   var owned=tracked.FirstOrDefault(c=>SameCoach(c,coach));
   if(owned==null){
    byte original=Read(coach.description+0x7D,1)[0];if((original&2)!=0)continue;
    coach.originalFlag=original;tracked.Add(coach);j.coaches=tracked.ToArray();Save(file,j);owned=coach;journalChanged=false;
   }
   byte current=Read(owned.description+0x7D,1)[0],target=(byte)(owned.originalFlag|2);
   if(current==owned.originalFlag){
    if(!ActiveCoach(owned))throw new Stop("waiting","Coach changed during scaling; waiting.");
    Write(owned.description+0x7D,new[]{target});if(Read(owned.description+0x7D,1)[0]!=target)throw new Stop("write-failed","Coach size flag readback failed. Recovery data was retained.");writes++;
   }else if(current!=target)throw new Stop("conflict","A tracked coach size flag changed outside Scaler. No memory changed.");
  }
  if(journalChanged){j.coaches=tracked.ToArray();Save(file,j);}return writes;
 }
 static bool ArenaEnabled(ArenaCharacterOptIn actor,bool enabled,int categories){return enabled&&(actor.kind=="generic"?(categories&4)!=0:(categories&2)!=0);}
 static int SyncArena(string file,ScalerJournal j,bool enabled,int categories,uint sharedTarget,List<ArenaCharacterOptIn> discovered){
  if(!PopulationSupported)return 0;
  var tracked=new List<ArenaCharacterOptIn>();foreach(var old in j.arenaCharacters)if(ActiveArena(old)&&discovered.Any(c=>SameArena(c,old)))tracked.Add(old);
  bool changed=tracked.Count!=j.arenaCharacters.Length;int writes=0;
  var retained=new List<ArenaCharacterOptIn>();
  foreach(var actor in tracked){
   if(!ArenaEnabled(actor,enabled,categories)){
    byte flag=Read(actor.description+0x7D,1)[0],targetFlag=(byte)(actor.originalFlag|2);
    if(flag==targetFlag){Write(actor.description+0x7D,new[]{actor.originalFlag});if(Read(actor.description+0x7D,1)[0]!=actor.originalFlag)throw new Stop("write-failed","Arena character size flag readback failed. Recovery data was retained.");writes++;}
    else if(flag!=actor.originalFlag)throw new Stop("conflict","A tracked arena character size flag changed outside Scaler. No memory changed.");
    byte[] pair=Read(actor.actor+0xBCC,8);uint scale=D(pair),inverse=D(pair,4);
    if(scale!=actor.originalScale||inverse!=actor.originalInverse){
     if(!ScalePair(scale,inverse))throw new Stop("conflict","A tracked arena character size became invalid. Recovery data was retained.");
     // If our exact seed is still present, restore the exact captured pair. If
     // the game has already recalculated a prepared actor, clearing the owned
     // flag is sufficient and avoids overwriting its native individual scale.
     if(scale==actor.targetScale&&inverse==actor.targetInverse){Write(actor.actor+0xBCC,BitConverter.GetBytes(((ulong)actor.originalInverse<<32)|actor.originalScale));pair=Read(actor.actor+0xBCC,8);if(D(pair)!=actor.originalScale||D(pair,4)!=actor.originalInverse)throw new Stop("write-failed","Arena character size readback failed. Recovery data was retained.");writes++;}
     else if(!actor.cachePrepared&&!OwnedArenaScale(actor,scale,inverse))throw new Stop("conflict","A tracked arena character size changed before preparation completed. Recovery data was retained.");
    }
    changed=true;
   }else retained.Add(actor);
  }
  tracked=retained;
  if(!enabled||((categories&6)==0)){
   if(changed){j.arenaCharacters=tracked.ToArray();Save(file,j);}return writes;
   }
  float factor=F(sharedTarget);
  foreach(var actor in discovered){
   if(!ArenaEnabled(actor,enabled,categories))continue;
   var owned=tracked.FirstOrDefault(c=>SameArena(c,actor));
   if(owned==null){
    byte originalFlag=Read(actor.description+0x7D,1)[0];if((originalFlag&2)!=0)continue;
    byte[] pair=Read(actor.actor+0xBCC,8);uint scale=D(pair),inverse=D(pair,4);if(!ScalePair(scale,inverse))throw new Stop("waiting","Arena character sizes are loading; waiting for stable data.");
    actor.originalFlag=originalFlag;actor.originalScale=scale;actor.originalInverse=inverse;tracked.Add(actor);owned=actor;changed=true;
   }
   uint desiredScale=B(F(owned.originalScale)*factor),desiredInverse=B(1f/F(desiredScale));
   if(owned.targetScale!=desiredScale||owned.targetInverse!=desiredInverse){owned.targetScale=desiredScale;owned.targetInverse=desiredInverse;owned.cachePrepared=false;changed=true;}
  }
  if(changed){j.arenaCharacters=tracked.ToArray();Save(file,j);}
  bool preparedChanged=false;
  foreach(var actor in tracked){
   if(!ActiveArena(actor))throw new Stop("waiting","Arena character changed during scaling; waiting.");
   byte flag=Read(actor.description+0x7D,1)[0],targetFlag=(byte)(actor.originalFlag|2);
   if(flag==actor.originalFlag){Write(actor.description+0x7D,new[]{targetFlag});if(Read(actor.description+0x7D,1)[0]!=targetFlag)throw new Stop("write-failed","Arena character size flag readback failed. Recovery data was retained.");writes++;}
   else if(flag!=targetFlag)throw new Stop("conflict","A tracked arena character size flag changed outside Scaler. No memory changed.");
   byte[] pair=Read(actor.actor+0xBCC,8);uint scale=D(pair),inverse=D(pair,4);
   if(!ScalePair(scale,inverse)||(!actor.cachePrepared&&!OwnedArenaScale(actor,scale,inverse)))throw new Stop("conflict","A tracked arena character size changed before preparation completed. Recovery data was retained.");
   if(!actor.cachePrepared){
    if(scale!=actor.targetScale||inverse!=actor.targetInverse){Write(actor.actor+0xBCC,BitConverter.GetBytes(((ulong)actor.targetInverse<<32)|actor.targetScale));pair=Read(actor.actor+0xBCC,8);if(D(pair)!=actor.targetScale||D(pair,4)!=actor.targetInverse)throw new Stop("write-failed","Arena character size readback failed. Recovery data was retained.");writes++;}
    actor.cachePrepared=true;preparedChanged=true;
   }
  }
  if(preparedChanged){j.arenaCharacters=tracked.ToArray();Save(file,j);}
  return writes;
 }
 static int SyncCharacters(string file,ScalerJournal j,bool enabled,int categories,uint target){var scan=Characters();return SyncCoaches(file,j,enabled&&(categories&1)!=0,scan.coaches)+SyncArena(file,j,enabled,categories,target,scan.arena);}
 static object Run(string[] args){
  int percent,categories;
  if(args.Length!=5||!new[]{"apply","observe","restore","check"}.Contains(args[0])||!int.TryParse(args[3],out percent)||!int.TryParse(args[4],out categories)||categories<0||categories>7)throw new Stop("error","Invalid player size request.");
  if(args[0]=="restore")categories=0;
  uint target=ScalerPolicy.Target(args[0]=="restore"?100:percent);string file=Path.GetFullPath(args[2]),expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  string mutexName;using(var sha=SHA256.Create())mutexName="Local\\2kLauncherScaler-"+BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(file.ToUpperInvariant()))).Replace("-","");
  using(var gate=new Mutex(false,mutexName)){
   bool locked=false;try{try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}if(!locked)throw new Stop("waiting","A player size operation is already running.");
    var j=args[0]=="check"?null:Load(file);var games=Process.GetProcessesByName("NBA2K27");
    try{
     if(games.Length==0)return NoGame(file,j);
     if(games.Length!=1)throw new Stop("error","More than one NBA 2K27 process is running.");
     var game=games[0];int gameId=-1;string image;long created;
     try{
      gameId=game.Id;if(game.HasExited)return NoGame(file,j);
      var module=game.MainModule;image=Path.GetFullPath(module.FileName);created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)module.BaseAddress.ToInt64();
     }catch(InvalidOperationException){return NoGame(file,j);}
     catch(Win32Exception){if(!Running(gameId))return NoGame(file,j);throw new Stop("error","Cannot inspect NBA 2K27. Use the same permission level for the game and launcher.");}
     if(!String.Equals(image,expected,StringComparison.OrdinalIgnoreCase))throw new Stop("error","The running game does not match the launcher game folder.");
     bool write=args[0]!="check";
     if(write){foreach(var p in Process.GetProcesses()){using(p){if(p.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Stop("error","Use the launcher's without-EAC session for Scaler.");}}}
     handle=OpenProcess(write?0x438U:0x410U,false,game.Id);if(handle==IntPtr.Zero){if(!Running(game.Id))return NoGame(file,j);throw new Stop("error","Cannot access NBA 2K27. Use the same permission level for the game and launcher.");}
     if(Creation()!=created)throw new Stop("waiting","The game process changed; waiting for the new session.");
     Build();
     if(args[0]=="check")return new{phase="verified",build=activeBuild,loaded=Loaded(),current=D(Read(basis+InputRva,4)),writes=0};
     bool same=ScalerPolicy.Same(j,game.Id,created,basis,image,activeBuild);uint current=D(Read(basis+InputRva,4));
     if(!same){
      if(j!=null&&j.phase!="ended"){j.phase="ended";Save(file,j);}
      if(args[0]=="restore")return new{phase="ended",message="Previous game session ended. Nothing to restore.",writes=0};
      if(args[0]=="observe")return new{phase="new-session",message="Waiting for a launcher-managed game session.",writes=0};
      if(current==0)throw new Stop("waiting","Waiting for the original player size input to initialize.");
      if(current!=ScalerPolicy.Original)throw new Stop("conflict","Player size was changed outside Scaler. No memory changed.");
      j=new ScalerJournal{pid=game.Id,created=created,basis=basis,address=basis+InputRva,image=image,build=activeBuild};
     }
     if(!ScalerPolicy.Owned(current,j.applied,j.requested))throw new Stop("conflict","Player size differs from this session's saved values. No memory changed.");
     if(args[0]=="observe"){
      bool reload=j.reloadRequired;
      try{if(!Loaded()){reload=false;if(j.reloadRequired){j.reloadRequired=false;Save(file,j);}}}catch(Stop){/* Observation never weakens write readiness. */}
      if(current!=j.requested)return new{phase="reset",message="The game reset player size. Apply the saved size again before loading a mode.",writes=0};
      int characterWrites=SyncCharacters(file,j,current!=ScalerPolicy.Original,categories,current);
      return new{phase=reload?"reload":"applied",message=reload?"Reload the mode to update player and referee size. Loaded selected arena groups update automatically.":"Shared size applied to the selected arena groups.",writes=characterWrites};
     }
     bool loaded=Loaded();
     if(current==target&&same){j.requested=target;j.applied=target;if(!loaded)j.reloadRequired=false;j.phase=target==ScalerPolicy.Original?"restored":"applied";Save(file,j);int characterWrites=SyncCharacters(file,j,target!=ScalerPolicy.Original,categories,target);return new{phase=j.reloadRequired?"reload":"applied",message=j.reloadRequired?"Reload the mode to update player and referee size. Loaded selected arena groups update automatically.":"Shared size is already applied to the selected arena groups.",writes=characterWrites};}
     uint before=current;j.applied=before;j.requested=target;j.phase="prepared";j.reloadRequired=loaded;Save(file,j);
     Build();bool loadedAtWrite=Loaded();
     if(loadedAtWrite!=loaded){j.reloadRequired=true;Save(file,j);}
     if(game.HasExited||Creation()!=created||game.StartTime.ToUniversalTime().ToFileTimeUtc()!=created||D(Read(j.address,4))!=before)throw new Stop("conflict","The game or player size changed during preparation. No memory changed.");
     Write(j.address,BitConverter.GetBytes(target));
     if(D(Read(j.address,4))!=target)throw new Stop("write-failed","Player size readback failed. Recovery data was retained.");
     try{if(Loaded())j.reloadRequired=true;}catch(Stop){j.reloadRequired=true;}
     j.applied=target;j.phase=target==ScalerPolicy.Original?"restored":"applied";Save(file,j);
     int writes=1+SyncCharacters(file,j,target!=ScalerPolicy.Original,categories,target);
     return new{phase=j.reloadRequired?"reload":"applied",message=j.reloadRequired?"Reload the mode to update player and referee size. Loaded selected arena groups update automatically.":"Shared size applied to the selected arena groups.",writes=writes};
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var p in games)p.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
 public static int Main(string[] args){try{Console.WriteLine(json.Serialize(Run(args)));return 0;}catch(Stop e){Console.WriteLine(json.Serialize(new{phase=e.phase,message=e.Message}));return 0;}catch(Exception){Console.WriteLine(json.Serialize(new{phase="error",message="Scaler could not complete safely. No further writes will be attempted; recovery data was retained."}));return 1;}}
}
