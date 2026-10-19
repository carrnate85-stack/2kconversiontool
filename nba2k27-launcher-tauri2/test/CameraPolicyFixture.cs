using System;
using System.IO;
using System.Text;
using System.Linq;
using System.Reflection;
using System.Globalization;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text.RegularExpressions;
using System.CodeDom.Compiler;
using Microsoft.CSharp;
using System.Web.Script.Serialization;

// Only pure static WPF camera transforms are invoked. No Form is instantiated,
// no process is discovered/opened, and no native allocator is called.
class CameraPolicyFixture {
 const int N=0x4990;
 static Type policy;
 static byte[] Pattern(int length,int seed){return Enumerable.Range(0,length).Select(i=>(byte)((i+seed)%251)).ToArray();}
 static void D(byte[] b,int o,int v){Buffer.BlockCopy(BitConverter.GetBytes(v),0,b,o,4);}
 static void Q(byte[] b,int o,long v){Buffer.BlockCopy(BitConverter.GetBytes(v),0,b,o,8);}
 static void F(byte[] b,int o,float v){Buffer.BlockCopy(BitConverter.GetBytes(v),0,b,o,4);}
 static byte[] Parameters(int seed,float[] framing){var b=Pattern(N,seed);foreach(int start in new[]{0x10,0x30})for(int i=0;i<5;i++)F(b,start+i*4,framing[i]);return b;}
 static byte[] Entry(int seed,int kind,long a,long b){var r=Pattern(64,seed);D(r,8,kind);Q(r,0x30,a);Q(r,0x38,b);return r;}
 static string Hash(byte[] b){if(b==null)return null;using(var h=SHA256.Create())return BitConverter.ToString(h.ComputeHash(b)).Replace("-","");}
 static MethodInfo Method(string name){return policy.GetMethod(name,BindingFlags.NonPublic|BindingFlags.Static);}
 static float Bad(string kind){return kind=="nan"?float.NaN:kind=="infinity"?float.PositiveInfinity:99f;}
 static object Metadata(int seed,int badWord,int length){var b=Pattern(length,seed);if(length==52)for(int i=0;i<6;i++)D(b,i*4,new[]{1,1,1,10,1,10}[i]);if(badWord>=0)D(b,badWord*4,99);var output=(byte[])Method("UnrestrictedMetadata").Invoke(null,new object[]{b});return new{seed,badWord,length,accepted=output!=null,hash=Hash(output)};}
 static object Press(int seed,int origin,int offset,string bad,int kind,bool tolerantW){
  var target=Entry(11,8,0x140200000L,0x140200000L);var donor=Entry(31,kind,0x140100000L,0x140105000L);
  var source=Parameters(seed,new float[]{-1350,400,0,1,45});var a=Parameters(seed+1,new float[]{-1300,400,0,1,50});var b=Parameters(seed+2,new float[]{-1300,400,0,1,50});
  if(origin>=0)F(new[]{source,a,b}[origin],offset,Bad(bad));
  if(tolerantW){F(a,0x1c,0.995f);F(a,0x3c,1.005f);F(b,0x1c,1.005f);F(b,0x3c,0.995f);}
  object[] call={target,donor,source,a,b,0x20000000L,0x20005000L,null,null,null};
  bool accepted=(bool)Method("Press27TrackingClone").Invoke(null,call);
  return new{seed,origin,offset,bad,kind,tolerantW,accepted,entry=Hash((byte[])call[7]),primary=Hash((byte[])call[8]),secondary=Hash((byte[])call[9])};
 }
 static object Beluba(int seed,int origin,int offset,string bad){
  var source=Parameters(seed,new float[]{-1900,1200,1700,1,38});var side=Parameters(seed+1,new float[]{-1750,675,0,1,70});
  if(origin>=0)F(origin==0?source:side,offset,Bad(bad));
  var output=(byte[])Method("BelubaSideParameters").Invoke(null,new object[]{source,side});
  return new{seed,origin,offset,bad,accepted=output!=null,hash=Hash(output)};
 }
 static object Steps(string source,string fixture){
  var expression=Regex.Match(source,@"float targetY=baseY\+[^;]+;").Value;
  if(expression.Length==0||!expression.Contains("zoomStep"))throw new Exception("Original step expression not found");
  string code="public static class OriginalCameraStepMath { public static int[] Run(float baseY,float heightStep,float targetZ,float baseFov,float zoomStep,int height,int zoom){"+expression+"return new[]{System.BitConverter.ToInt32(System.BitConverter.GetBytes(targetY),0),System.BitConverter.ToInt32(System.BitConverter.GetBytes(targetZ),0),System.BitConverter.ToInt32(System.BitConverter.GetBytes(fov),0)};} }";
  var options=new CompilerParameters(new[]{"System.dll"},Path.Combine(fixture,"OriginalStepMath.dll"));
  options.GenerateExecutable=false;options.TempFiles=new TempFileCollection(fixture,true);
  var provider=new CSharpCodeProvider();var compiled=provider.CompileAssemblyFromSource(options,code);
  if(compiled.Errors.HasErrors)throw new Exception("Original step expression compilation failed");
  var run=compiled.CompiledAssembly.GetType("OriginalCameraStepMath").GetMethod("Run");
  var rows=new List<object>();var ids=new HashSet<int>();
  foreach(Match call in Regex.Matches(source,@"ApplyMappedSteps\((\d+),([^;]+)\);")){
   int id=int.Parse(call.Groups[1].Value);var tokens=call.Groups[2].Value.Split(',');if(tokens.Length!=5||!ids.Add(id))throw new Exception("Original step calls changed");
   var values=tokens.Select(token=>{float value;if(float.TryParse(token.Trim().TrimEnd('f'),NumberStyles.Float,CultureInfo.InvariantCulture,out value))return value;return (float)policy.GetField(token.Trim(),BindingFlags.NonPublic|BindingFlags.Static).GetRawConstantValue();}).ToArray();
   foreach(int height in new[]{-100,-1,0,1,10,100})foreach(int zoom in new[]{-100,-1,0,1,10,100}){
    var bits=(int[])run.Invoke(null,new object[]{values[0],values[1],values[2],values[3],values[4],height,zoom});
    var bytes=Pattern(N,42);foreach(int start in new[]{0x10,0x30}){D(bytes,start+4,bits[0]);D(bytes,start+8,bits[1]);D(bytes,start+16,bits[2]);}
    rows.Add(new{id,height,zoom,seed=42,hash=Hash(bytes)});
   }
  }
  if(ids.Count!=4||rows.Count!=144)throw new Exception("Original camera-step profiles changed");
  return new{expression,rows};
 }
 static int Main(string[] args){
  if(args.Length!=3)throw new Exception("Expected owned reference DLL, source and fixture paths");
  var assembly=Assembly.LoadFrom(args[0]);policy=assembly.GetTypes().Single(t=>t.Name=="MenuCameraUnlock");
  var metadata=new List<object>();foreach(int seed in new[]{0,1,17,250})metadata.Add(Metadata(seed,-1,52));for(int i=0;i<6;i++)metadata.Add(Metadata(17,i,52));metadata.Add(Metadata(17,-1,4));
  var press=new List<object>();foreach(int seed in new[]{0,1,17,248})press.Add(Press(seed,-1,0,"",3,false));press.Add(Press(17,-1,0,"",3,true));press.Add(Press(17,-1,0,"",8,false));
  var beluba=new List<object>();foreach(int seed in new[]{0,1,17,249})beluba.Add(Beluba(seed,-1,0,""));
  foreach(int offset in new[]{0x10,0x14,0x18,0x1c,0x20,0x30,0x34,0x38,0x3c,0x40})foreach(string bad in new[]{"value","nan","infinity"}){
   for(int origin=0;origin<3;origin++)press.Add(Press(17,origin,offset,bad,3,false));
   for(int origin=0;origin<2;origin++)beluba.Add(Beluba(17,origin,offset,bad));
  }
  var report=new{schema="nba2k27-camera-policy-oracle-v1",gameAccess=false,originalSourceUnmodified=true,metadata,press,beluba,steps=Steps(File.ReadAllText(args[1]),args[2])};
  Console.WriteLine(new JavaScriptSerializer{MaxJsonLength=4194304}.Serialize(report));return 0;
 }
}
