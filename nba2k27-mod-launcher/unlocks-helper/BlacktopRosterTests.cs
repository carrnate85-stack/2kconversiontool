using System;
internal static class BlacktopRosterTests {
 static void Check(bool value,string message){if(!value)throw new Exception(message);}
 public static void Main(){
  const uint home=0x6509837B,quick=0x079964C0,selection=0x92DF06DE;
  Check(BlacktopRosterPolicy.Menu(true,home,home,0,0,0),"Initial Home must arm");
  Check(BlacktopRosterPolicy.Menu(true,home,home,1,0,0),"Post-match Home must rearm");
  Check(BlacktopRosterPolicy.Menu(true,quick,quick,1,0,0),"Post-match Quick Play must rearm");
  Check(!BlacktopRosterPolicy.Menu(true,quick,quick,0,0,0),"Unverified Quick Play stage");
  foreach(uint menu in new[]{home,quick}){
   foreach(uint stage in new uint[]{2,3,4,99})Check(!BlacktopRosterPolicy.Menu(true,menu,menu,stage,0,0),"Unexpected stage armed");
   Check(!BlacktopRosterPolicy.Menu(true,menu,menu,1,1,0),"Active match armed");
   Check(!BlacktopRosterPolicy.Menu(true,menu,menu,1,0,1),"Live game owner armed");
   Check(!BlacktopRosterPolicy.Menu(true,menu,selection,1,0,0),"Mode transition armed");
  }
  foreach(uint other in new uint[]{0,selection,0xB4D37B22,0x7C602DF8,0xDEADBEEF})
   Check(!BlacktopRosterPolicy.Menu(true,other,other,1,0,0),"Foreign mode armed");
  Check(BlacktopRosterPolicy.Menu(false,home,home,0,0,0),"Legacy Home changed");
  Check(!BlacktopRosterPolicy.Menu(false,home,home,1,0,0)&&!BlacktopRosterPolicy.Menu(false,quick,quick,1,0,0),"Unverified legacy exit allowed");
  // Exercise the actual policy used by the worker for several Blacktop visits.
  bool armed=false;int arms=0,releases=0;
  foreach(uint menu in new[]{home,quick,quick,home}){
   uint stage=arms==0?0U:1U;
   if(BlacktopRosterPolicy.Menu(true,menu,menu,stage,0,0)&&!armed){armed=true;arms++;}
   Check(armed,"Return route failed to rearm");
   foreach(uint loading in new uint[]{1,2})
    Check(!BlacktopRosterPolicy.Release(armed,BlacktopPresentationPolicy.RosterEntryComplete(selection,selection,loading),false),"Loader restored before entry completed");
   if(BlacktopRosterPolicy.Release(armed,BlacktopPresentationPolicy.RosterEntryComplete(selection,selection,3),false)){armed=false;releases++;}
   Check(!armed,"Completed entry did not restore callback");
   Check(!BlacktopRosterPolicy.Menu(true,0xB4D37B22,0xB4D37B22,1,1,1),"Gameplay rearmed");
  }
  Check(arms==4&&releases==4,"Repeated visits did not balance ownership");
  Check(BlacktopRosterPolicy.Release(true,false,true),"Existing deadline safeguard lost");
  Check(!BlacktopRosterPolicy.Release(false,true,true),"Unowned callback restored");
  Console.WriteLine("PASS: WNBA Home/Quick Play repeated entry, inactive guards, loading preservation, legacy boundary and timeout");
 }
}
