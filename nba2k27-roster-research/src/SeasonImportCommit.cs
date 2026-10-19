using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.RegularExpressions;
using System.Web.Script.Serialization;

namespace NBA2K27RosterResearch {
internal static partial class Program {
    public sealed class SeasonImportRowRequest {public int year,primaryTeamIndex;public int secondaryTeamIndex=-1;public Dictionary<string,int> totals,awards;}
    public sealed class SeasonImportCommitRequest {public int recordIndex,cyberfaceId;public string firstName,lastName,token,expectedDigest,backupRoot;public bool confirmed;public List<SeasonImportRowRequest> seasons;}
    private static Dictionary<string,object> SeasonImportObject(object value,string[] keys){
        var item=value as Dictionary<string,object>;
        if(item==null||item.Count!=keys.Length)throw new Exception("Invalid confirmed season import request.");
        foreach(string key in keys)if(!item.ContainsKey(key))throw new Exception("Invalid confirmed season import request.");return item;
    }
    private static void ValidateSeasonImportJson(JavaScriptSerializer json,string input){
        // Typed deserialization coerces strings/fractions and defaults omitted
        // integers to zero. Reject those forms before any process is discovered.
        var request=SeasonImportObject(json.DeserializeObject(input),new[]{"recordIndex","cyberfaceId","firstName","lastName","token","expectedDigest","backupRoot","confirmed","seasons"});
        foreach(string key in new[]{"recordIndex","cyberfaceId"})if(!(request[key] is int))throw new Exception("Invalid confirmed season import request.");
        foreach(string key in new[]{"firstName","lastName","token","expectedDigest","backupRoot"})if(!(request[key] is string))throw new Exception("Invalid confirmed season import request.");
        if(!(request["confirmed"] is bool))throw new Exception("Invalid confirmed season import request.");
        var rows=request["seasons"] as object[];if(rows==null)throw new Exception("Invalid confirmed season import request.");
        foreach(object value in rows){
            var row=SeasonImportObject(value,new[]{"year","primaryTeamIndex","secondaryTeamIndex","totals","awards"});
            foreach(string key in new[]{"year","primaryTeamIndex","secondaryTeamIndex"})if(!(row[key] is int))throw new Exception("Invalid confirmed season import request.");
            foreach(string key in new[]{"totals","awards"}){
                if(key=="awards"&&row[key]==null)continue;
                var fields=row[key] as Dictionary<string,object>;if(fields==null)throw new Exception("Invalid confirmed season import request.");
                foreach(object number in fields.Values)if(!(number is int))throw new Exception("Invalid confirmed season import request.");
            }
        }
    }
    private sealed class SeasonImportMenu {
        internal int CurrentIndex, AlternateIndex, Stage;
        internal uint Current, Alternate;
        internal long Settings;
        internal byte Active, Kind;
        internal bool Safe {get{return SeasonImportModel.SafeMenu(Current,Alternate,Stage,Active,Kind);}}
        internal bool Same(SeasonImportMenu b){return CurrentIndex==b.CurrentIndex&&AlternateIndex==b.AlternateIndex&&Stage==b.Stage&&Current==b.Current&&Alternate==b.Alternate&&Settings==b.Settings&&Active==b.Active&&Kind==b.Kind;}
    }
    private static SeasonImportMenu ReadSeasonImportMenu(SeasonImportReadContext context){
        var c=context.Context;var menu=new SeasonImportMenu{CurrentIndex=c.I(c.image+0xAC77E28),AlternateIndex=c.I(c.image+0xAC77E24),Stage=c.I(c.image+0xAEB69D0+0xEA8),Settings=c.Q(c.image+0xAACB9A0),Active=c.m.Read(c.image+0xAACC310,1)[0]};
        if(menu.CurrentIndex<0||menu.CurrentIndex>=128||menu.AlternateIndex<0||menu.AlternateIndex>=128||menu.Settings<0x10000||menu.Settings>0x7FFFFFFFFFFF)throw new Exception("Stay at the NBA 2K27 roster-edit player list, Home or Quick Play menu before importing seasons.");
        menu.Current=BitConverter.ToUInt32(c.m.Read(c.image+0xAC77660+(long)menu.CurrentIndex*64,4),0);menu.Alternate=BitConverter.ToUInt32(c.m.Read(c.image+0xAC77660+(long)menu.AlternateIndex*64,4),0);menu.Kind=c.m.Read(menu.Settings,1)[0];
        if(!menu.Safe)throw new Exception("Stay at the NBA 2K27 roster-edit player list, Home or Quick Play menu before importing seasons. Close in-game player-edit/save dialogs. No gameplay/loading writes are allowed.");
        return menu;
    }
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)] private static extern uint GetFinalPathNameByHandle(IntPtr handle,StringBuilder path,uint size,uint flags);
    private static string SeasonImportBackupDirectory(string path){
        if(String.IsNullOrWhiteSpace(path)||!Path.IsPathRooted(path)||path.StartsWith(@"\\")||path.IndexOf(':',2)>=0)throw new Exception("Season recovery folder must be an absolute local directory.");
        string full=Path.GetFullPath(path);if(full==Path.GetPathRoot(full))throw new Exception("Season recovery folder cannot be a drive root.");
        // Reject links at every ancestor, both before and after creation.
        for(int pass=0;pass<2;pass++){
            for(var directory=new DirectoryInfo(full);directory!=null;directory=directory.Parent)if(directory.Exists&&(directory.Attributes&FileAttributes.ReparsePoint)!=0)throw new Exception("Season recovery directories cannot be links.");
            if(pass==0)Directory.CreateDirectory(full);
        }
        return full;
    }
    internal static string WriteSeasonImportRecoveryFile(string root,byte[] bytes){
        if(bytes==null||bytes.Length<1||bytes.Length>1024*1024)throw new Exception("Invalid season recovery contents.");
        string directory=SeasonImportBackupDirectory(root),file=Path.Combine(directory,DateTime.UtcNow.ToString("yyyyMMdd-HHmmss-fff")+"-"+Guid.NewGuid().ToString("N")+".json");
        using(var stream=new FileStream(file,FileMode.CreateNew,FileAccess.ReadWrite,FileShare.None,4096,FileOptions.WriteThrough)){
            var actual=new StringBuilder(32768);uint length=GetFinalPathNameByHandle(stream.SafeFileHandle.DangerousGetHandle(),actual,(uint)actual.Capacity,0);string resolved=actual.ToString();if(resolved.StartsWith(@"\\?\"))resolved=resolved.Substring(4);
            if(length==0||length>=actual.Capacity||!String.Equals(Path.GetFullPath(resolved),file,StringComparison.OrdinalIgnoreCase))throw new Exception("The recovery path changed before writing.");
            stream.Write(bytes,0,bytes.Length);stream.Flush(true);stream.Position=0;var check=new byte[bytes.Length];int at=0;while(at<check.Length){int n=stream.Read(check,at,check.Length-at);if(n<1)throw new Exception("Season recovery readback failed.");at+=n;}if(!SameBytes(bytes,check))throw new Exception("Season recovery readback failed.");
        }
        SeasonImportBackupDirectory(directory);return file;
    }
    private sealed class SeasonImportProcessMemory : ISeasonImportMemory,IDisposable {
        private SeasonImportReadContext context;private Reader writer;private SeasonImportMenu menu;private string backupRoot;private JavaScriptSerializer json;
        private IntPtr pauseHandle;private Stopwatch timer=new Stopwatch();private SeasonImportPlan approved;
        internal SeasonImportProcessMemory(SeasonImportReadContext c,Reader w,string root,JavaScriptSerializer serializer){
            context=c;writer=w;backupRoot=root;json=serializer;menu=ReadSeasonImportMenu(c);
            // Bound the known menu identity in the supported executable too.
            if(!SameBytes(c.Context.m.Read(c.Context.image+0x31436D0,10),new byte[]{0x48,0x89,0xD0,0xC7,0x02,0xC0,0x64,0x99,0x07,0xC3})||!SameBytes(c.Context.m.Read(c.Context.image+0x77F9368,21),Encoding.ASCII.GetBytes("NBA_MainMenu_PlayNow\0")))throw new Exception("Season import menu identity is unsupported.");
            if(!SameBytes(c.Context.m.Read(c.Context.image+0x35BC270,10),new byte[]{0x48,0x89,0xD0,0xC7,0x02,0x44,0x85,0x88,0x0B,0xC3}))throw new Exception("Season import roster-edit menu identity is unsupported.");
        }
        public void CheckOwner(){
            context.CheckOwner(true);if(!menu.Same(ReadSeasonImportMenu(context)))throw new Exception("The game menu changed during season import.");
            if(!SameBytes(context.TeamBytes,context.Context.m.Read(context.TeamAddress,context.TeamBytes.Length)))throw new Exception("The destination team directory changed during season import.");
        }
        private long Address(string table,int offset,int length){
            var s=context.Snapshot;int bound;long address;
            if(table=="players"){bound=s.Players.Length;address=context.Context.table;}else if(table=="stats"){bound=s.Stats.Length;address=context.Context.stats;}else if(table=="pool"){bound=64;address=context.PoolAddress;}else throw new Exception("Unsupported import table.");
            if(offset<0||length<1||offset>bound-length)throw new Exception("Import mutation is outside its verified table.");return address+offset;
        }
        public byte[] Read(string table,int offset,int length){return context.Context.m.Read(Address(table,offset,length),length);}
        public void Write(string table,int offset,byte[] value){
            if(pauseHandle==IntPtr.Zero||!timer.IsRunning||approved==null)throw new Exception("Season import write was not prepared and paused.");
            bool allowed=false;foreach(var change in approved.Changes)if(change.Table==table&&change.Offset==offset&&(SeasonImportModel.Equal(value,change.Before)||SeasonImportModel.Equal(value,change.After))){allowed=true;break;}
            if(!allowed)throw new Exception("Season import attempted an unplanned write.");writer.Write(Address(table,offset,value.Length),value);
        }
        public string SaveRecovery(SeasonImportPlan plan){
            CheckOwner();var rows=new List<object>();foreach(var change in plan.Changes)rows.Add(new {table=change.Table,offset=change.Offset,before=SeasonHex(change.Before),after=SeasonHex(change.After)});
            byte[] bytes=Encoding.UTF8.GetBytes(json.Serialize(new {schema=1,operation="selected-season-import",created=DateTime.UtcNow.ToString("o"),pid=context.Context.m.ProcessId,birth=context.Birth,token=context.Token,player=plan.Before.RecordIndex,root=context.Context.root,owner=context.Context.owner,players=context.Context.table,stats=context.Context.stats,pool=context.PoolAddress,changes=rows}));
            string file=WriteSeasonImportRecoveryFile(backupRoot,bytes);approved=plan;return file;
        }
        public bool Suspend(){
            if(pauseHandle!=IntPtr.Zero)throw new Exception("Season import pause already exists.");pauseHandle=Native.OpenProcess(0x800,false,context.Context.m.ProcessId);if(pauseHandle==IntPtr.Zero)return false;
            if(NtSuspendProcess(pauseHandle)<0)return false;timer.Start();return true;
        }
        public bool Resume(){if(pauseHandle==IntPtr.Zero)return false;if(NtResumeProcess(pauseHandle)<0)return false;timer.Stop();return true;}
        public long PausedMilliseconds {get{return timer.ElapsedMilliseconds;}}
        public void Dispose(){if(pauseHandle!=IntPtr.Zero){if(timer.IsRunning)NtResumeProcess(pauseHandle);Native.CloseHandle(pauseHandle);pauseHandle=IntPtr.Zero;}}
    }
    private static int RunSeasonImportCommit(string[] args){
        // STDIN avoids Windows command-line limits and keeps the reviewed career
        // and recovery location out of error messages containing command lines.
        var input=new StringBuilder();var chunk=new char[1024];int n;while((n=Console.In.Read(chunk,0,chunk.Length))>0){input.Append(chunk,0,n);if(input.Length>65536)throw new Exception("Season import request is too large.");}
        var json=new JavaScriptSerializer{MaxJsonLength=65536,RecursionLimit=32};ValidateSeasonImportJson(json,input.ToString());var r=json.Deserialize<SeasonImportCommitRequest>(input.ToString());
        if(r==null||!r.confirmed||r.recordIndex<0||r.recordIndex>=8192||r.cyberfaceId<0||r.cyberfaceId>65535||r.firstName==null||r.lastName==null||r.firstName.Length>19||r.lastName.Length>19||r.token==null||!Regex.IsMatch(r.token,@"\A[0-9a-f]{64}\z")||r.expectedDigest==null||!Regex.IsMatch(r.expectedDigest,@"\A[0-9a-f]{64}\z")||String.IsNullOrWhiteSpace(r.backupRoot)||r.seasons==null||r.seasons.Count<1||r.seasons.Count>25)throw new Exception("Invalid confirmed season import request.");
        var choices=new List<SeasonImportChoice>();var years=new HashSet<int>();foreach(var row in r.seasons){if(row==null||!years.Add(row.year))throw new Exception("Choose one aggregate row per imported season.");var choice=new SeasonImportChoice{Year=row.year,PrimaryTeamIndex=row.primaryTeamIndex,SecondaryTeamIndex=row.secondaryTeamIndex,Totals=row.totals,Awards=row.awards};SeasonImportModel.ValidateChoice(choice);choices.Add(choice);}
        foreach(var process in Process.GetProcesses()){using(process){if(process.ProcessName.StartsWith("EasyAntiCheat",StringComparison.OrdinalIgnoreCase))throw new Exception("Use an offline without-EAC launcher session for season imports.");}}
        using(var memory=new Reader(ProcessIdFromArgs(args),false)){
            var context=new SeasonImportReadContext(memory,new SeasonRequest{recordIndex=r.recordIndex,firstName=r.firstName,lastName=r.lastName,cyberfaceId=r.cyberfaceId});
            if(context.Token!=r.token)throw new Exception("Season history, roster ownership or teams changed. Review the import again.");
            var plan=SeasonImportModel.Build(context.Snapshot,choices);if(SeasonImportModel.Digest(plan.After)!=r.expectedDigest)throw new Exception("Native import does not match the reviewed changes. No seasons were written.");
            using(var writer=new Reader(memory.ProcessId,true))using(var adapter=new SeasonImportProcessMemory(context,writer,r.backupRoot,json)){
                var result=SeasonImportTransaction.Commit(adapter,context.Snapshot,choices);
                Console.OutputEncoding=new UTF8Encoding(false);Console.WriteLine(json.Serialize(new {ok=true,saved=result.Saved,backup=result.Backup,allocated=plan.Allocated,originYear=plan.OriginYear,afterDigest=SeasonImportModel.Digest(plan.After)}));return 0;
            }
        }
    }
}
}
