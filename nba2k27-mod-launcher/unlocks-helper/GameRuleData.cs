using System;
using System.Diagnostics;
using System.IO;
using System.Linq;

// Direct, exact-build GameData fields for the F8 overlay. This does not use
// the retired Game Rules menu hook, patch code, or change saved settings.
internal static partial class FreestyleTeams {
 sealed class RuleField {
  internal string label,field;internal int offset,width;
  internal RuleField(string l,string f,int o,int w){label=l;field=f;offset=o;width=w;}
 }
 static readonly RuleField[] RuleFields={
  new RuleField("Shot Clock","ShotClock",0x816,1),
  new RuleField("Out Of Bounds","OutOfBounds",0x824,1),
  new RuleField("Backcourt Violation","BackcourtViolation",0x825,1),
  new RuleField("8 Sec Half Court","EightSecondsHalfCourt",0x826,1),
  new RuleField("5 Sec Inbound","FiveSecondsInbounding",0xA75,1),
  new RuleField("3 in the Key","OffensiveViolationInKey",0x908,4),
  new RuleField("Def 3 in the Key","DefensiveViolationInKey",0x90C,4),
  new RuleField("5 Sec Back To Basket","FiveSecondsBackToBasket",0x827,1),
  new RuleField("5 Sec Closely Guarded","CloselyGuarded",0xA4E,1),
  new RuleField("Over the Back Foul","OverTheBackRule",0x830,1),
  new RuleField("Generic Personal Foul","GenericPersonalFoulRule",0x831,1),
  new RuleField("Charging Foul","ChargingRule",0x832,1),
  new RuleField("Offensive Offball Foul","OffensiveOffballFoulRule",0x833,1),
  new RuleField("Defensive Offball Foul","DefensiveOffballFoulRule",0x834,1),
  new RuleField("Blocking Foul","BlockingRule",0x835,1),
  new RuleField("Reaching Foul","ReachingFoulRule",0x836,1),
  new RuleField("Loose Ball Foul","LooseBallRule",0x838,1),
  new RuleField("Shooting Foul","ShootingFoulRule",0x837,1),
  new RuleField("Intentional Foul","IntentionalFoul",0x839,1),
  new RuleField("Clear Path Foul","ClearPathRule",0x840,1),
  new RuleField("Transition Take Foul","TransitionTakeFoulRule",0x841,1),
  new RuleField("Kick Ball","KickBall",0x83A,1),
  new RuleField("Goaltending","Goaltending",0x828,1),
  new RuleField("Illegal Pick","IllegalPick",0x82B,1),
  new RuleField("Traveling","Traveling",0x829,1),
  new RuleField("Double Dribble","DoubleDribble",0x82A,1),
  new RuleField("Foul Out","FoulOut",0x82C,4),
  new RuleField("3Pt Shots","ThreePointShotRestriction",0xA98,4),
  new RuleField("Zone Defense Allowed","ZoneDefenseAllowed",0x8D4,1),
  new RuleField("No Charge Zone","NoChargeZone",0xA50,1),
  new RuleField("Buzzer Beater Review","BuzzerBeaterReview",0x849,1),
  new RuleField("Clear Ball Required","ClearBallRequired",0xA51,1),
  new RuleField("Timeouts","Timeouts",0xA5D,1),
  new RuleField("Screen Set Out of Bounds","ScreenOutOfBoundsViolation",0x844,1),
  new RuleField("Offensive Player Out of Bounds","PlayerOutOfBoundsViolationRule",0x846,1),
  new RuleField("Ball Entered Basket From Below","BallEnteredBasketFromBelow",0xA74,1),
  new RuleField("Technical Foul","TechnicalFoulRule",0x83B,1),
  new RuleField("Flagrant Foul","FlagrantFoulRule",0x83C,1),
  new RuleField("Technical Foul Ejection","TechnicalFoulEjectionRule",0x83D,1)
 };
 internal static bool RuleValueAllowed(string field,int width,int value){
  if(width==1)return value==0||value==1;
  if(width!=4||value<0)return false;
  if(field=="FoulOut")return value<=6;
  return value<=2&&(field=="OffensiveViolationInKey"||field=="DefensiveViolationInKey"||field=="ThreePointShotRestriction");
 }
 static long RuleValue(ulong owner,RuleField rule){
  var raw=Read(owner+(ulong)rule.offset,rule.width);
  return rule.width==1?raw[0]:(long)D(raw);
 }
 static byte[] RuleBytes(RuleField rule,int value){return rule.width==1?new[]{(byte)value}:BitConverter.GetBytes(value);}
 static ulong RuleOwner(){
  var code=Read(basis+0x2610815,7);
  if(code[0]!=0x48||code[1]!=0x8B||code[2]!=0x05||0x2610815L+7+BitConverter.ToInt32(code,3)!=0xAACB9A0L)
   throw new Stop("unsupported","Game Rules owner anchor differs from this build.");
  ulong owner=Q(Read(basis+0xAACB9A0,8));
  if(owner<0x10000||owner>0x7fffffffffffUL||owner%8!=0)throw new Stop("waiting","Game Rules are not initialized yet.");
  Area(owner,0xA9C,0x20000,true);
  return owner;
 }
 static object RunRuleData(string[] args){
  if(args[0]=="rules-self-test"){
   if(args.Length!=1||RuleFields.Length!=39||RuleFields.Count(r=>r.width==1)!=35||RuleFields.Count(r=>r.width==4)!=4||RuleFields.Select(r=>r.field).Distinct().Count()!=39)
    throw new Stop("error","Invalid Game Rules field catalog.");
   return new{phase="verified",count=39,editable=39,writes=0};
  }
  bool write=args[0]=="rules-write";
  if((write&&args.Length!=10)||(!write&&args.Length!=2))throw new Stop("error","Invalid Game Rules request.");
  string expected=Path.GetFullPath(Path.Combine(args[1],"NBA2K27.exe"));
  using(var gate=new System.Threading.Mutex(false,"Local\\2kLauncherDirectRules")){
   bool locked=false;
   try{
    try{locked=gate.WaitOne(0);}catch(System.Threading.AbandonedMutexException){locked=true;}
    if(!locked)throw new Stop("waiting","Another Game Rules update is running.");
    var games=Process.GetProcessesByName("NBA2K27");
    try{
     if(games.Length!=1)throw new Stop("waiting","Start one offline NBA 2K27 session first.");
     var game=games[0];
     if(!String.Equals(Path.GetFullPath(game.MainModule.FileName),expected,StringComparison.OrdinalIgnoreCase))
      throw new Stop("error","Running game does not match the configured game folder.");
     GameRulesEac();
     created=game.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)game.MainModule.BaseAddress.ToInt64();
     if(write&&(args[2]!=game.Id.ToString()||args[3]!=created.ToString()||args[4]!=basis.ToString()))
      throw new Stop("ended","Game session changed. Refresh rules before editing.");
     handle=OpenProcess(write?0x438U:0x410U,false,game.Id);
     if(handle==IntPtr.Zero)throw new Stop("error","Cannot access game rules. Run game and launcher at the same permission level.");
     Identity();
     var pe=Read(basis,4096);int p=(int)D(pe,60);
     if(pe[0]!=77||pe[1]!=90||p<64||p>3800||D(pe,p)!=0x4550||D(pe,p+8)!=0x6AA2013D||D(pe,p+80)!=0x36558000)
      throw new Stop("unsupported","Game Rules support a different NBA 2K27 build.");
     ulong owner=RuleOwner();
     if(write){
      if(args[5]!=owner.ToString())throw new Stop("ended","Game Rules owner changed. Refresh before editing.");
      var originalValues=args[9].Split(',');
      if(originalValues.Length!=RuleFields.Length)throw new Stop("error","Invalid Game Rules snapshot.");
      for(int i=0;i<RuleFields.Length;i++){
       long expectedValue;
       if(!long.TryParse(originalValues[i],out expectedValue)||expectedValue<0||expectedValue>uint.MaxValue)
        throw new Stop("error","Invalid Game Rules snapshot value.");
       var mapped=RuleFields[i];
       long current=RuleValue(owner,mapped);
       if(current!=expectedValue)throw new Stop("conflict","Game Rules changed or the game switched modes. Refresh before editing.");
      }
      var rule=RuleFields.FirstOrDefault(r=>r.field==args[6]);
      int before,after;
      if(rule==null||!int.TryParse(args[7],out before)||!int.TryParse(args[8],out after)||!RuleValueAllowed(rule.field,rule.width,before)||!RuleValueAllowed(rule.field,rule.width,after)||before==after)
       throw new Stop("error","Invalid Game Rules change.");
      ulong address=owner+(ulong)rule.offset;
      if(RuleValue(owner,rule)!=before)throw new Stop("conflict","Rule changed in game. Refresh before editing.");
      if(RuleOwner()!=owner)throw new Stop("ended","Game Rules owner changed before write.");
      bool attempted=false;
      try{
       GameRulesEac();Identity();attempted=true;UIntPtr done;
       if(!WriteProcessMemory(handle,(IntPtr)(long)address,RuleBytes(rule,after),(UIntPtr)rule.width,out done)||done.ToUInt64()!=(ulong)rule.width||RuleValue(owner,rule)!=after)
        throw new Stop("write-failed","Rule update did not read back.");
      }catch{
       if(attempted&&RuleOwner()==owner&&RuleValue(owner,rule)==after){UIntPtr done;WriteProcessMemory(handle,(IntPtr)(long)address,RuleBytes(rule,before),(UIntPtr)rule.width,out done);}
       throw;
      }
     }
     if(RuleOwner()!=owner)throw new Stop("ended","Game Rules owner changed during read.");
     var values=RuleFields.Select(r=>new{label=r.label,field=r.field,width=r.width,value=RuleValue(owner,r)}).ToArray();
     if(RuleOwner()!=owner)throw new Stop("ended","Game Rules owner changed during read.");
     Identity();
     return new{phase="ready",pid=game.Id,created=created.ToString(),basis=basis.ToString(),owner=owner.ToString(),values=values,writes=write?1:0};
    }finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}foreach(var game in games)game.Dispose();}
   }finally{if(locked)gate.ReleaseMutex();}
  }
 }
}
