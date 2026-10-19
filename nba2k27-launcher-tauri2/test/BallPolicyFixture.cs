using System;
using System.Linq;
using System.Text;
using System.Collections.Generic;
using System.Web.Script.Serialization;
// Calls only the original pure BallAssetPolicy. No process discovery, native
// APIs, WPF helper entry point or game access is compiled into this oracle.
internal static class BallPolicyFixture {
 static string Hex(byte[] b){return BitConverter.ToString(b).Replace("-","");}
 static byte[] Original(){var b=new byte[BallSelectorPolicy.RecordStride];for(int i=0;i<BallSelectorPolicy.RecordAssetOffset;i++)b[i]=(byte)(i+3);Array.Copy(BitConverter.GetBytes(BallAssetPolicy.Normal),0,b,4,4);var n=Encoding.ASCII.GetBytes(BallAssetPolicy.NormalFile);Array.Copy(n,0,b,BallSelectorPolicy.RecordAssetOffset,n.Length);b[b.Length-4]=98;return b;}
 static int Main(){
  var restore=new List<object>();
  for(int mask=0;mask<32;mask++)foreach(bool quiet in new[]{false,true})foreach(int refs in new[]{-1,0,1,2})foreach(bool loaded in new[]{false,true}){var r=new byte[48];if(loaded)r[16]=1;restore.Add(new{mask,quiet,refs,loaded,expected=BallAssetPolicy.RestoreNow((mask&1)!=0,(mask&2)!=0,(mask&4)!=0,(mask&8)!=0,(mask&16)!=0,quiet,refs,r)});}
  var menus=new List<object>();var modes=new uint[]{0x92DF06DE,0x6509837B,0x079964C0,0xB4D37B22,0x7C602DF8,0};
  foreach(uint a in modes)foreach(uint b in modes)for(uint stage=0;stage<5;stage++)for(byte active=0;active<2;active++)for(byte kind=0;kind<2;kind++)menus.Add(new{a,b,stage,active,kind,quiet=BallAssetPolicy.QuietMenu(a,b,stage,active,kind)});
  var replacements=new List<object>();
  Action<byte[],string> sample=(original,filename)=>{try{var next=BallAssetPolicy.Replacement(original,filename);replacements.Add(new{originalHex=Hex(original),filename,accepted=true,replacementHex=Hex(next),fieldLength=BallAssetPolicy.FieldLength(original,next)});}catch{replacements.Add(new{originalHex=Hex(original),filename,accepted=false,replacementHex=(string)null,fieldLength=0});}};
  foreach(string name in new[]{"ball_x.iff","ball_gameplay_official_animal_cheetah.iff","ball_"+new string('x',118)+".iff","ball_"+new string('x',119)+".iff","ball_.iff","../ball_x.iff","balls/ball_x.iff","ball_..iff","ball_é.iff","ball_x.iff\0bad","","BALL_x.iff","ball_X.iff","ball_a-b.iff","ball_a_b.iff"})sample(Original(),name);
  var corrupt=Original();corrupt[4]^=1;sample(corrupt,"ball_x.iff");corrupt=Original();corrupt[0x9C]=(byte)'X';sample(corrupt,"ball_x.iff");corrupt=Original();corrupt[0x9C+BallAssetPolicy.NormalFile.Length+1]=7;sample(corrupt,"ball_"+new string('x',80)+".iff");sample(new byte[0],"ball_x.iff");sample(new byte[415],"ball_x.iff");
  Console.WriteLine(new JavaScriptSerializer{MaxJsonLength=4000000}.Serialize(new{schema="nba2k27-ball-policy-oracle-v1",gameAccess=false,restore,menus,replacements}));return 0;
 }
}
