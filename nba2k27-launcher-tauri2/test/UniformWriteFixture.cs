// Original WPF uniform writer against this fixture's own private allocation.
using System;using System.Collections.Generic;using System.Diagnostics;using System.Globalization;using System.IO;using System.Reflection;using System.Runtime.InteropServices;using System.Text;using System.Web.Script.Serialization;
namespace NBA2K27RosterResearch {
internal static class UniformWriteFixture {
 [DllImport("kernel32.dll",SetLastError=true)]static extern IntPtr VirtualAlloc(IntPtr a,UIntPtr n,uint type,uint protection);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool VirtualFree(IntPtr a,UIntPtr n,uint type);
 static object Call(string name,params object[] args){try{return typeof(Program).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);}catch(TargetInvocationException e){throw e.InnerException;}}
 static void Set(byte[] b,int at,byte[] v){Array.Copy(v,0,b,at,v.Length);}
 static void Text(byte[] b,int at,string s,int len,bool wide=true){Array.Clear(b,at,len);Set(b,at,wide?Encoding.Unicode.GetBytes(s):Encoding.ASCII.GetBytes(s));}
 static string Hex(byte[] b,int at,int n){return BitConverter.ToString(b,at,n).Replace("-","");}
 static string Value(object o){return Convert.ToString(o,CultureInfo.InvariantCulture);}
 static uint U(object o){return Convert.ToUInt32(o,CultureInfo.InvariantCulture);}
 static object ReadTeam(Reader reader,long basis,int size){
  object[] aa={reader,basis,(long)size,0L};var ar=(List<Arena>)Call("ReadArenas",aa);var ur=(Dictionary<long,TeamUniform>)Call("ReadUniforms",reader,basis,(long)size);object[] tt={reader,basis,(long)size,ar,(long)aa[3],ur,0L};var tr=(List<Team>)Call("ReadTeams",tt);
  var capture=new StringWriter(CultureInfo.InvariantCulture);var old=Console.Out;Console.SetOut(capture);try{Call("WriteReport",0,basis,(long)size,100,basis+0x680,new List<Player>(),(long)tt[6],tr,ur,(long)aa[3],ar,"[]");}finally{Console.SetOut(old);}var report=(Dictionary<string,object>)new JavaScriptSerializer().DeserializeObject(capture.ToString());return ((object[])report["teams"])[0];
 }
 static object Case(int c,object[] fields){
  const int size=0x800000,teams=0x21000,arenas=0x26000,uniforms=0x30000,ts=0x1550,us=0x170;
  IntPtr allocation=VirtualAlloc(IntPtr.Zero,(UIntPtr)size,0x3000,4);if(allocation==IntPtr.Zero)throw new Exception("Own uniform allocation failed");long basis=allocation.ToInt64();
  try {
   byte[] image=new byte[size];Set(image,0x60,BitConverter.GetBytes((long)100));Set(image,0x68,BitConverter.GetBytes(basis+0x680));
   for(int i=0;i<100;i++){Text(image,0x680+i*0x4F8,"Source",40);Text(image,0x680+i*0x4F8+40,"QA"+i,40);}
   Set(image,0xD0,BitConverter.GetBytes((long)3));Set(image,0xD8,BitConverter.GetBytes(basis+teams));Set(image,0xB0,BitConverter.GetBytes((long)3));Set(image,0xB8,BitConverter.GetBytes(basis+arenas));Set(image,0x290,BitConverter.GetBytes((long)4));Set(image,0x298,BitConverter.GetBytes(basis+uniforms));
   for(int i=0;i<3;i++){int at=teams+i*ts;Text(image,at+0x2FA,i==0?"QA Team":"Other "+i,48);Set(image,at+0xF20,BitConverter.GetBytes((ushort)(42+i)));Set(image,at+0xEE0,BitConverter.GetBytes(basis+arenas+i*0x1278));Text(image,arenas+i*0x1278,"Arena "+i,66);Set(image,arenas+i*0x1278+0x792,BitConverter.GetBytes((ushort)i));}
   Set(image,teams+0xDE0,BitConverter.GetBytes(basis+uniforms));Set(image,teams+0xDE8,BitConverter.GetBytes(basis+uniforms));Set(image,teams+ts+0xDE0,BitConverter.GetBytes(basis+uniforms+us));
   for(int i=0;i<4;i++){
    int at=uniforms+i*us;byte[] row=new byte[us];new Random(177+i).NextBytes(row);Set(image,at,row);Set(image,at,BitConverter.GetBytes(0L));Set(image,at+0x50,BitConverter.GetBytes((ushort)(77+i)));Set(image,at+0x52,BitConverter.GetBytes((ushort)(42+i)));
    Text(image,at+8,i==0?"uniform_qa":"uniform_other"+i,64,false);Text(image,at+0x48,"qa",8,false);Text(image,at+0x68,"Uniform "+i,64,false);Text(image,at+0xA8,"Team "+i,40,false);
    for(int f=0;f<fields.Length;f++){var field=(Dictionary<string,object>)fields[f];int offset=Convert.ToInt32(field["offset"]),shift=Convert.ToInt32(field["shift"]);uint mask=U(field["mask"]),max=U(field["maximum"]);uint value=(bool)field["color"]?(0x7F112233u+(uint)(f*0x010305)):((uint)f%(max+1));uint old=BitConverter.ToUInt32(image,at+offset);Set(image,at+offset,BitConverter.GetBytes((old&~(mask<<shift))|((value&mask)<<shift)));}
   }
   Marshal.Copy(image,0,allocation,size);
   var input=new Dictionary<string,object>{{"recordIndex",0},{"teamId",42},{"expectedTeamName","QA Team"},{"slot",0},{"uniformId",77},{"expectedBaseFilename","uniform_qa"}};
   foreach(object obj in fields){var f=(Dictionary<string,object>)obj;string key=(string)f["request"];uint value=(BitConverter.ToUInt32(image,uniforms+Convert.ToInt32(f["offset"]))>>Convert.ToInt32(f["shift"]))&U(f["mask"]);input[key]=value;if((bool)f["color"])input["expected"+Char.ToUpperInvariant(key[0])+key.Substring(1)]=value;}
   input["expectedHome"]=input["home"];input["expectedAway"]=input["away"];input["expectedLogo"]=input["logo"];
   foreach(string s in new[]{"160","164","168","16C"})input["expectedFlags"+s]=BitConverter.ToUInt32(image,uniforms+Int32.Parse(s,NumberStyles.HexNumber));
   if(c==1||c==2)foreach(object obj in fields){var f=(Dictionary<string,object>)obj;input[(string)f["request"]]=c==1?U(f["maximum"]):0u;}
   if(c>=3&&c<107){int field=(c-3)/2;var f=(Dictionary<string,object>)fields[field];input[(string)f["request"]]=(c-3)%2==0?U(f["maximum"]):0u;}
   if(c==107)input["expectedTeamName"]="Wrong team";if(c==108)input["teamId"]=43;if(c==109)input["uniformId"]=78;if(c==110)input["expectedBaseFilename"]="wrong_uniform";if(c==111)input["slot"]=31;
   if(c==112)input["expectedFlags160"]=U(input["expectedFlags160"])^1u;if(c==113)input["expectedPrimaryColor"]=U(input["expectedPrimaryColor"])^1u;if(c==114)input["expectedHome"]=U(input["expectedHome"])^1u;
   object cached;
   using(var reader=new Reader(Process.GetCurrentProcess().Id)){cached=ReadTeam(reader,basis,size);}
   var args=new List<string>{"--pid",Process.GetCurrentProcess().Id.ToString()};
   foreach(var pair in new[]{new[]{"recordIndex","record-index"},new[]{"teamId","team-id"},new[]{"expectedTeamName","expected-team-name"},new[]{"slot","slot"},new[]{"uniformId","uniform-id"},new[]{"expectedBaseFilename","expected-base-filename"},new[]{"expectedHome","expected-home"},new[]{"expectedAway","expected-away"},new[]{"expectedLogo","expected-logo"}}){args.Add("--"+pair[1]);args.Add(Value(input[pair[0]]));}
   foreach(object obj in fields){var f=(Dictionary<string,object>)obj;string key=(string)f["request"],argument=(string)f["argument"];args.Add("--"+argument);args.Add(Value(input[key]));if((bool)f["color"]){args.Add("--expected-"+argument);args.Add(Value(input["expected"+Char.ToUpperInvariant(key[0])+key.Substring(1)]));}}
   foreach(string s in new[]{"160","164","168","16C"}){args.Add("--expected-flags-"+s.ToLowerInvariant());args.Add(Value(input["expectedFlags"+s]));}
   object result=null;string error=null;var output=new StringWriter(CultureInfo.InvariantCulture);var previous=Console.Out;Console.SetOut(output);try{Call("RunTeamUniformEdit",(object)args.ToArray());result=new JavaScriptSerializer().DeserializeObject(output.ToString());}catch(Exception e){error=e.Message;}finally{Console.SetOut(previous);}
   byte[] after=new byte[us],teamAfter=new byte[ts],allUniformsAfter=new byte[4*us];Marshal.Copy(new IntPtr(basis+uniforms),after,0,us);Marshal.Copy(new IntPtr(basis+teams),teamAfter,0,ts);Marshal.Copy(new IntPtr(basis+uniforms),allUniformsAfter,0,allUniformsAfter.Length);
   object refreshed;using(var reader=new Reader(Process.GetCurrentProcess().Id)){refreshed=ReadTeam(reader,basis,size);}
   return new{input,cached,refreshed,ok=error==null,error,report=result,regionBase=Value(basis),regionSize=size,teamTable=Value(basis+teams),teamCount=3,arenaTable=Value(basis+arenas),arenaCount=3,uniformTable=Value(basis+uniforms),uniformCount=4,before=Hex(image,uniforms,us),after=Hex(after,0,us),teamBefore=Hex(image,teams,ts),teamAfter=Hex(teamAfter,0,ts),allUniformsBefore=Hex(image,uniforms,4*us),allUniformsAfter=Hex(allUniformsAfter,0,allUniformsAfter.Length)};
  }finally{if(!VirtualFree(allocation,UIntPtr.Zero,0x8000))throw new Exception("Own uniform allocation release failed");}
 }
 static int Main(string[] args){try{if(args.Length!=1)throw new Exception("Use explicit layout file");var layout=(Dictionary<string,object>)new JavaScriptSerializer().DeserializeObject(File.ReadAllText(args[0]));var fields=(object[])layout["fields"];if(fields.Length!=52)throw new Exception("Unexpected uniform field count");var cases=new List<object>();for(int i=0;i<115;i++)cases.Add(Case(i,fields));Console.OutputEncoding=new UTF8Encoding(false);Console.WriteLine(new JavaScriptSerializer{MaxJsonLength=32000000}.Serialize(cases));return 0;}catch(Exception e){Console.Error.WriteLine(e);return 1;}}
}
}
