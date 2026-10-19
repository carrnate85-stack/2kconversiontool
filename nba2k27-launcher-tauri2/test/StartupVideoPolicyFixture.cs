using System;
using System.Collections.Generic;
using System.Reflection;
using System.Web.Script.Serialization;
// Compile original ControllerSkip/GameVolume sources unchanged, but invoke ONLY
// their pure Edge and MuteLease classes. No Main, XInput, COM or process APIs.
class StartupVideoPolicyFixture {
 static object Pad(bool[] connected,bool[] down,long[] ms) {
  var edge=new ControllerSkip.Edge();var rows=new List<object>();
  for(int i=0;i<ms.Length;i++)rows.Add(new{connected=connected[i],down=down[i],ms=ms[i],skip=edge.Step(connected[i],down[i],ms[i])});return rows;
 }
 static object Mute(bool original,bool externalUnmute) {
  bool muted=original;int changes=0;
  Type type=typeof(GameVolume).GetNestedType("MuteLease",BindingFlags.NonPublic);
  object lease=Activator.CreateInstance(type,BindingFlags.Instance|BindingFlags.NonPublic,null,new object[]{new Func<bool>(()=>muted),new Action<bool>(v=>{muted=v;changes++;}),new Action(()=>{})},null);
  Func<string,object> call=name=>type.GetMethod(name,BindingFlags.Instance|BindingFlags.Public|BindingFlags.NonPublic).Invoke(lease,null);
  call("Apply");bool observedBefore=(bool)call("Observe");if(externalUnmute)muted=false;bool observedAfter=(bool)call("Observe");call("Restore");call("Restore");call("Dispose");
  return new{original,externalUnmute,observedBefore,observedAfter,changes,finalMuted=muted};
 }
 static void Main() {
  var pads=new List<object>();
  pads.Add(Pad(new[]{true,true,true,true,true,true,true},new[]{true,false,false,true,false,false,false},new long[]{1000,1100,1500,1600,1700,1899,1900}));
  pads.Add(Pad(new[]{true,true,true,true,true},new[]{false,true,false,true,false},new long[]{0,100,1000,1400,1600}));
  pads.Add(Pad(new[]{true,true,false,true,true},new[]{false,true,false,false,false},new long[]{0,1000,1100,2000,2500}));
  pads.Add(Pad(new[]{true,true,true,true,true},new[]{false,true,false,false,false},new long[]{0,700,701,900,901}));
  pads.Add(Pad(new[]{true,true,true,true,true,true},new[]{false,true,false,true,false,false},new long[]{0,800,900,1000,1100,1300}));
  var mute=new List<object>();foreach(bool original in new[]{false,true})foreach(bool external in new[]{false,true})mute.Add(Mute(original,external));
  Console.WriteLine(new JavaScriptSerializer().Serialize(new{schema="nba2k27-startup-video-policy-oracle-v1",gameAccess=false,audioSettingsChanged=false,originalSourceUnmodified=true,pads,mute}));
 }
}
