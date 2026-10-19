using System;
using System.Linq;
using System.Runtime.InteropServices;

// Confirmed native category3 path. One process-lifetime RX bridge; only the
// unused dispatch entry and label key are enabled while on Blacktop selection.
internal static partial class FreestyleTeams {
 [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr VirtualAllocEx(IntPtr h,IntPtr at,UIntPtr bytes,uint kind,uint protection);
 [DllImport("kernel32.dll")] static extern bool FlushInstructionCache(IntPtr h,IntPtr at,UIntPtr bytes);
 static uint BcTable=0x73b6630,BcLabel=0xac686b0,BcContinue=0x22de8fb;
 const ulong BcEuro=0x6fc058b0442652cd,BcWomen=0x0f39a44c3426c962;
 static byte[] BcOriginal=new uint[]{0x22de8e8,0x22de8f6,0x22de8ef,0x22de974,0x22de94a,0x22de953}.SelectMany(v=>BitConverter.GetBytes(unchecked((int)(v-BcTable)))).ToArray();
 static ulong bcBridge;static byte[] bcPatched;static bool bcRouteOwned,bcLabelOwned;
 static DateTime bcNextCheck=DateTime.MinValue;static ulong bcRoot,bcOwner,bcTable;static int bcCount;static string bcLast="";
 internal static bool BcRosterSupported(int[] teams){return teams!=null&&teams.Length>=2&&teams.Length<=32&&teams.All(id=>id>=0&&id<=1023)&&teams.Distinct().Count()==teams.Length;}
 static void BcStatus(string phase,string message){if(bcLast!=phase+message){bcLast=phase+message;BtEmit(phase,message);}}
 static bool BcSelection(){return BtMode(FsModeCurrent)==0x92df06de&&BtMode(FsModeAlternate)==0x92df06de&&Q(Read(basis+BtRoot,8))==basis+BtVtable&&D(Read(basis+BtRoot+0xea8,4))==3;}
 static void BcWrite(ulong at,byte[] bytes){Identity();UIntPtr done;if(!WriteProcessMemory(handle,(IntPtr)(long)at,bytes,(UIntPtr)bytes.Length,out done)||done.ToUInt64()!=(ulong)bytes.Length||!Read(at,bytes.Length).SequenceEqual(bytes))throw new Stop("write-failed","WNBA category write was not verified. Close the game.");}
 static void BcDispatch(byte[] expected,byte[] next){
  Identity();var area=Area(basis+BcTable,24,0x1000000,false);
  if((ulong)area.Allocation.ToInt64()!=basis||area.Protect!=2||!Read(basis+BcTable,24).SequenceEqual(expected))throw new Stop("conflict","Blacktop category table changed outside this launcher.");
  uint old;if(!VirtualProtectEx(handle,(IntPtr)(long)(basis+BcTable+12),(UIntPtr)4,4,out old))throw new Stop("write-failed","Cannot prepare WNBA category.");
  try{if(old!=2||!Read(basis+BcTable,24).SequenceEqual(expected))throw new Stop("conflict","Category protection or ownership changed.");BcWrite(basis+BcTable+12,next.Skip(12).Take(4).ToArray());if(!Read(basis+BcTable,24).SequenceEqual(next))throw new Stop("write-failed","Category readback differs.");}
  finally{uint ignored;if(!VirtualProtectEx(handle,(IntPtr)(long)(basis+BcTable+12),(UIntPtr)4,old,out ignored))throw new Stop("write-failed","Close the game: category protection restoration failed.");}
 }
 static void BcSetLabel(ulong expected,ulong next){
  Identity();var area=Area(basis+BcLabel,16,0x1000000,true);var data=Read(basis+BcLabel,16);
  if((ulong)area.Allocation.ToInt64()!=basis||D(data)!=0xba3c982d||D(data,4)!=0||Q(data,8)!=expected)throw new Stop("conflict","Category label changed outside this launcher.");
  BcWrite(basis+BcLabel+8,BitConverter.GetBytes(next));
 }
 static void BcRestore(){
  try{if(bcRouteOwned){var now=Read(basis+BcTable,24);if(!now.SequenceEqual(BcOriginal))BcDispatch(bcPatched,BcOriginal);bcRouteOwned=false;}}
  finally{if(bcLabelOwned){ulong now=Q(Read(basis+BcLabel+8,8));if(now!=BcEuro)BcSetLabel(BcWomen,BcEuro);bcLabelOwned=false;}}
 }
 static void BcPrepareBridge(){
  if(bcBridge!=0)return;
  for(ulong offset=0x40000000;offset<0x50000000;offset+=0x10000){var at=VirtualAllocEx(handle,(IntPtr)(long)((basis+offset)&~0xffffUL),(UIntPtr)4096,0x3000,4);if(at!=IntPtr.Zero){bcBridge=(ulong)at.ToInt64();break;}}
  if(bcBridge==0)throw new Stop("error","No nearby WNBA category allocation is available.");
  long jump=checked((long)(basis+BcContinue)-(long)(bcBridge+10)),entry=checked((long)bcBridge-(long)(basis+BcTable));
  var code=new byte[]{0xbe,0,0,0,0x20,0xe9}.Concat(BitConverter.GetBytes(checked((int)jump))).ToArray();BcWrite(bcBridge,code);
  uint old;if(!VirtualProtectEx(handle,(IntPtr)(long)bcBridge,(UIntPtr)4096,0x20,out old)||!FlushInstructionCache(handle,(IntPtr)(long)bcBridge,(UIntPtr)code.Length))throw new Stop("write-failed","Cannot prepare the WNBA category bridge.");
  bcPatched=(byte[])BcOriginal.Clone();Array.Copy(BitConverter.GetBytes(checked((int)entry)),0,bcPatched,12,4);
  // Never free code that another game thread could still be finishing.
 }
 static void BcTick(){
  if(!BcSelection()){BcRestore();bcNextCheck=DateTime.MinValue;bcRoot=0;bcLast="";return;}
  ulong root=Q(Read(basis+FsRosterRoot,8));Area(root,0xb0,0x20000,false);ulong owner=Q(Read(root+0xa8,8));var ownerArea=Area(owner,0x90,0x20000,false);
  byte[] header=Read(owner+0x80,16);int count=(int)D(header);ulong table=Q(header,8);
  bool same=root==bcRoot&&owner==bcOwner&&table==bcTable&&count==bcCount;
  if(!same)BcRestore();
  if(same&&DateTime.UtcNow<bcNextCheck)return;
  bcNextCheck=DateTime.UtcNow.AddSeconds(1);bcRoot=root;bcOwner=owner;bcTable=table;bcCount=count;
  if(count<1||count>1024||Area(table,(ulong)count*0x1550,0x20000,false).Allocation!=ownerArea.Allocation)throw new Stop("waiting","Waiting for a stable Blacktop roster.");
  byte[] rows=Read(table,count*0x1550);
  var women=Enumerable.Range(0,count).Where(i=>D(rows,i*0x1550+0x123c)>>26==29).Select(i=>(int)BitConverter.ToUInt16(rows,i*0x1550+0xf20)).OrderBy(i=>i).ToArray();
  if(!BcRosterSupported(women)){int usable=women.Distinct().Count();BcRestore();BcStatus("unavailable","This loaded roster exposes only "+usable+" usable WNBA team"+(usable==1?"":"s")+"; at least two are required. Your roster is left unchanged.");return;}
  if(!BcSelection()||Q(Read(basis+FsRosterRoot,8))!=root||Q(Read(root+0xa8,8))!=owner||!Read(owner+0x80,16).SequenceEqual(header)||!Read(table,rows.Length).SequenceEqual(rows))throw new Stop("waiting","Roster changed during category preparation.");
  if(!bcRouteOwned){
   var labelData=Read(basis+BcLabel,16);
   if(D(labelData)==0||Q(labelData,8)==0)throw new Stop("waiting","Waiting for Blacktop category labels to initialize.");
   if(D(labelData)!=0xba3c982d||D(labelData,4)!=0||Q(labelData,8)!=BcEuro)throw new Stop("conflict","Blacktop category label differs from the original.");
   BcPrepareBridge();if(!BcSelection())return;
   bcLabelOwned=true;BcSetLabel(BcEuro,BcWomen);
   bcRouteOwned=true;BcDispatch(BcOriginal,bcPatched);
  }
  BcStatus("applied",women.Length+" WNBA teams are available through Blacktop's Y category button.");
 }
}
