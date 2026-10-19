using System;
using System.Linq;

internal static class BallSelectorTests {
 static void Check(bool value,string message){if(!value)throw new Exception(message);}
 static bool Has(byte[] source,byte[] value){for(int i=0;i<=source.Length-value.Length;i++)if(source.Skip(i).Take(value.Length).SequenceEqual(value))return true;return false;}
 public static int Main(){
  foreach(string action in new[]{"ball-bind","ball-select","ball-current","ball-observe"})Check(BallSelectorPolicy.ActivationDisabled(action),"activation must remain quarantined");
  foreach(string action in new[]{"ball-restore","ball-recover"})Check(!BallSelectorPolicy.ActivationDisabled(action),"legacy recovery must remain available");
  Check(BallSelectorPolicy.Original.SequenceEqual(new byte[]{0x41,0x8B,0x8E,0x4C,0x03,0x00,0x00}),"exact AI_BALL identity instruction changed");
  var stub=BallSelectorPolicy.Stub(0x1122334455667788UL,0x140000000UL);Check(stub.Length>100&&stub.Length<512,"stub length is not bounded");Check(stub.Take(7).SequenceEqual(BallSelectorPolicy.Original),"stub must replay original instruction");
  Check(stub[7]==0x50&&stub[8]==0x52&&stub[stub.Length-3]==0x5A&&stub[stub.Length-2]==0x58&&stub[stub.Length-1]==0xC3,"stub must preserve RDX/RAX and return");
  foreach(ulong address in new[]{0x140000000UL+BallSelectorPolicy.ModeCurrentRva,0x140000000UL+BallSelectorPolicy.ModeAlternateRva,0x140000000UL+BallSelectorPolicy.ModeTableRva,0x140000000UL+BallSelectorPolicy.BlacktopStageRva,0x1122334455667788UL})Check(Has(stub,BitConverter.GetBytes(address)),"mode-scoped stub is missing an exact address");
  foreach(uint value in new uint[]{0x92DF06DE,0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898})Check(Has(stub,BitConverter.GetBytes(value)),"mode-scoped stub is missing a verified Blacktop identity");
  Check(BallSelectorPolicy.Scope(0x92DF06DE,0x92DF06DE,3,0)=="selection","selection scope changed");
  foreach(uint value in new uint[]{0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898}){
   Check(BallSelectorPolicy.Scope(value,value,1,6)=="match","Blacktop match scope changed");
   foreach(uint mode in new uint[]{0,1,2,3,4,5,7,8})Check(BallSelectorPolicy.Scope(value,value,1,mode)=="outside","stale Blacktop stage must not own another game mode");
  }
  Check(Has(stub,BitConverter.GetBytes(0x140000000UL+BallSelectorPolicy.SettingsRva)),"native render stub must check the actual game mode");
  Check(!stub.SequenceEqual(BallSelectorPolicy.Stub(0x1122334455667788UL,0x140000000UL,false)),"legacy recovery stub must differ from guarded stub");
  Check(BallSelectorPolicy.Scope(0x6509837B,0x6509837B,1,0)=="outside","stale stage in main menu must not prevent cleanup");
  Check(BallSelectorPolicy.Scope(0xB4D37B22,0xB4D37B22,0,6)=="outside","stale match identity without match stage must be outside");
  Check(BallSelectorPolicy.Scope(0x92DF06DE,0xB4D37B22,3,6)=="transition","mismatched mode slots cannot prove match exit");
  Check(BallSelectorPolicy.Scope(0x92DF06DE,0x92DF06DE,2,6)=="transition","loading cannot prove match exit");
  Check(BallSelectorPolicy.Scope(0x92DF06DE,0x92DF06DE,2,0)=="transition","loading before game mode initialization cannot prove match exit");
  Check(BallSelectorPolicy.Scope(0x1234,0x1234,1,6)=="transition","unknown Blacktop submode cannot prove match exit");
  Check(BallSelectorPolicy.Scope(0xB4D37B22,0xB4D37B22,1,uint.MaxValue)=="transition","unreadable settings cannot prove match exit");
  BallSelectorNativeTests.Run();
  var patch=BallSelectorPolicy.Patch(0x140000000UL+BallSelectorPolicy.HookRva,0x170000000UL);Check(patch.Length==7&&patch[0]==0xE8&&patch[5]==0x90&&patch[6]==0x90,"hook patch must be one rel32 call plus two NOPs");
  Console.WriteLine("Ball selector policy tests passed.");return 0;
 }
}
