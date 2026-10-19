using System;
using System.Linq;
using System.Text;
internal static class BallAssetExperimentTests {
 static void Check(bool value,string why){if(!value)throw new Exception(why);}
 static void Reject(Action action){try{action();}catch(Exception){return;}throw new Exception("Unsafe replacement accepted");}
 public static int Main(){
  var empty=new byte[48];Check(BallAssetPolicy.Canonical(0)==BallAssetPolicy.Normal&&BallAssetPolicy.Canonical(BallAssetPolicy.Default)==BallAssetPolicy.Normal,"default resolution");
  Check(BallAssetPolicy.Canonical(0xB58EF870)==0xB58EF870,"custom identity preserved");
  foreach(int refs in new[]{-1,1,2,int.MaxValue})Check(!BallAssetPolicy.MayWrite(true,refs,empty),"referenced asset must never be rewritten");
  Check(!BallAssetPolicy.MayWrite(false,0,empty),"no match/transition writes");Check(BallAssetPolicy.MayWrite(true,0,empty),"cold menu write");
  Check(!BallAssetPolicy.ObservedLoad(0,empty),"temporary game-mode changes without any resource use are not a completed match");
  Check(BallAssetPolicy.ObservedLoad(1,empty),"pending native load must be observed before renderer is ready");
  for(int i=0;i<48;i++){var slot=new byte[48];slot[i]=1;Check(!BallAssetPolicy.Unloaded(0,slot),"every resource pointer must be cleared");}
  Check(!BallAssetPolicy.Unloaded(0,null)&&!BallAssetPolicy.Unloaded(0,new byte[47]),"invalid slot fails closed");
  Check(BallAssetPolicy.Selection(0x92DF06DE,0x92DF06DE,3,0,0),"selection");
  Check(!BallAssetPolicy.Selection(0x92DF06DE,0x92DF06DE,2,0,0),"loading not selection");
  Check(!BallAssetPolicy.QuietMenu(0x92DF06DE,0x6509837B,3,0,0),"mode transition");
  Check(BallAssetPolicy.QuietMenu(0x6509837B,0x6509837B,1,0,0),"main menu stale stage");
  Check(BallAssetPolicy.QuietMenu(0x079964C0,0x079964C0,1,0,0),"verified Play Now menu after Blacktop exit");
  Check(!BallAssetPolicy.QuietMenu(0x079964C0,0x7C602DF8,1,0,0)&&!BallAssetPolicy.QuietMenu(0x079964C0,0x079964C0,1,1,1),"Play Now identity alone is insufficient");
  Check(!BallAssetPolicy.MainMenu(0x12345678),"unknown menus fail closed");
  Check(!BallAssetPolicy.QuietMenu(0x6509837B,0x6509837B,0,1,0)&&!BallAssetPolicy.QuietMenu(0x6509837B,0x6509837B,0,0,1),"active game not safe");
  foreach(uint hash in new uint[]{0x7C602DF8,0xB4D37B22,0x3EEF98B9})Check(!BallAssetPolicy.QuietMenu(hash,hash,1,0,0),"pause/play never permits writes");
  var original=new byte[BallSelectorPolicy.RecordStride];for(int i=0;i<0x9c;i++)original[i]=(byte)(i+3);
  Array.Copy(BitConverter.GetBytes(BallAssetPolicy.Normal),0,original,4,4);var name=Encoding.ASCII.GetBytes(BallAssetPolicy.NormalFile);Array.Copy(name,0,original,0x9c,name.Length);
  foreach(string selected in new[]{"ball_gameplay_official_animal_cheetah.iff","ball_x.iff","ball_"+new string('x',118)+".iff"}){
   var next=BallAssetPolicy.Replacement(original,selected);Check(next.Take(0x9c).SequenceEqual(original.Take(0x9c)),"identity/metadata untouched");
   int length=BallAssetPolicy.FieldLength(original,next);Check(next.Skip(0x9c+length).SequenceEqual(original.Skip(0x9c+length)),"tail untouched");
   Check(Encoding.ASCII.GetString(next,0x9c,selected.Length)==selected&&next[0x9c+selected.Length]==0,"replacement string");
   Check(Encoding.ASCII.GetString(original,0x9c,name.Length)==BallAssetPolicy.NormalFile,"original retained");
  }
  foreach(string invalid in new[]{"../ball_x.iff","balls/ball_x.iff","ball_..iff","ball_é.iff","ball_x.iff\0bad","ball_"+new string('x',119)+".iff"})Reject(()=>BallAssetPolicy.Replacement(original,invalid));
  var conflict=(byte[])original.Clone();conflict[0x9c+name.Length+1]=7;Reject(()=>BallAssetPolicy.Replacement(conflict,"ball_"+new string('x',80)+".iff"));
  var wrongId=(byte[])original.Clone();wrongId[4]^=1;Reject(()=>BallAssetPolicy.Replacement(wrongId,"ball_x.iff"));
  var wrongFile=(byte[])original.Clone();wrongFile[0x9c]=(byte)'X';Reject(()=>BallAssetPolicy.Replacement(wrongFile,"ball_x.iff"));
  Console.WriteLine("Ball asset substitution policy tests passed; no game process accessed.");return 0;
 }
}
