using System;
using System.Linq;
internal static class BlacktopPresentationTests {
 static void Check(bool value){if(!value)throw new Exception("Presentation policy assertion failed");}
 public static void Main(){
  for(int m=0;m<8;m++){
   var selected=BlacktopPresentationPolicy.Selected(m);
   Check(selected.Contains(0)==((m&1)!=0));Check(selected.Contains(1)==((m&2)!=0));
   Check(selected.Contains(2)==((m&4)!=0));Check(selected.Contains(5)==((m&4)!=0));Check(!selected.Contains(3)&&!selected.Contains(4));
  }
  foreach(int bad in new[]{-1,8,255}){bool rejected=false;try{BlacktopPresentationPolicy.Selected(bad);}catch(ArgumentException){rejected=true;}Check(rejected);}
  Check(BlacktopPresentationPolicy.Selection(0x92DF06DE,0x92DF06DE,3,0,0));
  Check(!BlacktopPresentationPolicy.Selection(0x6509837B,0x6509837B,0,0,0));
  Check(!BlacktopPresentationPolicy.Selection(0x92DF06DE,0xB4D37B22,3,0,0));
  Check(!BlacktopPresentationPolicy.Selection(0x92DF06DE,0x92DF06DE,1,0,0));
  Check(!BlacktopPresentationPolicy.Selection(0x92DF06DE,0x92DF06DE,3,1,0));
  Check(!BlacktopPresentationPolicy.Selection(0x92DF06DE,0x92DF06DE,3,0,1));
  // First application is allowed only at stable Blacktop player selection, never in a live match.
  foreach(uint hash in new uint[]{0,0xB4D37B22,0x7C602DF8,0x50BAD898,0xDEADBEEF})
   foreach(uint stage in new uint[]{0,1,3})
    Check(!BlacktopPresentationPolicy.CanApply(hash,hash,stage,1,1));
  Check(!BlacktopPresentationPolicy.CanApply(0,0,0,0,0));
  Check(!BlacktopPresentationPolicy.CanApply(0x6509837B,0x6509837B,0,0,0));
  Check(BlacktopPresentationPolicy.CanApply(0x92DF06DE,0x92DF06DE,3,0,0));
  Check(!BlacktopPresentationPolicy.CanApply(0x6509837B,0x7C602DF8,0,0,0));
  Check(!BlacktopPresentationPolicy.CanApply(0x6509837B,0x6509837B,0,1,1));
  Check(!BlacktopPresentationPolicy.CanApply(0x6509837B,0x6509837B,1,0,0));
  Check(!BlacktopPresentationPolicy.RosterEntryComplete(0x92DF06DE,0x92DF06DE,1));
  Check(!BlacktopPresentationPolicy.RosterEntryComplete(0x92DF06DE,0x92DF06DE,2));
  Check(BlacktopPresentationPolicy.RosterEntryComplete(0x92DF06DE,0x92DF06DE,3));
  Check(!BlacktopPresentationPolicy.RosterEntryComplete(0x92DF06DE,0,3));
  byte[] original={0,0,0,0,0,0},applied={1,1,1,0,0,1};
  Check(BlacktopPresentationPolicy.ResetToOriginal(10,10,original,original));
  Check(!BlacktopPresentationPolicy.ResetToOriginal(11,10,original,original));
  Check(!BlacktopPresentationPolicy.ResetToOriginal(10,10,applied,original));
  Check(BlacktopPresentationPolicy.CanReapply(true,10,10,original,original));
  Check(!BlacktopPresentationPolicy.CanReapply(false,10,10,original,original));
  Check(!BlacktopPresentationPolicy.CanReapply(true,11,10,original,original));
  Check(!BlacktopPresentationPolicy.CanReapply(true,10,10,applied,original));
  Check(FreestyleTeams.BcRosterSupported(new[]{300,301}));
  Check(FreestyleTeams.BcRosterSupported(Enumerable.Range(300,15).ToArray()));
  Check(!FreestyleTeams.BcRosterSupported(new[]{300}));
  Check(!FreestyleTeams.BcRosterSupported(new[]{300,300}));
  Check(!FreestyleTeams.BcRosterSupported(new[]{300,2048}));
  Console.WriteLine("PASS: eight masks, MC coupling, selection-only apply and guarded reapply; no restoration action");
 }
}
