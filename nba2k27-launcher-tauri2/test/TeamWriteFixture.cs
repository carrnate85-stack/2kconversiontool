// Build-time oracle: original team writer, exclusively its own synthetic buffer.
using System;using System.Collections.Generic;using System.Diagnostics;using System.Globalization;using System.IO;using System.Reflection;using System.Runtime.InteropServices;using System.Text;using System.Web.Script.Serialization;
namespace NBA2K27RosterResearch {
internal static class TeamWriteFixture {
 [DllImport("kernel32.dll",SetLastError=true)]static extern IntPtr VirtualAlloc(IntPtr address,UIntPtr size,uint allocation,uint protection);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool VirtualFree(IntPtr address,UIntPtr size,uint free);
 static object Call(string name,params object[] args){try{return typeof(Program).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);}catch(TargetInvocationException e){throw e.InnerException;}}
 static void Set(byte[] a,int at,byte[] b){Array.Copy(b,0,a,at,b.Length);}
 static void Text(byte[] a,int at,string s,int len,bool unicode=true){Array.Clear(a,at,len);Set(a,at,unicode?Encoding.Unicode.GetBytes(s):Encoding.ASCII.GetBytes(s));}
 static string Hex(byte[] a){return BitConverter.ToString(a).Replace("-","");}
 static byte[] Slice(byte[] a,int at,int len){byte[] b=new byte[len];Array.Copy(a,at,b,0,len);return b;}
 static object Case(int c,string backup) {
  const int size=0x800000,teams=0x21000,arenas=0x2A000,ts=0x1550,ars=0x1278,count=6;
  IntPtr allocation=VirtualAlloc(IntPtr.Zero,(UIntPtr)size,0x3000,4);if(allocation==IntPtr.Zero)throw new Exception("Own team buffer failed");long basis=allocation.ToInt64();
  try {
   byte[] image=new byte[size];Set(image,0x60,BitConverter.GetBytes((long)100));Set(image,0x68,BitConverter.GetBytes(basis+0x680));
   for(int i=0;i<100;i++){Text(image,0x680+i*0x4F8,"Source",40);Text(image,0x680+i*0x4F8+40,"QA"+i,40);}
   Set(image,0xD0,BitConverter.GetBytes((long)count));Set(image,0xD8,BitConverter.GetBytes(basis+teams));Set(image,0xB0,BitConverter.GetBytes((long)count));Set(image,0xB8,BitConverter.GetBytes(basis+arenas));
   for(int i=0;i<count;i++){
    int at=teams+i*ts;byte[] row=new byte[ts];new Random(900+i).NextBytes(row);Set(image,at,row);
    // Unknown bytes are deliberately nonzero, but optional pointer slots must
    // be valid nulls rather than random non-canonical signed addresses.
    Array.Clear(image,at+0xDE0,32*8);
    Text(image,at+0x2FA,i==0?"QA Team":"Other "+i,48);Text(image,at+0x32C,"Philadelphia",36);Text(image,at+0x350,"PHI",14);Text(image,at+0x35E,"PA",14);Text(image,at+0x1AE,"logo_qa",72);
    Set(image,at+0xF20,BitConverter.GetBytes((ushort)(42+i)));image[at+0x123F]=0x83;Set(image,at+0x154A,BitConverter.GetBytes((ushort)0xFC07));
    Set(image,at+0xEE0,BitConverter.GetBytes(basis+arenas+(i==1?1:i==2?3:0)*ars));
    at=arenas+i*ars;row=new byte[ars];new Random(1200+i).NextBytes(row);Set(image,at,row);
    Text(image,at,i<2?(i==0?"Rucker":"Philadelphia"):"",66);Text(image,at+0x7E,i<2?"arena_"+i:"",38,false);Text(image,at+0xA4,i<2?(i==0?"rucker_floor":"philly_floor"):"",40,false);
    Set(image,at+0x792,BitConverter.GetBytes((ushort)(i<2?i:994+i)));
   }
   if(c==3)for(int i=2;i<6;i++)Set(image,teams+i*ts+0xEE0,BitConverter.GetBytes(basis+arenas+i*ars));
   if(c==4)Set(image,arenas+4*ars+0x792,BitConverter.GetBytes((ushort)996));
   if(c==5)Set(image,arenas+ars+0x792,BitConverter.GetBytes((ushort)0));
   Marshal.Copy(image,0,allocation,size);
   object[] aa;List<Arena> ar;List<Team> tt;using(var r=new Reader(Process.GetCurrentProcess().Id)){
    aa=new object[]{r,basis,(long)size,0L};ar=(List<Arena>)Call("ReadArenas",aa);object[] ta={r,basis,(long)size,ar,(long)aa[3],new Dictionary<long,TeamUniform>(),0L};tt=(List<Team>)Call("ReadTeams",ta);
   }
   var input=new Dictionary<string,object>{{"recordIndex",0},{"teamId",42},{"expectedName","QA Team"},{"name","QA Team"},{"cityName","Philadelphia"},{"cityShort","PHI"},{"stateShort","PA"},{"mainLogo","logo_qa"},{"rosterTypeId",0},{"historicYearId",0},{"arenaId",0}};
   if(c==1||c==9){input["name"]=c==9?new string('Q',23):"Équipe QA";input["cityName"]=c==9?new string('C',17):"New York";input["cityShort"]="NYC";input["stateShort"]="NY";input["mainLogo"]=c==9?new string('L',35):"logo_new";input["rosterTypeId"]=31;input["historicYearId"]=127;input["arenaId"]=1;input["floorFile"]="philly_floor";}
   if(c==2||c==3||c==4){input["arenaId"]=1;input["floorFile"]="rucker_floor";}
   if(c==5)input["arenaId"]=0;
   if(c==6)input["expectedName"]="Wrong team";
   if(c==7){input["arenaId"]=996;input["floorFile"]="rucker_floor";}
   if(c==8){input["arenaId"]=1;input["floorFile"]="missing_floor";}
   if(c==10){input["arenaId"]=1;input["floorFile"]="é";}
   if(c==11)input["arenaId"]=996;
   Func<string,string> value=k=>Convert.ToString(input[k],CultureInfo.InvariantCulture);
   var args=new List<string>{"--pid",Process.GetCurrentProcess().Id.ToString(),"--record-index","0","--team-id","42","--expected-name",value("expectedName"),"--name",value("name"),"--city-name",value("cityName"),"--city-short",value("cityShort"),"--state-short",value("stateShort"),"--main-logo",value("mainLogo"),"--roster-type",value("rosterTypeId"),"--historic-year",value("historicYearId"),"--arena-id",value("arenaId")};
   if(input.ContainsKey("floorFile")){args.AddRange(new[]{"--floor-file",value("floorFile"),"--backup-root",backup});}
   object report=null;string error=null;var capture=new StringWriter(CultureInfo.InvariantCulture);var old=Console.Out;Console.SetOut(capture);
   try{Call("RunTeamEdit",(object)args.ToArray());report=new JavaScriptSerializer().DeserializeObject(capture.ToString());}catch(Exception e){error=e.Message;}finally{Console.SetOut(old);}
   byte[] after=new byte[ts],arenaAfter=new byte[count*ars];Marshal.Copy(new IntPtr(basis+teams),after,0,ts);Marshal.Copy(new IntPtr(basis+arenas),arenaAfter,0,arenaAfter.Length);
   var references=new string[count];for(int i=0;i<count;i++)references[i]=BitConverter.ToInt64(image,teams+i*ts+0xEE0).ToString(CultureInfo.InvariantCulture);
   return new {input,ok=error==null,error,regionBase=basis.ToString(CultureInfo.InvariantCulture),regionSize=size,teamTable=(basis+teams).ToString(CultureInfo.InvariantCulture),arenaTable=(basis+arenas).ToString(CultureInfo.InvariantCulture),teamCount=count,arenaCount=count,
    before=Hex(Slice(image,teams,ts)),after=Hex(after),arenasBefore=Hex(Slice(image,arenas,count*ars)),arenasAfter=Hex(arenaAfter),references,report};
  }finally{if(!VirtualFree(allocation,UIntPtr.Zero,0x8000))throw new Exception("Own team buffer release failed");}
 }
 static int Main(string[] args){try{if(args.Length!=1)throw new Exception("Use an explicit fixture backup folder");Directory.CreateDirectory(args[0]);var rows=new List<object>();for(int i=0;i<12;i++)rows.Add(Case(i,args[0]));Console.OutputEncoding=new UTF8Encoding(false);Console.WriteLine(new JavaScriptSerializer{MaxJsonLength=16000000}.Serialize(rows));return 0;}catch(Exception e){Console.Error.WriteLine(e);return 1;}}
}
}
