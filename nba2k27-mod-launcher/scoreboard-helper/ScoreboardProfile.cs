using System;
using System.Linq;
using System.Collections.Generic;

// Pure planner: no process access. Three existing scalar headers only;
// no allocation, list-link edits, hooks, or controller calls.
internal static class ScoreboardProfile {
 internal const uint Stock=0x39264490,Name=0xFE11D138,Resource=0x45E79640,Class=0x2C8A369B,Mode=0xFEC8E933,Handler=0xC63510F0;
 internal sealed class Field { public ulong Address; public byte[] Before,After; }
 static uint D(byte[] b,int n){return BitConverter.ToUInt32(b,n);}
 static byte[] Header(byte[] b,uint key,uint value){var r=b.Take(12).ToArray();Array.Copy(BitConverter.GetBytes(key),0,r,0,4);Array.Copy(BitConverter.GetBytes(value),0,r,8,4);return r;}
 internal static List<Field> Plan(Dictionary<ulong,byte[]> fields,uint wanted,out uint current){
  var keys=new HashSet<uint>();
  foreach(var b in fields.Values)if(b.Length!=96||!keys.Add(D(b,0)))throw new Exception("Invalid or duplicate scoreboard properties.");
  var resource=fields.Where(f=>D(f.Value,0)==Name||D(f.Value,0)==Resource).ToArray();
  var cls=fields.Where(f=>D(f.Value,0)==Class).ToArray();
  var mode=fields.Where(f=>D(f.Value,0)==Mode||D(f.Value,0)==Handler).ToArray();
  if(resource.Length!=1||cls.Length!=1||mode.Length!=1)throw new Exception("Scoreboard descriptor fields could not be verified.");
  var selected=new[]{resource[0],cls[0],mode[0]};
  if(selected.Any(f=>(D(f.Value,4)&15)!=5))throw new Exception("Unexpected scoreboard scalar type.");
  current=D(resource[0].Value,8);
  if(!ScoreboardCatalog.Profiles.ContainsKey(current)||!ScoreboardCatalog.Profiles.ContainsKey(wanted)||(D(resource[0].Value,0)==Name&&current!=Stock))throw new Exception("Unknown scoreboard resource.");
  var profile=ScoreboardCatalog.Profiles[current];
  uint c=D(cls[0].Value,8),k=D(mode[0].Value,0),v=D(mode[0].Value,8);
  bool legacy=c==0x2A22DFCC&&k==Mode&&v==0xAFA29832;
  if(!legacy&&(c!=profile[0]||k!=profile[1]||v!=profile[2]))throw new Exception("Unknown scoreboard controller configuration. Restart the game.");
  var target=ScoreboardCatalog.Profiles[wanted];
  var values=new[]{Header(resource[0].Value,wanted==Stock?Name:Resource,wanted),Header(cls[0].Value,Class,target[0]),Header(mode[0].Value,target[1],target[2])};
  var result=new List<Field>();
  for(int i=0;i<3;i++)if(!selected[i].Value.Take(12).SequenceEqual(values[i]))result.Add(new Field{Address=selected[i].Key,Before=selected[i].Value,After=values[i]});
  return result;
 }
}
