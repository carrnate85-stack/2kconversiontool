using System;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Diagnostics;
using System.Collections.Generic;

internal static partial class FreestyleTeams {
 // One process owns one game lifetime. EOF means restore safely, never kill a
 // resource or rewrite a filename while the engine still holds references.
 static volatile bool assetServiceEnded;
 static volatile string assetServiceDesired="current";
 static string assetServiceLast="",assetServiceLog,assetServiceCommand="current";
 static bool AssetChoiceUnavailable(Stop error){
  // Only a missing/duplicate choice may be retried in the same game. Build,
  // ownership, and write failures still require a fresh, safe session.
  return error.phase=="unsupported"&&error.Message=="This stock ball is not available in the live game registry."
   ||error.phase=="conflict"&&error.Message=="The selected ball does not have one unique live model.";
 }
 static void AssetServiceEmit(string phase,string message,BallAssetSnapshot snapshot=null,BallAssetJournal journal=null){
  string line=json.Serialize(new{phase=phase,message=message,selected=assetServiceCommand,applied=journal==null?null:journal.logical,state=snapshot==null?null:AssetReport(snapshot,journal)});
  if(line==assetServiceLast)return;assetServiceLast=line;
  try{Plain(assetServiceLog);Directory.CreateDirectory(Path.GetDirectoryName(assetServiceLog));
   using(var f=new FileStream(assetServiceLog,FileMode.OpenOrCreate,FileAccess.Write,FileShare.Read)){
    if(f.Length>1024*1024)f.SetLength(0);f.Seek(0,SeekOrigin.End);
    byte[] b=Encoding.UTF8.GetBytes(json.Serialize(new{time=DateTime.UtcNow.ToString("o"),eventData=json.DeserializeObject(line)})+Environment.NewLine);f.Write(b,0,b.Length);f.Flush(true);
   }
  }catch{ /* Logging must not interrupt restoration. */ }
  try{Console.WriteLine(line);Console.Out.Flush();}catch(IOException){assetServiceEnded=true;}
 }
 static void AssetServiceInput(){
  try{string line;while((line=Console.ReadLine())!=null){if(BallAssetPolicy.ValidChoice(line))assetServiceDesired=line;}}
  catch(IOException){}finally{assetServiceEnded=true;}
 }
 static bool AssetJournalAlive(BallAssetJournal journal){
  if(journal==null)return false;
  try{using(var p=Process.GetProcessById(journal.pid)){return !p.HasExited&&p.StartTime.ToUniversalTime().ToFileTimeUtc()==journal.birth;}}
  catch(ArgumentException){return false;}
 }
 static BallAssetJournal AssetPrepareJournal(Process game,string image,string logical,BallAssetSnapshot s,HashSet<string> manifest){
  uint sourceIndex;uint source=BallResolve(logical,out sourceIndex);
  if(source==BallAssetPolicy.Normal||sourceIndex==s.index)throw new Exception("Selected ball is already the normal ball.");
  byte[] record=Read(s.records+sourceIndex*(ulong)BallSelectorPolicy.RecordStride,BallSelectorPolicy.RecordStride);
  if(D(record,4)!=source)throw new Exception("Selected ball registry changed.");string filename=AssetName(record);
  if(!manifest.Contains("balls/"+filename))throw new Exception("Selected ball is not in the stock manifest.");
  return new BallAssetJournal{pid=game.Id,birth=created,imageBase=basis,image=image,holder=s.holder,records=s.records,address=s.address,allocation=s.allocation,index=s.index,sourceModel=source,logical=logical,phase="prepared",original=s.record,replacement=BallAssetPolicy.Replacement(s.record,filename)};
 }
 internal static int RunBallAssetService(string[] args){
  Process game=null;BallAssetJournal owned=null;bool locked=false;
  using(var gate=new Mutex(false,"Local\\2kLauncherBallAssetExperiment"))try{
   long minimumBirth;
   if(args.Length!=6||!long.TryParse(args[3],out minimumBirth)||minimumBirth<0||!BallAssetPolicy.ValidChoice(args[4])||!new[]{"session","recover"}.Contains(args[5]))throw new Exception("Invalid ball asset service arguments.");
   string root=Path.GetFullPath(args[1]),file=Path.GetFullPath(args[2]),image=Path.Combine(root,"NBA2K27.exe");
   bool recovery=args[5]=="recover";
   if(!recovery&&minimumBirth==0)throw new Exception("A new launcher session is required.");
   assetServiceCommand=assetServiceDesired=args[4];assetServiceLog=file+".log.jsonl";
   try{locked=gate.WaitOne(0);}catch(AbandonedMutexException){locked=true;}
   if(!locked)throw new Exception("Another ball observer is restoring an earlier session. Close that game before launching again.");
   var saved=AssetReadJournal(file);
   if(recovery&&(saved==null||saved.phase=="restored"||!AssetJournalAlive(saved))){AssetServiceEmit("restored","No active ball substitution needs recovery.");return 0;}
   var input=new Thread(AssetServiceInput);input.IsBackground=true;input.Start();
   var waiting=Stopwatch.StartNew();
   while(game==null){
    if(assetServiceEnded&&!recovery){AssetServiceEmit("ended","Ball observer stopped before attaching; no ball assets changed.");return 0;}
    var games=Process.GetProcessesByName("NBA2K27");
    try{
     if(games.Length>1)throw new Exception("More than one NBA 2K27 process is running.");
     if(games.Length==1){var p=games[0];
      if(!String.Equals(Path.GetFullPath(p.MainModule.FileName),image,StringComparison.OrdinalIgnoreCase))throw new Exception("Running game does not match the launcher folder.");
      created=p.StartTime.ToUniversalTime().ToFileTimeUtc();basis=(ulong)p.MainModule.BaseAddress.ToInt64();
      if(recovery&&!AssetSame(saved,p.Id,image))throw new Exception("Ball recovery belongs to a different game session; no memory changed.");
      if(!recovery&&created<minimumBirth)throw new Exception("Ball selection requires a new game started with Play.");
      if(saved!=null&&!AssetSame(saved,p.Id,image)&&AssetJournalAlive(saved))throw new Exception("An earlier game still owns the ball recovery journal.");
      BallEac();handle=OpenProcess(0x438,false,p.Id);if(handle==IntPtr.Zero)throw new Exception("Run the launcher and game at the same permission level.");
      Identity();game=p;
      if(AssetSame(saved,p.Id,image)&&saved.phase!="restored")owned=saved;
     }
    }finally{foreach(var p in games)if(p!=game)p.Dispose();}
    if(game==null){if(waiting.ElapsedMilliseconds>120000){AssetServiceEmit("error","No new game session found. No ball assets changed.");return 1;}AssetServiceEmit("pending","Waiting for the new Play session.");Thread.Sleep(250);}
   }
   HashSet<string> manifest=null;bool buildReady=false,restoredInSession=false;string rejectedChoice=null;
   while(!game.HasExited){
    // Freeze this command for the iteration. Later commands are queued.
    string desired=assetServiceDesired;assetServiceCommand=desired;bool stopping=assetServiceEnded||recovery;
    try{
     if(!buildReady){AssetBuild();buildReady=true;}
     if(owned==null&&stopping){AssetServiceEmit("restored","Original ball restored.");return 0;}
     var s=AssetSnapshot();
     if(owned!=null){
      // Interrupted preparation/restoration: accept only the complete original.
      if(s.record.SequenceEqual(owned.original)){
       AssetOwned(s,owned,owned.original);owned.phase="restored";AssetSave(file,owned);owned=null;restoredInSession=true;continue;
      }
      AssetOwned(s,owned,owned.replacement);
      if(!owned.used&&BallAssetPolicy.ObservedLoad(s.references,s.resource)){owned.used=true;AssetSave(file,owned);}
      bool changed=!String.Equals(desired,owned.logical,StringComparison.OrdinalIgnoreCase);
      if(BallAssetPolicy.RestoreNow(owned.used,changed,stopping,recovery,BallAssetPolicy.MainMenu(s.current),s.quiet,s.references,s.resource)){
       AssetWrite(owned,true);owned.phase="restored";AssetSave(file,owned);owned=null;restoredInSession=true;
       AssetServiceEmit("restored","Original ball restored. The saved choice is ready for the next Blacktop setup.",AssetSnapshot());
       if(stopping)return 0;
      }else if(stopping)AssetServiceEmit("restoring","Leave the match and wait at the main menu to restore the original ball, or close the game.",s,owned);
      else if(changed)AssetServiceEmit("pending","Choice saved for the next match. The current ball stays unchanged until it unloads.",s,owned);
      else if(owned.used&&s.mode!=6)AssetServiceEmit("restoring","Return to the main menu and wait for Original ball restored before another mode.",s,owned);
      else if(Q(s.resource,16)!=0)AssetServiceEmit("applied","Selected ball loaded by the game. Original restores after Blacktop unloads.",s,owned);
      else if(owned.used)AssetServiceEmit("loading","Waiting for the game to finish loading or unloading the ball.",s,owned);
      else AssetServiceEmit("ready","Ready for Blacktop. Start the match when ready.",s,owned);
     }else if(BallAssetPolicy.Current(desired))AssetServiceEmit("current","Current installed ball will be used.",s);
     else if(s.selection&&s.unloaded&&BallAssetPolicy.Canonical(s.nativeBall)!=BallAssetPolicy.Normal)
      AssetServiceEmit("incompatible","The game selected a different stock ball model. Live swaps cannot prepare in this game process; use Current installed or restart NBA 2K27 before retrying.",s);
     else if(desired==rejectedChoice){ /* Keep error until another choice. */ }
     else if(!s.selection||!s.unloaded||BallAssetPolicy.Canonical(s.nativeBall)!=BallAssetPolicy.Normal)AssetServiceEmit(restoredInSession?"restored":"pending",restoredInSession?"Original ball restored. Your choice prepares at the next Blacktop player-selection screen.":"Return to Blacktop player selection and wait for Ready before starting the match.",s);
     else{
      if(manifest==null){string manifestFile=Path.Combine(root,"manifest");Plain(manifestFile);var info=new FileInfo(manifestFile);if(!info.Exists||info.Length<1||info.Length>64*1024*1024)throw new Exception("Stock ball manifest is unavailable.");
       manifest=new HashSet<string>(File.ReadLines(manifestFile).Select(line=>line.Split(',')[0].Replace('\\','/')).Where(line=>line.StartsWith("balls/",StringComparison.OrdinalIgnoreCase)),StringComparer.OrdinalIgnoreCase);
      }
      var candidate=AssetPrepareJournal(game,image,desired,s,manifest);
      AssetSave(file,candidate);owned=candidate; // durable ownership BEFORE the data write
      AssetWrite(owned,false);owned.phase="armed";AssetSave(file,owned);restoredInSession=false;
      AssetServiceEmit("ready","Ready for Blacktop. Start the match when ready.",AssetSnapshot(),owned);
     }
    }catch(Stop e){
     if(game.HasExited)break;
     if(e.phase=="waiting")AssetServiceEmit(owned==null?"pending":"restoring",owned==null?"Waiting for a stable Blacktop player-selection screen.":"Waiting for the ball to unload at a safe menu; no filename change attempted.");
     else{
      bool choiceUnavailable=owned==null&&buildReady&&AssetChoiceUnavailable(e);
      AssetServiceEmit(choiceUnavailable?"unavailable":"error",e.Message+(owned==null?"":" Close the game if restoration cannot complete."));
      if(owned==null){if(!buildReady)return 1;rejectedChoice=desired;}
     }
    }catch(Exception e){
     if(game.HasExited)break;
     AssetServiceEmit("error",e.Message+(owned==null?" No ball substitution applied.":" Close the game before another mode; recovery data retained."));
     if(owned==null){if(!buildReady)return 1;rejectedChoice=desired;}
     // Keep an owned journal until safe restoration or process exit.
    }
    Thread.Sleep(100);
   }
   AssetServiceEmit("ended","Game closed; temporary ball substitution is gone.");return 0;
  }catch(Exception e){AssetServiceEmit("error",e.Message);return 1;}
  finally{if(handle!=IntPtr.Zero){CloseHandle(handle);handle=IntPtr.Zero;}if(game!=null)game.Dispose();if(locked)gate.ReleaseMutex();}
 }
}
