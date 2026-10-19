using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Security.Cryptography;

namespace NBA2K27RosterResearch {
// Pure native import boundary. It accepts reviewed years, totals, awards and
// current team indexes, never caller-provided process addresses or patch bytes.
internal sealed class SeasonImportTeam { internal int Index; internal long Pointer; internal string Name,DisplayName; }
internal sealed class SeasonImportSnapshot {
    internal int RecordIndex;
    internal byte[] Players, Stats, Pool;
    internal List<SeasonImportTeam> Teams;
}
internal sealed class SeasonImportChoice {
    internal int Year, PrimaryTeamIndex, SecondaryTeamIndex = -1;
    internal Dictionary<string,int> Totals, Awards;
}
internal sealed class SeasonImportChange {
    internal string Table; internal int Offset; internal byte[] Before, After;
    internal SeasonImportChange(string table,int offset,byte[] before,byte[] after){Table=table;Offset=offset;Before=before;After=after;}
}
internal sealed class SeasonImportPlan {
    internal SeasonImportSnapshot Before, After;
    internal int OriginYear, Allocated;
    internal List<SeasonImportChange> Changes;
}
internal static class SeasonImportModel {
    internal const int PlayerStride=0x4F8, Directory=0x130, RegularSlots=25, AllSlots=33;
    private const uint ReferenceMask=0x1FE000u;
    internal static bool SafeMenu(uint current,uint alternate,int stage,byte active,byte kind){
        if(current!=alternate||active!=0||kind!=0)return false;
        // Roster list observed with a loaded test roster and verified against
        // the supported executable's menu getter. Do not admit other stages.
        return (current==0x6509837B&&(stage==0||stage==1))
            ||(current==0x079964C0&&stage==1)
            ||(current==0x0B888544&&stage==0);
    }
    internal static readonly string[] TotalKeys={"ftm","fta","fgm","fga","threeMade","threeAttempted","oreb","dreb","steals","blocks","assists","turnovers","points","gp","gs","minutes","fouls","doubleDoubles","tripleDoubles","plusMinus"};
    private static readonly int[,] TotalFields={{0x20,0,11},{0x20,11,11},{0x18,13,13},{0x14,15,14},{0x24,12,12},{0x24,0,12},{0x20,22,10},{0x28,0,12},{0x1C,0,11},{0x1C,11,11},{0x38,0,12},{0x28,12,11},{0x2C,0,14},{0x14,0,7},{0x38,12,7},{0x18,0,13},{0x1C,22,10},{0x34,25,7},{0x14,8,7},{0x2C,14,15}};
    internal static readonly string[] AwardKeys={"mvp","champion","finalsMvp","roy","sixthMan","dpoy","mip","allStar","allNba","allDefense","allRookie"};
    private static readonly int[,] AwardFields={{20,7,1,1},{43,6,1,1},{43,7,1,1},{27,5,1,1},{27,6,1,1},{27,7,1,1},{47,7,1,1},{39,7,2,2},{23,5,2,3},{47,5,2,2},{27,2,2,2}};
    internal static bool Equal(byte[] a,byte[] b){if(a==null||b==null||a.Length!=b.Length)return false;for(int i=0;i<a.Length;i++)if(a[i]!=b[i])return false;return true;}
    internal static byte[] Slice(byte[] a,int offset,int length){var b=new byte[length];Array.Copy(a,offset,b,0,length);return b;}
    private static void Put32(byte[] a,int offset,uint value){Array.Copy(BitConverter.GetBytes(value),0,a,offset,4);}
    private static void Put16(byte[] a,int offset,short value){Array.Copy(BitConverter.GetBytes(value),0,a,offset,2);}
    internal static int Count(byte[] a){return (int)((BitConverter.ToUInt32(a,60)>>13)&255);}
    private static void SetCount(byte[] a,int n){if(n<0||n>255)throw new Exception("Season reference count overflow.");Put32(a,60,(BitConverter.ToUInt32(a,60)&~ReferenceMask)|((uint)n<<13));}
    internal static byte[] Clear(byte[] source,ushort next){
        var a=(byte[])source.Clone();Array.Clear(a,0,48);Put32(a,48,BitConverter.ToUInt32(a,48)&0xF0000000u);Put32(a,52,0);
        Put32(a,56,BitConverter.ToUInt32(a,56)&0xF8000000u);Put32(a,60,BitConverter.ToUInt32(a,60)&0xF0000000u);Put32(a,16,(uint)next<<16);return a;
    }
    private sealed class Existing {internal int Year, Index; internal byte[] Raw;}
    private sealed class State {
        internal int PlayerCount, StatCount, Origin=-1;
        internal int[] References; internal byte[][] Rows;
        internal List<int> Free=new List<int>();
        internal Dictionary<int,SeasonImportTeam> Teams=new Dictionary<int,SeasonImportTeam>();
        internal Dictionary<int,Existing> Existing=new Dictionary<int,Existing>();
    }
    private static State Validate(SeasonImportSnapshot input){
        if(input==null||input.Players==null||input.Players.Length==0||input.Players.Length%PlayerStride!=0||input.Players.Length/PlayerStride>8192||input.Stats==null||input.Stats.Length==0||input.Stats.Length%64!=0||input.Stats.Length/64>=32768||input.Pool==null||input.Pool.Length!=64)throw new Exception("Unsupported native season snapshot.");
        var state=new State{PlayerCount=input.Players.Length/PlayerStride,StatCount=input.Stats.Length/64};
        if(input.RecordIndex<0||input.RecordIndex>=state.PlayerCount||input.Teams==null||input.Teams.Count<1||input.Teams.Count>1024)throw new Exception("Invalid destination player/team directory.");
        var pointers=new HashSet<long>();
        foreach(var t in input.Teams){
            if(t==null||t.Index<0||t.Index>=1024||state.Teams.ContainsKey(t.Index)||t.Pointer<=0||t.Pointer>0x7FFFFFFFFFFF||!pointers.Add(t.Pointer)||String.IsNullOrWhiteSpace(t.Name)||t.Name.Length>100)throw new Exception("Invalid destination team.");
            foreach(char c in t.Name)if(c<32)throw new Exception("Invalid destination team name.");
            state.Teams.Add(t.Index,t);
        }
        state.References=new int[state.StatCount];state.Rows=new byte[state.StatCount][];
        for(int p=0;p<state.PlayerCount;p++){
            if(BitConverter.ToInt64(input.Players,p*PlayerStride+0xE0)!=0)throw new Exception("Private season histories are not supported.");
            for(int s=0;s<AllSlots;s++){int i=BitConverter.ToInt16(input.Players,p*PlayerStride+Directory+s*2);if(i==-1)continue;if(i<0||i>=state.StatCount)throw new Exception("Invalid native season reference.");state.References[i]++;}
        }
        int used=0;
        for(int i=0;i<state.StatCount;i++){state.Rows[i]=Slice(input.Stats,i*64,64);if(state.References[i]!=Count(state.Rows[i]))throw new Exception("Native season reference counts disagree.");if(state.References[i]>0)used++;}
        var free=new HashSet<int>();int cursor=BitConverter.ToUInt16(input.Pool,0);
        while(cursor!=65535){
            if(cursor>=state.StatCount||state.References[cursor]!=0||!free.Add(cursor))throw new Exception("Invalid native free-season chain.");
            var raw=state.Rows[cursor];ushort next=BitConverter.ToUInt16(raw,18);
            if(!Equal(raw,Clear(raw,next)))throw new Exception("Free season record is not initialized.");state.Free.Add(cursor);cursor=next;
        }
        if(used+free.Count!=state.StatCount)throw new Exception("Native free-season chain is incomplete.");
        for(int slot=0;slot<RegularSlots;slot++){
            int i=BitConverter.ToInt16(input.Players,input.RecordIndex*PlayerStride+Directory+slot*2);if(i<0)continue;
            int year=1900+state.Rows[i][17];if(state.Existing.ContainsKey(year)||state.Origin>=0&&state.Origin!=year+slot)throw new Exception("Destination regular-season ordering is ambiguous.");
            state.Origin=year+slot;state.Existing.Add(year,new Existing{Year=year,Index=i,Raw=state.Rows[i]});
        }
        return state;
    }
    internal static void ValidateSnapshot(SeasonImportSnapshot input){Validate(input);}
    private static void ValidateValues(Dictionary<string,int> values,string[] keys){
        if(values==null||values.Count!=keys.Length)throw new Exception("Imported season fields are incomplete.");
        foreach(string key in keys)if(!values.ContainsKey(key))throw new Exception("Imported season field is missing: "+key);
    }
    private static byte[] Encode(byte[] raw,SeasonImportChoice choice){
        ValidateValues(choice.Totals,TotalKeys);var a=(byte[])raw.Clone();
        for(int i=0;i<TotalKeys.Length;i++){
            int value=choice.Totals[TotalKeys[i]],bits=TotalFields[i,2];bool signed=i==19;
            int min=signed?-(1<<(bits-1)):0,max=signed?(1<<(bits-1))-1:(1<<bits)-1;
            if(value<min||value>max)throw new Exception("Season total outside the stored range: "+TotalKeys[i]);
            uint mask=((1u<<bits)-1)<<TotalFields[i,1];Put32(a,TotalFields[i,0],(BitConverter.ToUInt32(a,TotalFields[i,0])&~mask)|(((uint)value&((1u<<bits)-1))<<TotalFields[i,1]));
        }
        var t=choice.Totals;
        if(t["gs"]>t["gp"]||t["doubleDoubles"]>t["gp"]||t["tripleDoubles"]>t["gp"]||t["ftm"]>t["fta"]||t["fgm"]>t["fga"]||t["threeMade"]>t["threeAttempted"]||t["threeMade"]>t["fgm"]||t["threeAttempted"]>t["fga"])throw new Exception("Invalid imported season totals.");
        if(choice.Awards!=null){
            ValidateValues(choice.Awards,AwardKeys);
            for(int i=0;i<AwardKeys.Length;i++){
                int v=choice.Awards[AwardKeys[i]];if(v<0||v>AwardFields[i,3])throw new Exception("Unsupported award tier.");
                if(i==7){a[39]=(byte)((a[39]&127)|(v==1?128:0));a[27]=(byte)((a[27]&239)|(v==2?16:0));}
                else{int mask=((1<<AwardFields[i,2])-1)<<AwardFields[i,1];a[AwardFields[i,0]]=(byte)((a[AwardFields[i,0]]&~mask)|(v<<AwardFields[i,1]));}
            }
        }
        return a;
    }
    internal static void ValidateChoice(SeasonImportChoice choice){
        if(choice==null||choice.Year<1900||choice.Year>2155||choice.PrimaryTeamIndex<0||choice.PrimaryTeamIndex>=1024||choice.SecondaryTeamIndex< -1||choice.SecondaryTeamIndex>=1024)throw new Exception("Invalid imported season year/team indexes.");Encode(new byte[64],choice);
    }
    internal static SeasonImportPlan Build(SeasonImportSnapshot input,IList<SeasonImportChoice> choices){
        var state=Validate(input);if(choices==null||choices.Count<1||choices.Count>RegularSlots)throw new Exception("Choose one to 25 seasons to import.");
        int origin=state.Origin;var years=new HashSet<int>();
        foreach(var c in choices){
            if(c==null||c.Year<1900||c.Year>2155||!years.Add(c.Year))throw new Exception("Choose one aggregate row per valid season year.");
            if(!state.Teams.ContainsKey(c.PrimaryTeamIndex)||c.SecondaryTeamIndex!=-1&&!state.Teams.ContainsKey(c.SecondaryTeamIndex))throw new Exception("Selected destination team is no longer available.");
            origin=Math.Max(origin,c.Year);
        }
        foreach(int year in state.Existing.Keys)if(origin-year>=RegularSlots)throw new Exception("Selected and existing history spans more than 25 regular-season slots. No seasons will be dropped.");
        foreach(int year in years)if(origin-year>=RegularSlots)throw new Exception("Selected and existing history spans more than 25 regular-season slots. No seasons will be dropped.");
        // Clone input too: a pending plan must not alias mutable caller buffers.
        var before=new SeasonImportSnapshot{RecordIndex=input.RecordIndex,Players=(byte[])input.Players.Clone(),Stats=(byte[])input.Stats.Clone(),Pool=(byte[])input.Pool.Clone(),Teams=input.Teams.ConvertAll(t=>new SeasonImportTeam{Index=t.Index,Pointer=t.Pointer,Name=t.Name})};
        var after=new SeasonImportSnapshot{RecordIndex=input.RecordIndex,Players=(byte[])input.Players.Clone(),Stats=(byte[])input.Stats.Clone(),Pool=(byte[])input.Pool.Clone(),Teams=before.Teams};
        int directory=input.RecordIndex*PlayerStride+Directory,allocated=0;
        for(int i=0;i<RegularSlots*2;i++)after.Players[directory+i]=255;
        foreach(var old in state.Existing.Values)Put16(after.Players,directory+(origin-old.Year)*2,(short)old.Index);
        foreach(var choice in choices){
            Existing old;state.Existing.TryGetValue(choice.Year,out old);
            var next=Encode(old!=null?old.Raw:new byte[64],choice);
            Array.Copy(BitConverter.GetBytes(state.Teams[choice.PrimaryTeamIndex].Pointer),0,next,0,8);
            Array.Copy(BitConverter.GetBytes(choice.SecondaryTeamIndex<0?0L:state.Teams[choice.SecondaryTeamIndex].Pointer),0,next,8,8);next[17]=(byte)(choice.Year-1900);
            bool changed=old==null||!Equal(next,old.Raw),shared=old!=null&&state.References[old.Index]>1;int index=old==null?-1:old.Index;
            if(changed){
                if(old==null||shared){
                    if(allocated>=state.Free.Count)throw new Exception("No free season records remain. Existing history will not be evicted.");index=state.Free[allocated++];
                    if(old==null){next=Encode(state.Rows[index],choice);Array.Copy(BitConverter.GetBytes(state.Teams[choice.PrimaryTeamIndex].Pointer),0,next,0,8);Array.Copy(BitConverter.GetBytes(choice.SecondaryTeamIndex<0?0L:state.Teams[choice.SecondaryTeamIndex].Pointer),0,next,8,8);next[17]=(byte)(choice.Year-1900);}
                    next[18]=255;next[19]=255;SetCount(next,1);
                    if(shared){var decrement=(byte[])old.Raw.Clone();SetCount(decrement,state.References[old.Index]-1);Array.Copy(decrement,0,after.Stats,old.Index*64,64);}
                }
                Array.Copy(next,0,after.Stats,index*64,64);
            }
            Put16(after.Players,directory+(origin-choice.Year)*2,(short)index);
        }
        if(allocated>0)Put16(after.Pool,0,unchecked((short)(allocated<state.Free.Count?state.Free[allocated]:65535)));
        Validate(after);var changes=new List<SeasonImportChange>();
        for(int i=0;i<state.StatCount;i++){var a=Slice(before.Stats,i*64,64);var b=Slice(after.Stats,i*64,64);if(!Equal(a,b))changes.Add(new SeasonImportChange("stats",i*64,a,b));}
        if(!Equal(before.Pool,after.Pool))changes.Add(new SeasonImportChange("pool",0,Slice(before.Pool,0,2),Slice(after.Pool,0,2)));
        if(!Equal(before.Players,after.Players))changes.Add(new SeasonImportChange("players",directory,Slice(before.Players,directory,50),Slice(after.Players,directory,50)));
        return new SeasonImportPlan{Before=before,After=after,OriginYear=origin,Allocated=allocated,Changes=changes};
    }
    internal static string Digest(SeasonImportSnapshot s){using(var sha=SHA256.Create()){
        sha.TransformBlock(s.Players,0,s.Players.Length,s.Players,0);sha.TransformBlock(s.Stats,0,s.Stats.Length,s.Stats,0);sha.TransformFinalBlock(s.Pool,0,s.Pool.Length);return BitConverter.ToString(sha.Hash).Replace("-","").ToLowerInvariant();
    }}
}
// The process adapter supplies fresh layout/identity checks, scoped memory
// access and recovery storage. Tests substitute a fault-injected memory port.
internal interface ISeasonImportMemory {
    void CheckOwner();
    byte[] Read(string table,int offset,int length);
    void Write(string table,int offset,byte[] bytes);
    string SaveRecovery(SeasonImportPlan plan);
    bool Suspend();
    bool Resume();
    long PausedMilliseconds {get;}
}
internal sealed class SeasonImportResult {internal bool Saved;internal string Backup;}
internal static class SeasonImportTransaction {
    private static void Verify(ISeasonImportMemory memory,SeasonImportSnapshot s){
        memory.CheckOwner();
        if(!SeasonImportModel.Equal(s.Players,memory.Read("players",0,s.Players.Length))||!SeasonImportModel.Equal(s.Stats,memory.Read("stats",0,s.Stats.Length))||!SeasonImportModel.Equal(s.Pool,memory.Read("pool",0,s.Pool.Length)))throw new Exception("Roster changed or full season readback failed.");
        memory.CheckOwner();
    }
    internal static SeasonImportResult Commit(ISeasonImportMemory memory,SeasonImportSnapshot snapshot,IList<SeasonImportChoice> choices){
        // Independently generate the native plan. No arbitrary after-buffer is
        // accepted, even if the UI review used the JavaScript planner.
        var plan=SeasonImportModel.Build(snapshot,choices);Verify(memory,plan.Before);
        if(plan.Changes.Count==0)return new SeasonImportResult{Saved=false};
        string backup=memory.SaveRecovery(plan);if(String.IsNullOrWhiteSpace(backup))throw new Exception("Season recovery backup was not confirmed. No changes were made.");
        bool paused=false;int attempted=0;Exception failure=null;
        try{
            paused=memory.Suspend();if(!paused)throw new Exception("Could not pause the game safely. No season changes were made.");
            try{
                Verify(memory,plan.Before);
                foreach(var change in plan.Changes){
                    memory.CheckOwner();if(memory.PausedMilliseconds>2000)throw new Exception("Season verification took too long.");
                    attempted++;memory.Write(change.Table,change.Offset,change.After);
                }
                Verify(memory,plan.After);if(memory.PausedMilliseconds>2000)throw new Exception("Season verification took too long.");
            }catch(Exception e){
                bool restored=true,sameOwner=true;
                try{memory.CheckOwner();}catch{sameOwner=false;restored=false;}
                // Include the attempted write: WriteProcessMemory may write a
                // prefix before failing. One failed rollback never stops the
                // remaining restoration attempts.
                if(sameOwner)for(int i=attempted-1;i>=0;i--){try{memory.CheckOwner();memory.Write(plan.Changes[i].Table,plan.Changes[i].Offset,plan.Changes[i].Before);}catch{restored=false;}}
                try{Verify(memory,plan.Before);}catch{restored=false;}
                failure=new Exception(e.Message+(restored?" Original history restored.":" Restoration could not be verified; reload the roster without saving.")+" Recovery: "+backup,e);
            }
        }catch(Exception e){failure=e;}
        finally{
            if(paused){bool resumed=false;try{resumed=memory.Resume();}catch{}if(!resumed){try{memory.Resume();}catch{}failure=new Exception((failure==null?"Season data was written and verified.":failure.Message)+" Game resume failed. Recovery: "+backup,failure);}}
        }
        if(failure!=null)throw failure;return new SeasonImportResult{Saved=true,Backup=backup};
    }
}
}
