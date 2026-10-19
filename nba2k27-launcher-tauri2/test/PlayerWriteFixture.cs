// Build-time only. Original helper writes are directed exclusively at this
// fixture process's own VirtualAlloc buffer, never an NBA2K27 process.
using System;using System.Collections.Generic;using System.Diagnostics;using System.Globalization;using System.IO;using System.Reflection;using System.Runtime.InteropServices;using System.Text;using System.Web.Script.Serialization;
namespace NBA2K27RosterResearch {
internal static class PlayerWriteFixture {
 [DllImport("kernel32.dll",SetLastError=true)]static extern IntPtr VirtualAlloc(IntPtr address,UIntPtr size,uint allocation,uint protection);
 [DllImport("kernel32.dll",SetLastError=true)]static extern bool VirtualFree(IntPtr address,UIntPtr size,uint free);
 static object Call(string name,params object[] args){try{return typeof(Program).GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static).Invoke(null,args);}catch(TargetInvocationException e){throw e.InnerException;}}
 static void Set(byte[] a,int at,byte[] b){Array.Copy(b,0,a,at,b.Length);}
 static void Packed(byte[] a,int at,int shift,int bits,uint value){uint mask=((1u<<bits)-1)<<shift;Set(a,at,BitConverter.GetBytes((BitConverter.ToUInt32(a,at)&~mask)|((value<<shift)&mask)));}
 static void Name(byte[] a,int at,string s){Array.Clear(a,at,40);Set(a,at,Encoding.Unicode.GetBytes(s));}
 static string Hex(byte[] a){return BitConverter.ToString(a).Replace("-", "");}
 static uint[] Values(byte[] record){var a=new uint[RatingSchema.Fields.GetLength(0)];for(int i=0;i<a.Length;i++){int bits=RatingSchema.Fields[i,2];a[i]=(BitConverter.ToUInt32(record,RatingSchema.Fields[i,0])>>RatingSchema.Fields[i,1])&(bits==32?UInt32.MaxValue:(1u<<bits)-1);}return a;}
 static Dictionary<string,object> Cached(Player p,byte[] record){var bio=PlayerBio.Read(record);return new Dictionary<string,object>{{"source","live-roster"},{"recordIndex",p.Index},{"firstName",p.FirstName},{"lastName",p.LastName},{"cyberfaceId",p.CyberfaceId},{"positionId",p.PositionId},{"secondaryPositionId",p.SecondaryPositionId},{"jerseyNumberRaw",p.JerseyNumberRaw},{"potentialRaw",p.PotentialRaw},{"potential",p.Potential},{"heightRaw",p.HeightRaw},{"wingspanRaw",p.WingspanRaw},{"heightCm",p.HeightCm},{"wingspanCm",p.WingspanCm},{"weightLbs",Double.Parse(p.WeightLbs.Value.ToString("0.##",CultureInfo.InvariantCulture),CultureInfo.InvariantCulture)},{"ratingsRaw",Values(record)},{"bioLayoutVerified",true},{"birthYear",bio.Year},{"birthMonth",bio.Month},{"birthDay",bio.Day},{"birthDate",bio.DateText()},{"yearsPro",bio.YearsPro},{"fromIndex",-1},{"audio",new {fullNameId=-1}}};}
 static Dictionary<string,object> Input(Player p){return new Dictionary<string,object>{{"recordIndex",p.Index},{"expectedFirstName",p.FirstName},{"expectedLastName",p.LastName},{"expectedCyberfaceId",p.CyberfaceId},{"firstName",p.FirstName},{"lastName",p.LastName},{"cyberfaceId",p.CyberfaceId},{"positionId",p.PositionId},{"secondaryPositionId",p.SecondaryPositionId},{"jerseyNumberRaw",p.JerseyNumberRaw},{"weightLbs",Double.Parse(p.WeightLbs.Value.ToString("0.##",CultureInfo.InvariantCulture),CultureInfo.InvariantCulture)},{"heightCm",p.HeightCm},{"wingspanCm",p.WingspanCm},{"potential",p.Potential}};}
 static object Run(){
  const int size=0x800000,stride=0x4F8;IntPtr allocation=VirtualAlloc(IntPtr.Zero,(UIntPtr)size,0x3000,4);if(allocation==IntPtr.Zero)throw new Exception("Own allocation failed");long basis=allocation.ToInt64(),table=basis+0x680,appearance=basis+0x30000;
  try{
   byte[] image=new byte[size];Set(image,0x60,BitConverter.GetBytes((long)100));Set(image,0x68,BitConverter.GetBytes(table));
   for(int i=0;i<100;i++){Name(image,0x680+i*stride,"Source");Name(image,0x680+i*stride+40,i==0?"QA":"QA"+i);}
   byte[] record=new byte[stride];new Random(731).NextBytes(record);Name(record,0,"Source");Name(record,40,"QA");
   Set(record,0x60,BitConverter.GetBytes(0L));Set(record,0x78,BitConverter.GetBytes(appearance));Set(record,0xB0,BitConverter.GetBytes(0L));Set(record,0x108,BitConverter.GetBytes(185.12345f));Set(record,0x110,BitConverter.GetBytes((ushort)4090));Set(record,0x128,BitConverter.GetBytes((ushort)42));
   Packed(record,0x12D,0,3,0);Packed(record,0x12D,3,3,1);Packed(record,0x115,3,7,30);record[0x45D]=199;
   Packed(record,0x112,0,12,1988);Packed(record,0x36B,4,4,3);Packed(record,0x114,1,5,14);Packed(record,0x3E2,1,5,17);Set(record,0x2E8,BitConverter.GetBytes((short)-1));Set(record,0x1AC,BitConverter.GetBytes(91.5f));
   byte[] measure={0xA3,0x4D,0xF5,0x50}; // 19875 cm/100 and 20725 cm/100.
   Set(image,0x680,record);Set(image,0x30000,measure);Marshal.Copy(image,0,allocation,size);
   Player p;using(var reader=new Reader(Process.GetCurrentProcess().Id)){p=((List<Player>)Call("ReadPlayers",reader,1,table,new List<Team>(),0L))[0];}
   var baseline=Cached(p,record);var basic=new List<object>();
   for(int c=0;c<4;c++){
    Marshal.Copy(record,0,new IntPtr(table),record.Length);Marshal.Copy(measure,0,new IntPtr(appearance),4);
    var input=Input(p);if(c==1){input["firstName"]="Željko";input["lastName"]="O’Neal";input["cyberfaceId"]=8011;input["positionId"]=4;input["secondaryPositionId"]=5;input["jerseyNumberRaw"]=100;input["weightLbs"]=250.25;input["heightCm"]=220.125;input["wingspanCm"]=245.875;input["potential"]=110;}
    if(c==2){input["firstName"]="";input["lastName"]=new string('A',19);input["jerseyNumberRaw"]=0;input["weightLbs"]=70;input["heightCm"]=120;input["wingspanCm"]=120;input["potential"]=25;}
    if(c==3){input["weightLbs"]=500;input["heightCm"]=260;input["wingspanCm"]=320;input["potential"]=26;}
    Func<string,string> text=key=>Convert.ToString(input[key],CultureInfo.InvariantCulture);
    int potential=Convert.ToInt32(input["potential"]),potentialRaw=potential==p.Potential?p.PotentialRaw:potential<=25?0:Math.Min(255,(potential-25)*3-1);
    var args=new[]{"--pid",Process.GetCurrentProcess().Id.ToString(),"--record-index","0","--expected-first",p.FirstName,"--expected-last",p.LastName,"--expected-cyberface",p.CyberfaceId.ToString(),"--expected-position",p.PositionId.ToString(),"--expected-secondary",p.SecondaryPositionId.ToString(),"--expected-jersey",p.JerseyNumberRaw.ToString(),"--expected-potential-raw",p.PotentialRaw.ToString(),"--expected-height-raw",p.HeightRaw.ToString(),"--expected-wingspan-raw",p.WingspanRaw.ToString(),"--first",text("firstName"),"--last",text("lastName"),"--cyberface",text("cyberfaceId"),"--position",text("positionId"),"--secondary",text("secondaryPositionId"),"--jersey",text("jerseyNumberRaw"),"--weight",text("weightLbs"),"--height-raw",Math.Floor(Convert.ToDouble(input["heightCm"])*100+0.5).ToString(),"--wingspan-raw",Math.Floor(Convert.ToDouble(input["wingspanCm"])*100+0.5).ToString(),"--potential-raw",potentialRaw.ToString()};
    var capture=new StringWriter(CultureInfo.InvariantCulture);var old=Console.Out;Console.SetOut(capture);try{Call("RunPlayerEdit",(object)args);}finally{Console.SetOut(old);}
    byte[] after=new byte[stride],ap=new byte[4];Marshal.Copy(new IntPtr(table),after,0,stride);Marshal.Copy(new IntPtr(appearance),ap,0,4);
    basic.Add(new {input,before=Hex(record),appearance=Hex(measure),cached=baseline,after=Hex(after),afterAppearance=Hex(ap),preserveUnchangedFloat=c==0,report=new JavaScriptSerializer().DeserializeObject(capture.ToString())});
   }
   var fields=new List<object>();
   for(int i=0;i<RatingSchema.Fields.GetLength(0);i++)foreach(int value in new[]{0,Math.Max(0,RatingSchema.EditMax[i])}){
    string changes=i+":"+Values(record)[i]+":"+value;try{fields.Add(new {changes,ok=true,after=Hex((byte[])Call("PatchRatingFields",record,changes))});}catch{fields.Add(new {changes,ok=false});}
   }
   var bio=new List<object>();foreach(string birthday in new[]{"2000-02-29","1900-02-29","4095-12-31","1988-03-14"})foreach(int years in new[]{0,17,31,32}){
    try{var patches=PlayerBio.Plan(record,birthday,1988,3,14,years,17);var after=(byte[])record.Clone();foreach(var patch in patches)Set(after,patch.Offset,patch.After);bio.Add(new {birthday,years,ok=true,after=Hex(after),patches=patches.Count});}catch{bio.Add(new {birthday,years,ok=false});}
   }
   return new {before=Hex(record),appearance=Hex(measure),cached=baseline,input=Input(p),basic,fields,bio};
  }finally{if(!VirtualFree(allocation,UIntPtr.Zero,0x8000))throw new Exception("Own allocation release failed");}
 }
 static int Main(){try{Console.OutputEncoding=new UTF8Encoding(false);Console.WriteLine(new JavaScriptSerializer{MaxJsonLength=16000000}.Serialize(Run()));return 0;}catch(Exception e){Console.Error.WriteLine(e);return 1;}}
}
}
