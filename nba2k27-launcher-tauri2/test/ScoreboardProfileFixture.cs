using System;
using System.Linq;
using System.Collections.Generic;
using System.Web.Script.Serialization;
internal static class ScoreboardProfileFixture {
 static byte[] Scalar(uint key,uint value){var b=new byte[96];Array.Copy(BitConverter.GetBytes(key),b,4);Array.Copy(BitConverter.GetBytes(0x800005U),0,b,4,4);Array.Copy(BitConverter.GetBytes(value),0,b,8,4);for(int i=12;i<96;i++)b[i]=(byte)i;return b;}
 static string Hex(byte[] b){return BitConverter.ToString(b).Replace("-","").ToLowerInvariant();}
 static int Main(){
  var choices=ScoreboardCatalog.Choices.Select(c=>new {id=c.Key,resource=c.Value,file=ScoreboardCatalog.Files[c.Value],profile=ScoreboardCatalog.Profiles[c.Value]}).ToArray();
  var vectors=new List<object>();
  foreach(var from in choices)foreach(var to in choices)foreach(bool legacy in new[]{false,true}){
   var p=from.profile;
   var fields=new Dictionary<ulong,byte[]>{{0x10000,Scalar(from.resource==ScoreboardProfile.Stock?ScoreboardProfile.Name:ScoreboardProfile.Resource,from.resource)},{0x10100,Scalar(ScoreboardProfile.Class,legacy?0x2A22DFCC:p[0])},{0x10200,Scalar(legacy?ScoreboardProfile.Mode:p[1],legacy?0xAFA29832:p[2])}};
   uint current;var changes=ScoreboardProfile.Plan(fields,to.resource,out current);
   vectors.Add(new {from=from.id,to=to.id,legacy,current,changes=changes.Select(c=>new {address=c.Address,before=Hex(c.Before.Take(12).ToArray()),after=Hex(c.After)}).ToArray()});
  }
  var json=new JavaScriptSerializer{MaxJsonLength=4*1024*1024};
  Console.WriteLine(json.Serialize(new {schema="nba2k27-original-scoreboard-oracle-v1",gameAccess=false,choices,vectors}));return 0;
 }
}
