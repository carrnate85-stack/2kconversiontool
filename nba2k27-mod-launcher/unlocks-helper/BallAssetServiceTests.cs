using System;
using System.IO;
using System.Linq;
using System.Text;

internal static partial class FreestyleTeams {
 static void AssetCheck(bool value,string why){if(!value)throw new Exception(why);}
 static void AssetReject(Action action,string why){try{action();}catch{return;}throw new Exception("Unsafe action accepted: "+why);}
 internal static int TestAssetService(){
  // All lifecycle combinations: none permits writing a referenced or loaded
  // resource, even shutdown/recovery/current selection at a known menu.
  for(int mask=0;mask<32;mask++)foreach(bool quiet in new[]{false,true})foreach(int refs in new[]{-1,0,1,2})foreach(bool loaded in new[]{false,true}){
   var slot=new byte[48];if(loaded)slot[16]=1;
   bool result=BallAssetPolicy.RestoreNow((mask&1)!=0,(mask&2)!=0,(mask&4)!=0,(mask&8)!=0,(mask&16)!=0,quiet,refs,slot);
   AssetCheck(result==(mask!=0&&quiet&&refs==0&&!loaded),"lifecycle restore gate");
  }
  AssetCheck(!BallAssetPolicy.RestoreDue(false,false,false,false,false),"selection transition alone is not a match exit");
  foreach(string choice in new[]{"current","balls/"+BallAssetPolicy.NormalFile,"balls/ball_gameplay_official_default.iff"})AssetCheck(BallAssetPolicy.Current(choice),"normal/default needs no replacement");
  AssetCheck(!BallAssetPolicy.Current("balls/ball_gameplay_official_animal_cheetah.iff"),"custom ball");
  foreach(string bad in new[]{null,"", "balls/../ball_x.iff","balls/ball_x.iff\ncurrent","ball_x.iff","balls/ball_é.iff"})AssetCheck(!BallAssetPolicy.ValidChoice(bad),"invalid command");
  AssetCheck(AssetChoiceUnavailable(new Stop("unsupported","This stock ball is not available in the live game registry.")),"missing choice is retryable");
  AssetCheck(AssetChoiceUnavailable(new Stop("conflict","The selected ball does not have one unique live model.")),"duplicate choice is retryable");
  AssetCheck(!AssetChoiceUnavailable(new Stop("unsupported","Ball selection does not support this game update.")),"unsupported build is not retryable");
  AssetCheck(!AssetChoiceUnavailable(new Stop("conflict","Ownership changed.")),"ownership conflict is not retryable");
  var original=new byte[BallSelectorPolicy.RecordStride];Array.Copy(BitConverter.GetBytes(BallAssetPolicy.Normal),0,original,4,4);
  byte[] name=Encoding.ASCII.GetBytes(BallAssetPolicy.NormalFile);Array.Copy(name,0,original,BallSelectorPolicy.RecordAssetOffset,name.Length);original[original.Length-4]=98;
  var replacement=BallAssetPolicy.Replacement(original,"ball_gameplay_official_animal_cheetah.iff");AssetCheck(replacement[replacement.Length-4]==98,"real nonzero tail metadata preserved");
  created=123456789;basis=0x140000000;
  var journal=new BallAssetJournal{pid=123,birth=created,imageBase=basis,image=Path.GetFullPath("NBA2K27.exe"),holder=0x200000,records=0x300000,address=0x300000+BallSelectorPolicy.RecordStride,allocation=0x300000,index=1,sourceModel=0xB58EF870,logical="balls/ball_gameplay_official_animal_cheetah.iff",phase="prepared",original=original,replacement=replacement};
  var snapshot=new BallAssetSnapshot{holder=journal.holder,records=journal.records,address=journal.address,allocation=journal.allocation,index=journal.index,record=(byte[])replacement.Clone()};
  AssetOwned(snapshot,journal,replacement);
  snapshot.allocation++;AssetReject(()=>AssetOwned(snapshot,journal,replacement),"allocation changed");snapshot.allocation--;
  snapshot.record[0]++;AssetReject(()=>AssetOwned(snapshot,journal,replacement),"foreign metadata changed");snapshot.record[0]--;
  snapshot.record[BallSelectorPolicy.RecordAssetOffset]++;AssetReject(()=>AssetOwned(snapshot,journal,replacement),"partial/foreign filename");snapshot.record[BallSelectorPolicy.RecordAssetOffset]--;
  AssetCheck(AssetSame(journal,123,journal.image),"exact session");
  AssetCheck(!AssetSame(journal,124,journal.image)&&!AssetSame(journal,123,journal.image+"other"),"PID/path reuse");
  created++;AssetCheck(!AssetSame(journal,123,journal.image),"birth mismatch");created--;
  basis++;AssetCheck(!AssetSame(journal,123,journal.image),"base mismatch");basis--;
  string directory=Path.Combine(Path.GetTempPath(),"ball-asset-policy-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(directory);string file=Path.Combine(directory,"session.json");
  try{
   foreach(string phase in new[]{"prepared","armed","restored"}){
    journal.phase=phase;AssetSave(file,journal);var read=AssetLoad(file,123,journal.image);AssetCheck(read.phase==phase&&read.original.SequenceEqual(original)&&read.replacement.SequenceEqual(replacement),"durable bounded recovery roundtrip");
   }
   AssetReject(()=>AssetLoad(file,124,journal.image),"stale process recovery");
   journal.replacement[0]++;AssetSave(file,journal);AssetReject(()=>AssetReadJournal(file),"journal changes outside filename");journal.replacement[0]--;
   journal.phase="untrusted";AssetSave(file,journal);AssetReject(()=>AssetReadJournal(file),"unknown phase");
   File.WriteAllText(file,new string('x',16385));AssetReject(()=>AssetReadJournal(file),"oversized journal");
  }finally{if(File.Exists(file))File.Delete(file);Directory.Delete(directory);}
  Console.WriteLine("Ball asset service tests passed: lifecycle gates, ownership, exact session, recovery journals; no game process accessed.");return 0;
 }
}
internal static class BallAssetServiceTests {public static int Main(){return FreestyleTeams.TestAssetService();}}
