using System;
internal static class ScalerPolicyTests {
 static int count;
 static void Check(bool ok){count++;if(!ok)throw new Exception("Scaler policy assertion "+count);}
 public static int Main(){
  Check(ScalerPolicy.Target(95)==0x3f6e5604);Check(ScalerPolicy.Target(100)==0x3f7ae148);
  for(int p=95;p<=100;p++){Check(ScalerPolicy.Target(p)==BitConverter.ToUInt32(BitConverter.GetBytes((float)((double)BitConverter.ToSingle(BitConverter.GetBytes(ScalerPolicy.Original),0)*p/100)),0));if(p<100)Check(ScalerPolicy.Target(p)<ScalerPolicy.Target(p+1));}
  foreach(int p in new[]{0,94,101,Int32.MaxValue}){bool rejected=false;try{ScalerPolicy.Target(p);}catch(ArgumentException){rejected=true;}Check(rejected);}
  var j=new ScalerJournal{pid=10,created=123,basis=0x140000000,address=0x14a38f19c,image="fixture.exe",phase="applied",applied=ScalerPolicy.Target(95),requested=ScalerPolicy.Target(95)};
  ScalerPolicy.Validate(j);Check(ScalerPolicy.Same(j,10,123,j.basis,"fixture.exe"));Check(!ScalerPolicy.Same(j,10,124,j.basis,"fixture.exe"));Check(!ScalerPolicy.Same(j,11,123,j.basis,"fixture.exe"));Check(!ScalerPolicy.Same(j,10,123,j.basis+4096,"fixture.exe"));Check(!ScalerPolicy.Same(j,10,123,j.basis,"other.exe"));
  Check(ScalerPolicy.Owned(j.applied,j.applied,j.requested));Check(ScalerPolicy.Owned(ScalerPolicy.Original,j.applied,j.requested));Check(!ScalerPolicy.Owned(0x3f800000,j.applied,j.requested));
  // Each explicit update derives from Original, never the previously scaled input.
  Check(ScalerPolicy.Target(95)==j.applied);j.requested=ScalerPolicy.Target(97);ScalerPolicy.Validate(j);Check(ScalerPolicy.Owned(j.applied,j.applied,j.requested));Check(ScalerPolicy.Owned(j.requested,j.applied,j.requested));
  j.phase="ended";Check(!ScalerPolicy.Same(j,10,123,j.basis,"fixture.exe"));
  j.original=0x3f800000;bool bad=false;try{ScalerPolicy.Validate(j);}catch(Exception){bad=true;}Check(bad);
  var updated=new ScalerJournal{pid=10,created=123,basis=0x140000000,address=0x14a39319c,image="fixture.exe",phase="applied",build="6AA2013D:36558000"};
  ScalerPolicy.Validate(updated);Check(ScalerPolicy.Same(updated,10,123,updated.basis,"fixture.exe",updated.build));
  updated.arenaCharacters=new[]{new ArenaCharacterOptIn{kind="generic",manager=0x10000,actor=0x20000,description=0x30000,originalScale=0x3f800000,originalInverse=0x3f800000,targetScale=0x3f6e5604,targetInverse=0x3f892f15}};ScalerPolicy.Validate(updated);
  updated.arenaCharacters[0].kind="prop";bad=false;try{ScalerPolicy.Validate(updated);}catch(Exception){bad=true;}Check(bad);updated.arenaCharacters=new ArenaCharacterOptIn[0];
  Check(!ScalerPolicy.Same(updated,10,123,updated.basis,"fixture.exe"));
  updated.address=updated.basis+0xA38F19C;bad=false;try{ScalerPolicy.Validate(updated);}catch(Exception){bad=true;}Check(bad);
  updated.address=updated.basis+0xA39319C;updated.build="6AA2013D:368C3000";bad=false;try{ScalerPolicy.Validate(updated);}catch(Exception){bad=true;}Check(bad);
  Console.WriteLine("PASS: "+count+" offline scaler policy checks; no process access.");return 0;
 }
}
