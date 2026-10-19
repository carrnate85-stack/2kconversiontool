/* Presentation groups only. Packed roster offsets remain in the generated schema. */
(function(root,factory){if(typeof module==='object'&&module.exports)module.exports=factory();else root.playerEditorLayout=factory();})(typeof window==='undefined'?globalThis:window,()=>{
 const groups={
  attributes:{
   Finishing:'DrivingLayup PostFadeaway PostHook PostControl DrawFoul ShotClose StandingDunk DrivingDunk Hands',
   Shooting:'ShotMidrange ShotThree ShotFreeThrow ShotIQ OffensiveConsistency',
   Playmaking:'BallControl PassIQ PassAccuracy PassVision',
   Defense:'InteriorDefense PerimeterDefense PassPerception Block Steal DefensiveConsistency HelpDefenseIQ ReactionTime',
   Rebounding:'ReboundOffense ReboundDefense',
   Athleticism:'Speed SpeedWithBall Vertical Strength Stamina Hustle Agility Intangibles Potential'
  },
  badges:{
   Finishing:'FloatGame Posterizer RiseUp AerialWizard HookSpecialist LayupMixmaster PaintProdigy PhysicalFinisher PostPowerhouse GhostStepper PostSpinCatalyst',
   Shooting:'PostFadePhenom Deadeye LimitlessRange SlipperyOffball MiniMarksman SetAndFire ArcCadence StaticMiddy SmoothOperator QuickTrigger',
   Playmaking:'BailOut BreakStarter Dimer HandlesForDays Unpluckable VersatileVisionary AnkleAssassin LightningLaunch StrongHandle',
   Defense:'PostLockdown Challenger OffBallPest PickDodger Glove Interceptor ImmovableEnforcer HighFlyingDenier PaintPatroller WallUp Seatbelt AnkleBraces',
   Rebounding:'Crasher PossessionCloser SyncSnatcher BoxoutBoss',
   Athleticism:'Pogostick BrickWall Pace Breaker Flash WorkHorse Bruiser'
  }
 };
 const categories={attributes:[...Object.keys(groups.attributes),'Durability'],tendencies:['Shooting','Driving','Passing','Post','Freelance','Defense / Fouls'],badges:Object.keys(groups.badges),stats:['Season highs','Career highs'],signature:['Jump shots','Finishing','Post','Ball handling','Motion & rituals'],equipment:['Gear','Wear frequency','Home colors','Away colors']};
 function category(f){
  const key=f.key.replace(/(?:AbilityRaw|TendencyRaw|Raw|Badge)$/,'');
  if(groups[f.group]){for(const [name,keys] of Object.entries(groups[f.group]))if(keys.split(' ').includes(key))return name;return f.group==='attributes'&&key.startsWith('Durability')?'Durability':'Other';}
  if(f.group==='tendencies'){
   if(/^(Interception|TakeCharge|OnBallSteal|ContestShot|BlockShot|Foul|HardFoul)$/.test(key))return 'Defense / Fouls';
   if(/^(FindOpenMan|FlashyPass|AlleyOopPass)$/.test(key))return 'Passing';
   if(key.startsWith('Post'))return 'Post';
   if(/^(Touch|RollVsFade|TransitionSpotup|IsoVs|PlayDiscipline|Crash)/.test(key))return 'Freelance';
   if(/^(Driving|StandingDunk|FlashyDunk|AlleyOop|Putback|SpinLayup|HopstepLayup|EurostepLayup|Floater|DriveRight|Drive$|SpotupDrive|OffscreenDrive|TripleThreat|Setup|Dribble|FinishAttack)/.test(key))return 'Driving';
   return 'Shooting';
  }
  if(f.group==='stats')return key.startsWith('SeasonHigh')?'Season highs':'Career highs';
  if(f.group==='signature'){
   if(/^(Iso|Dribble)/.test(key))return 'Ball handling';
   if(key.startsWith('Post'))return 'Post';
   if(/^(Layup|DunkPackage|ShotStepThru)/.test(key))return 'Finishing';
   if(/^(Motion|Sig|Gum|NoMad|Emote)/.test(key))return 'Motion & rituals';
   return 'Jump shots';
  }
  if(f.group==='equipment'){
   if(f.key.startsWith('Shoe'))return 'Gear';
   if(f.key.endsWith('Frequency'))return 'Wear frequency';
   if(f.key.endsWith('HomeColor'))return 'Home colors';
   if(f.key.endsWith('AwayColor'))return 'Away colors';
   return 'Gear';
  }
  return '';
 }
 const personalityPairs=[
  {label:'Friendliness',keys:['LowFriendlinessBadge','HighFriendlinessBadge'],choices:['Low','High']},
  {label:'Ego',keys:['SmallEgoBadge','BigEgoBadge'],choices:['Small','Big']},
  {label:'Motivated',keys:['NegativelyMotivatedBadge','PositivelyMotivatedBadge'],choices:['No (negatively)','Yes (positively)']}
 ];
 const signatureOrder=['ShotJumper','ShotRelease','ShotRelease2','ShotBlendingRatio','ShotTiming','FreeThrow','GoToShot','ShotDribblePullup','ShotSpin','ShotSideHop','ShotStepThru','LayupPackage',...Array.from({length:15},(_,i)=>'DunkPackage'+(i+1)),'PostFade','PostHook','PostOneStepPullup','PostGoToShot','DribbleStyle','IsoSizeupSig','IsoBreakdownMove','IsoBreakdown','IsoCrossover','IsoCrossCombo','IsoBehindBack','IsoTweenlegs','IsoSpin','IsoCrossSpin','IsoHesitation','IsoHesitationLateral','IsoInandOut','IsoStepback','IsoStepbackCross','IsoStepbackLateral','IsoBehindBackLaunch','IsoEscapeCross','IsoEscapeTween','IsoEscapeBehindback','IsoEscapeHes','IsoComboHesicross','IsoComboDoublecross','IsoMisdirectCross','IsoMisdirectBehindback','IsoMisdirectHes','MotionStyle','SigPlayerIntro1','SigPlayerIntro2','SigJumpballStand','NoMadDunk','GumChewer','EmoteDRB','EmoteTT'];
 const statOrder=['TotalPoints','MinutesPlayed','FieldGoalsMade','FieldGoalsAttempted','ThreePointShotsMade','ThreePointShotsAttempted','FreeThrowsMade','FreeThrowsAttempted','Rebounds','OffensiveRebounds','DefensiveRebounds','Assists','Steals','Blocks'];
 const order=f=>f.group==='signature'?signatureOrder.indexOf(f.key):f.group==='stats'?statOrder.indexOf(f.key.replace(/^(Season|Career)High/,'')):0;
 return {categories,category,personalityPairs,order};
});
