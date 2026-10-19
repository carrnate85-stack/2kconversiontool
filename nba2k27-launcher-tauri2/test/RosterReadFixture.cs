// Build-time oracle only. Reuses the unchanged WPF reader against this process's
// own synthetic allocation. Never finds, opens or writes an NBA2K27 process.
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text;
using System.Web.Script.Serialization;
namespace NBA2K27RosterResearch {
internal static class RosterReadFixture {
 static object Call(string name,params object[] args) {
  return typeof(Program).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);
 }
 static void Set(byte[] a,int at,byte[] b){Array.Copy(b,0,a,at,b.Length);}
 static void Packed(byte[] a,int at,int shift,int bits,uint value){uint mask=((1u<<bits)-1)<<shift;Set(a,at,BitConverter.GetBytes((BitConverter.ToUInt32(a,at)&~mask)|((value<<shift)&mask)));}
 static void Text(byte[] a,int at,string s,int bytes,bool unicode=true){Array.Clear(a,at,bytes);Set(a,at,unicode?Encoding.Unicode.GetBytes(s):Encoding.ASCII.GetBytes(s));}
 static object Case(int seed) {
  const int size=0x800000,prefix=0x40000,players=0x680,teams=0x21000,arenas=0x26000,uniforms=0x30000;
  IntPtr pointer=Marshal.AllocHGlobal(size);long basis=pointer.ToInt64();
  try {
   byte[] data=new byte[size];var random=new Random(seed);
   Set(data,0x60,BitConverter.GetBytes((long)100));Set(data,0x68,BitConverter.GetBytes(basis+players));
   Set(data,0xD0,BitConverter.GetBytes((long)3));Set(data,0xD8,BitConverter.GetBytes(basis+teams));
   Set(data,0xB0,BitConverter.GetBytes((long)3));Set(data,0xB8,BitConverter.GetBytes(basis+arenas));
   Set(data,0x290,BitConverter.GetBytes((long)6));Set(data,0x298,BitConverter.GetBytes(basis+uniforms));
   for(int i=0;i<100;i++) {
    int at=players+i*0x4F8;byte[] row=new byte[0x4F8];random.NextBytes(row);Set(data,at,row);
    string first=i==0?"Joel":i==1?"Tyrese":i==2?"V.J.":i==3?"Cooper":i==4?"Nikola":i==5?"Élodie":"QA"+i;
    string last=i==0?"Embiid":i==1?"Maxey":i==2?"Edgecombe":i==3?"Flagg":i==4?"Jokić":i==5?"O’Neal":"Fixture";
    if(seed==17&&i==99)last=first="\uFFFD";
    Text(data,at, last,40);Text(data,at+40,first,40);
    Set(data,at+0x60,BitConverter.GetBytes(i%4==3?0L:basis+teams+(i%3)*0x1550));
    Set(data,at+0x78,BitConverter.GetBytes(basis+0x38000+i*8));
    Set(data,0x38000+i*8,BitConverter.GetBytes((ushort)(19000+i*13)));
    Set(data,0x38002+i*8,BitConverter.GetBytes((ushort)(20000+i*17)));
    Set(data,at+0x108,BitConverter.GetBytes(i==98?Single.NaN:180.125f+i));
    Set(data,at+0x110,BitConverter.GetBytes((ushort)(i==0?4090:i==1?8011:i==2?4120:i==3?4117:i)));
    Set(data,at+0x1AC,BitConverter.GetBytes(i==97?Single.PositiveInfinity:75.125f+(i%24)));
    if(i<3){Packed(data,at+0x112,0,12,(uint)(i==0?2000:i==1?1988:1900));Packed(data,at+0x36B,4,4,(uint)(i==1?3:2));Packed(data,at+0x114,1,5,(uint)(i==1?14:29));Packed(data,at+0x3E2,1,5,(uint)(i==0?0:i==1?31:17));}
    if(i==0)Packed(data,at+0x115,3,7,100);
    if(i==96)Set(data,at+0x78,BitConverter.GetBytes(0L));
    if(i==95)Set(data,0x38000+i*8,BitConverter.GetBytes((ushort)11999));
   }
   for(int i=0;i<3;i++) {
    int at=teams+i*0x1550;Text(data,at+0x2FA,i==0?"76ers":i==1?"Lakers":"Aces",48);
    Text(data,at+0x32C,i==0?"Philadelphia":i==1?"Los Angeles":"Las Vegas",36);
    Text(data,at+0x350,i==0?"PHI":i==1?"LAL":"LVA",14);Text(data,at+0x35E,"QA",14);Text(data,at+0x1AE,"logo"+i,72);
    Set(data,at+0xEE0,BitConverter.GetBytes(basis+arenas+i*0x1278));Set(data,at+0xF20,BitConverter.GetBytes((ushort)i));
    data[at+0x123F]=(byte)((i==0?0:i==1?1:28)<<2);Set(data,at+0x154A,BitConverter.GetBytes((ushort)((i==1?77:0)<<3)));
    Set(data,at+0xDE0,BitConverter.GetBytes(basis+uniforms+i*0x170));
    // A valid row with a different team must count as invalid, not as owned.
    Set(data,at+0xDE8,BitConverter.GetBytes(basis+uniforms+((i+1)%3)*0x170));
    int a=arenas+i*0x1278;Text(data,a,i==0?"Wells Fargo":i==1?"The Forum":"Michelob",66);
    Text(data,a+0x7E,"s"+i,38,false);Text(data,a+0xA4,"f"+i,40,false);Set(data,a+0x792,BitConverter.GetBytes((ushort)(10+i)));
   }
   for(int i=0;i<5;i++) {
    int at=uniforms+i*0x170;byte[] row=new byte[0x170];random.NextBytes(row);Set(data,at,row);
    Set(data,at,BitConverter.GetBytes(i==0?basis+0x32000:i==1?-1L:0L));
    Set(data,at+0x50,BitConverter.GetBytes((ushort)(300+i)));Set(data,at+0x52,BitConverter.GetBytes((ushort)i));
    Text(data,at+0x08,"uqa"+i,64,false);Text(data,at+0x48,"sponsor",8,false);Text(data,at+0x68,"Uniform "+i,64,false);Text(data,at+0xA8,"Team "+i,40,false);
   }
   Text(data,0x32008,"Custom QA",64,false);Set(data,0x32000,BitConverter.GetBytes((ushort)44));Set(data,0x32004,BitConverter.GetBytes((uint)0xF1234567));
   Set(data,0x32048,BitConverter.GetBytes((uint)0xDABBCCDD));Set(data,0x3204C,BitConverter.GetBytes((uint)0xFEDCBA98));
   Text(data,0x32050,"baked_jersey.iff",256,false);Text(data,0x32150,"baked_shorts.iff",256,false);
   Marshal.Copy(data,0,pointer,size);
   using(var reader=new Reader(Process.GetCurrentProcess().Id)) {
    object[] aa={reader,basis,(long)size,0L};var arenaRows=(List<Arena>)Call("ReadArenas",aa);long arenaTable=(long)aa[3];
    var uniformRows=(Dictionary<long,TeamUniform>)Call("ReadUniforms",reader,basis,(long)size);
    object[] tt={reader,basis,(long)size,arenaRows,arenaTable,uniformRows,0L};var teamRows=(List<Team>)Call("ReadTeams",tt);long teamTable=(long)tt[6];
    var playerRows=(List<Player>)Call("ReadPlayers",reader,100,basis+players,teamRows,teamTable);
    foreach(var p in playerRows) {
     byte[] row=new byte[0x4F8];Array.Copy(data,p.Index*0x4F8+players,row,0,row.Length);
     var values=new List<uint>();for(int i=0;i<RatingSchema.Fields.GetLength(0);i++){uint raw=BitConverter.ToUInt32(row,RatingSchema.Fields[i,0]);int bits=RatingSchema.Fields[i,2];values.Add((raw>>RatingSchema.Fields[i,1])&(bits==32?UInt32.MaxValue:(1u<<bits)-1));}
     p.RatingsJson=new JavaScriptSerializer().Serialize(values);var bio=PlayerBio.Read(row);p.YearsPro=bio.YearsPro;p.BioLayoutVerified=true;p.BirthYear=bio.Year;p.BirthMonth=bio.Month;p.BirthDay=bio.Day;p.BirthDate=bio.DateText();
     p.ArchetypeKey=(string)Call("ReadCachedArchetypeKey",row);
    }
    var capture=new StringWriter(CultureInfo.InvariantCulture);TextWriter old=Console.Out;Console.SetOut(capture);
    try{Call("WriteReport",0,basis,(long)size,100,basis+players,playerRows,teamTable,teamRows,uniformRows,arenaTable,arenaRows,"[]");}finally{Console.SetOut(old);}
    byte[] stored=new byte[prefix];Array.Copy(data,stored,prefix);
    return new {regionBase=basis.ToString(CultureInfo.InvariantCulture),regionSize=size,hex=BitConverter.ToString(stored).Replace("-", ""),expected=new JavaScriptSerializer {MaxJsonLength=16000000}.DeserializeObject(capture.ToString())};
   }
  } finally {Marshal.FreeHGlobal(pointer);}
 }
 static int Main(){try{Console.OutputEncoding=new UTF8Encoding(false);Console.WriteLine(new JavaScriptSerializer {MaxJsonLength=16000000}.Serialize(new[]{Case(1),Case(17),Case(29)}));return 0;}catch(Exception e){Console.Error.WriteLine(e);return 1;}}
}
}
