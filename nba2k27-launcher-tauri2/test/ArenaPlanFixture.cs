using System;
using System.Collections.Generic;
using System.Reflection;
using System.Web.Script.Serialization;

// Development-only reflection oracle. Never calls ArenaTool.Main, Connect,
// Read, Write, process discovery or native APIs. Only its original pure helpers.
internal static class ArenaPlanFixture {
 static int Main(){
  var type=typeof(ArenaPlanFixture).Assembly.GetType("ArenaTool",true);
  var flags=BindingFlags.Static|BindingFlags.NonPublic;
  var fields=type.GetMethod("TemplateFields",flags);
  var build=type.GetMethod("SelectBuild",flags);
  var cases=new List<object>();
  Action<string,string> sample=(file,floor)=>{
   try {var bytes=(byte[])fields.Invoke(null,new object[]{file,floor});cases.Add(new{file,floor,accepted=true,bytesHex=BitConverter.ToString(bytes).Replace("-","")});}
   catch(TargetInvocationException){cases.Add(new{file,floor,accepted=false,bytesHex=(string)null});}
  };
  sample("arena_700_int","arena_700_int_floor");
  sample("arena_brickleyfacility_int","arena_brickleyfacility_int_floor");
  sample(new string('a',33),new string('a',33)+"_floor");
  sample(new string('a',34),new string('a',34)+"_floor");
  sample("arena","floor");sample("../arena","../arena_floor");
  sample("ARENA","ARENA_floor");sample("arena\0","arena\0_floor");
  sample("arena-1","arena-1_floor");sample("aréna","aréna_floor");
  sample("","_floor");sample(null,"arena_floor");sample("arena",null);
  var builds=new List<object>();
  foreach(bool updated in new[]{false,true}){
   build.Invoke(null,new object[]{updated});var values=new Dictionary<string,object>();
   foreach(string name in new[]{"RosterRootRva","MappingRva","BlacktopRva","BlacktopVtable","ModeCurrent","ModeAlternate","ModeTable"})values.Add(name,type.GetField(name,flags).GetValue(null));
   builds.Add(new{updated,values});
  }
  Console.WriteLine(new JavaScriptSerializer().Serialize(new{schema="nba2k27-arena-plan-oracle-v1",gameAccess=false,stockHex=BitConverter.ToString((byte[])type.GetField("Stock",flags).GetValue(null)).Replace("-",""),cases,builds}));return 0;
 }
}
