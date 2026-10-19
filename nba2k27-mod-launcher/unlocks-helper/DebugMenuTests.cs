using System;
using System.Collections.Generic;
using System.Linq;
internal static class DebugMenuTests {
 static void Require(bool value){if(!value)throw new Exception("Debug menu policy assertion failed.");}
 static void Reject(Action action){bool failed=false;try{action();}catch{failed=true;}Require(failed);}
 public static int Main(){
  foreach(int count in new[]{0,-1,100001,int.MaxValue})Reject(()=>DebugFeaturePolicy.Count(count));
  DebugFeaturePolicy.Count(1);DebugFeaturePolicy.Count(100000);
  DebugFeaturePolicy.Entry(43,0);DebugFeaturePolicy.Entry(43,1);
  foreach(uint packed in new uint[]{0,11,42,75})Reject(()=>DebugFeaturePolicy.Entry(packed,0));
  Reject(()=>DebugFeaturePolicy.Entry(43,2));
  var bytes=new Dictionary<ulong,byte[]>();ulong table=0x10000;
  var keys=DebugFeaturePolicy.Keys.OrderBy(k=>k).ToArray();
  for(int i=0;i<keys.Length;i++){ulong pointer=0x20000+(ulong)i*32;bytes[table+(ulong)i*8]=BitConverter.GetBytes(pointer);bytes[pointer]=BitConverter.GetBytes(keys[i]);}
  Func<ulong,int,byte[]> read=(at,n)=>{var b=bytes[at];if(b.Length!=n)throw new Exception("Partial read");return b;};
  foreach(uint key in keys)Require(BitConverter.ToUInt32(read(DebugFeaturePolicy.Find(read,table,3,key),4),0)==key);
  Reject(()=>DebugFeaturePolicy.Find(read,table,3,1));
  bytes[table+16]=bytes[table+8];Reject(()=>DebugFeaturePolicy.Find(read,table,3,keys[1]));
  bytes[table+8]=new byte[2];Reject(()=>DebugFeaturePolicy.Find(read,table,3,keys[1]));
  Console.WriteLine("PASS: debug feature hashes, binary lookup, missing/duplicate keys, bounded counts, packed Boolean layout and failed reads");return 0;
 }
}
