using System;
using System.Linq;
internal static class GameRulesTests {
 static void Require(bool value){if(!value)throw new Exception("Game Rules policy assertion failed.");}
 public static int Main(){
  ulong basis=0x7FF600000000,stub=basis+0x50000000,hook=basis+GameRulesPolicy.HookRva;
  var patch=GameRulesPolicy.Patch(hook,stub);Require(patch.Length==GameRulesPolicy.Original.Length&&patch[0]==0xE9&&patch.Skip(5).All(v=>v==0x90));
  var code=GameRulesPolicy.Stub(stub,basis);Require(code.Length==32&&code[0]==0x84&&code[1]==0xC0&&code[8]==0x42&&code[16]==0x3D);Require(BitConverter.ToUInt32(code,17)==GameRulesPolicy.ItemId);
  int j1=BitConverter.ToInt32(code,4);Require(stub+8+(ulong)j1==basis+GameRulesPolicy.IncludeRva);int j2=BitConverter.ToInt32(code,23);Require(stub+27+(ulong)j2==basis+GameRulesPolicy.IncludeRva);
  int tail=BitConverter.ToInt32(code,28);Require(stub+32+(ulong)tail==basis+GameRulesPolicy.ContinueRva);
  foreach(var field in new[]{"OffensiveViolationInKey","DefensiveViolationInKey","ThreePointShotRestriction"}){
   for(int value=0;value<=2;value++)Require(FreestyleTeams.RuleValueAllowed(field,4,value));
   Require(!FreestyleTeams.RuleValueAllowed(field,4,3));
  }
  for(int value=0;value<=6;value++)Require(FreestyleTeams.RuleValueAllowed("FoulOut",4,value));
  Require(!FreestyleTeams.RuleValueAllowed("FoulOut",4,7));
  Require(!FreestyleTeams.RuleValueAllowed("Unknown",4,1));
  Require(FreestyleTeams.RuleValueAllowed("ShotClock",1,0)&&FreestyleTeams.RuleValueAllowed("ShotClock",1,1));
  Require(!FreestyleTeams.RuleValueAllowed("ShotClock",1,2));
  Console.WriteLine("PASS: Game Rules exact hook, item-only trampoline and rel32 destinations");return 0;
 }
}
