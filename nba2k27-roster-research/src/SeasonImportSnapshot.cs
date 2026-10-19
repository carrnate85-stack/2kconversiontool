using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;
using System.Web.Script.Serialization;

namespace NBA2K27RosterResearch {
internal static partial class Program {
    // Fingerprints audited from the supported executable, not inferred from a
    // player name or an unversioned pointer. Snapshot mode is read-only.
    private static readonly long[] SeasonImportCodeRvas={0x2DD22C0,0x2DD2500,0x2E25380,0x2E253B0,0x2E254F0,0x2E25CA0,0x2F9A760,0x2F9A7A0};
    private static readonly int[] SeasonImportCodeLengths={561,577,40,45,10,594,60,232};
    private static readonly string[] SeasonImportCodeHashes={
        "c2c4e301b3ba38147b77fe698bd480d51e8a53682f0095d2d8b241c647840b0b",
        "49fc52f24163285b8d5a260125e897f6c694e4c898aa9c4030b456eb3741e743",
        "810a603f3bd2f251e4ab58d4830d388b7050a1526484cfa0ccc165f034efc03d",
        "048d0d78397f4d44d70be37c2221460336ba982b2cccf82e226fa1e3a3be8973",
        "e6cb9c82fe8d5804d4ac471688bdb7a91c150b7efe7f46b1845545aa605feb09",
        "352dd04e6d4209d5d71b205ad517be3775926daa6434a44ad71c18a4c76dfd7d",
        "52ad9d8b2c3282f0d6fa8ca3c4c4a8624282cb9fadf65190015babb2caf99cc0",
        "bccbd7505ba9f7461b3c4a5d72a78f8eedb138df36cde917a991ecd919910520"
    };
    private sealed class SeasonImportReadContext {
        internal SeasonContext Context;
        internal long PoolAddress, TeamAddress;
        internal int TeamCount;
        internal long Birth;
        internal byte[] TeamBytes;
        internal SeasonImportSnapshot Snapshot;
        internal string Token;
        internal SeasonImportReadContext(Reader memory,SeasonRequest request){
            var c=new SeasonContext(memory,request);Context=c;
            using(var process=System.Diagnostics.Process.GetProcessById(memory.ProcessId))Birth=process.StartTime.ToUniversalTime().Ticks;
            using(var sha=SHA256.Create())for(int i=0;i<SeasonImportCodeRvas.Length;i++)if(SeasonHex(sha.ComputeHash(memory.Read(c.image+SeasonImportCodeRvas[i],SeasonImportCodeLengths[i])))!=SeasonImportCodeHashes[i])throw new Exception("Season import layout verification failed.");
            if(c.statCount>=32768||c.I(c.owner+0x1C0)!=1)throw new Exception("Unsupported season import pool.");
            PoolAddress=c.Q(c.owner+0x1C8);TeamAddress=c.Q(c.owner+0x88);TeamCount=c.I(c.owner+0x80);
            if(PoolAddress<=0||TeamAddress<=0||TeamCount<1||TeamCount>1024)throw new Exception("Invalid season import owner.");
            var pool=memory.Read(PoolAddress,64);long[] callbacks={0x2EB59E0,0x2EB59B0,0x2F9A7A0,0x2F9A760};
            for(int i=0;i<callbacks.Length;i++)if(BitConverter.ToInt64(pool,24+i*8)!=c.image+callbacks[i])throw new Exception("Season pool callback verification failed.");
            var players=memory.Read(c.table,c.count*PlayerStride);var stats=memory.Read(c.stats,c.statCount*64);TeamBytes=memory.Read(TeamAddress,TeamCount*TeamStride);
            var teams=new List<SeasonImportTeam>();
            for(int i=0;i<TeamCount;i++){
                string name=DecodeUtf16(TeamBytes,i*TeamStride+TeamNameOffset,48);
                if(String.IsNullOrWhiteSpace(name))name="Unnamed team #"+i;
                string city=DecodeUtf16(TeamBytes,i*TeamStride+TeamCityOffset,36),type=RosterTypeName(Bits(TeamBytes,i*TeamStride+TeamRosterTypeOffset,2,5)),year=HistoricYearName(Bits(TeamBytes,i*TeamStride+TeamHistoricYearOffset,3,7));
                string display=(city+" "+name).Trim()+" · "+type+(year.Length>0?" · "+year:"");
                teams.Add(new SeasonImportTeam{Index=i,Pointer=TeamAddress+(long)i*TeamStride,Name=name,DisplayName=display});
            }
            Snapshot=new SeasonImportSnapshot{RecordIndex=request.recordIndex,Players=players,Stats=stats,Pool=pool,Teams=teams};
            SeasonImportModel.ValidateSnapshot(Snapshot);CheckSnapshot();
            using(var sha=SHA256.Create()){
                byte[] binding=Encoding.UTF8.GetBytes(memory.ProcessId+":"+Birth+":"+c.image+":"+c.root+":"+c.owner+":"+c.table+":"+c.count+":"+c.stats+":"+c.statCount+":"+PoolAddress+":"+TeamAddress+":"+TeamCount+":"+request.recordIndex+":");
                sha.TransformBlock(binding,0,binding.Length,binding,0);
                foreach(byte[] b in new[]{players,stats,pool})sha.TransformBlock(b,0,b.Length,b,0);
                sha.TransformFinalBlock(TeamBytes,0,TeamBytes.Length);Token=SeasonHex(sha.Hash);
            }
        }
        internal void CheckOwner(bool allowRegularDirectoryChanges=false){
            var c=Context;
            using(var process=System.Diagnostics.Process.GetProcessById(c.m.ProcessId))if(process.StartTime.ToUniversalTime().Ticks!=Birth)throw new Exception("The game process changed. Refresh before continuing.");
            if(!allowRegularDirectoryChanges)c.Check();
            else{
                if(c.Q(c.image+0x9A47EB8)!=c.root||c.Q(c.root+0xA8)!=c.owner||c.Q(c.owner+0x18)!=c.table||c.I(c.owner+0x10)!=c.count||c.Q(c.owner+0x1D8)!=c.stats||c.I(c.owner+0x1D0)!=c.statCount)throw new Exception("The season directory owner changed.");
                var identity=c.m.Read(c.player,PlayerStride);
                for(int i=0;i<identity.Length;i++)if((i<0x130||i>=0x162)&&identity[i]!=c.identity[i])throw new Exception("The destination player changed during import.");
            }
            if(c.I(c.owner+0x1C0)!=1||c.Q(c.owner+0x1C8)!=PoolAddress||c.Q(c.owner+0x88)!=TeamAddress||c.I(c.owner+0x80)!=TeamCount)throw new Exception("Season import owner changed. Refresh before continuing.");
        }
        internal void CheckSnapshot(){
            CheckOwner();var c=Context;
            if(!SameBytes(Snapshot.Players,c.m.Read(c.table,Snapshot.Players.Length))||!SameBytes(Snapshot.Stats,c.m.Read(c.stats,Snapshot.Stats.Length))||!SameBytes(Snapshot.Pool,c.m.Read(PoolAddress,64))||!SameBytes(TeamBytes,c.m.Read(TeamAddress,TeamBytes.Length)))throw new Exception("Season import snapshot changed while reading. Refresh before continuing.");
            CheckOwner();
        }
    }
    private static int RunSeasonImportSnapshot(string[] args){
        string encoded=Argument(args,"--season-import-snapshot");if(encoded.Length>8192)throw new Exception("Season snapshot request is too large.");
        var json=new JavaScriptSerializer{MaxJsonLength=64*1024*1024};
        string input=Encoding.UTF8.GetString(Convert.FromBase64String(encoded));
        try{var shape=SeasonImportObject(json.DeserializeObject(input),new[]{"recordIndex","firstName","lastName","cyberfaceId"});if(!(shape["recordIndex"] is int)||!(shape["cyberfaceId"] is int)||!(shape["firstName"] is string)||!(shape["lastName"] is string))throw new Exception();}
        catch{throw new Exception("Invalid read-only season snapshot request.");}
        var request=json.Deserialize<SeasonRequest>(input);
        if(request==null||request.recordIndex<0||request.recordIndex>=8192||request.firstName==null||request.lastName==null||request.firstName.Length>19||request.lastName.Length>19||request.cyberfaceId<0||request.cyberfaceId>65535||request.after!=null||request.structure!=null||request.token!=null||request.backupRoot!=null||request.slot!=-1||request.confirmedShared||request.confirmedScoring||request.confirmedStructure)throw new Exception("Invalid read-only season snapshot request.");
        using(var memory=new Reader(ProcessIdFromArgs(args),false)){
            var context=new SeasonImportReadContext(memory,request);var s=context.Snapshot;
            var teams=s.Teams.ConvertAll(t=>new {index=t.Index,name=t.Name,displayName=t.DisplayName,pointer=t.Pointer.ToString(System.Globalization.CultureInfo.InvariantCulture)});
            Console.OutputEncoding=new UTF8Encoding(false);
            Console.WriteLine(json.Serialize(new {ok=true,writes=0,snapshotId=context.Token,recordIndex=s.RecordIndex,players=SeasonHex(s.Players),stats=SeasonHex(s.Stats),pool=SeasonHex(s.Pool),teams=teams}));return 0;
        }
    }
}
}
