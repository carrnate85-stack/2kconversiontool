using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;
namespace NBA2K27RosterResearch {
internal static partial class Program {
    [DllImport("ntdll.dll")] private static extern int NtSuspendProcess(IntPtr h);
    [DllImport("ntdll.dll")] private static extern int NtResumeProcess(IntPtr h);
    private sealed class SeasonMutation { internal long address; internal byte[] before,after; internal SeasonMutation(long a,byte[] b,byte[] c){address=a;before=b;after=c;} }
    private static byte[] SliceSeason(byte[] b,int start,int length){var v=new byte[length];Array.Copy(b,start,v,0,length);return v;}
    private static void PutSeason(byte[] b,int at,uint value){Array.Copy(BitConverter.GetBytes(value),0,b,at,4);}
    private static byte[] ClearSeasonForPool(byte[] source,ushort next){
        var result=(byte[])source.Clone();Array.Clear(result,0,48);
        PutSeason(result,48,BitConverter.ToUInt32(result,48)&0xF0000000u);PutSeason(result,52,0);
        PutSeason(result,56,BitConverter.ToUInt32(result,56)&0xF8000000u);PutSeason(result,60,BitConverter.ToUInt32(result,60)&0xF0000000u);
        PutSeason(result,16,(uint)next<<16);return result;
    }
    private static int ChangeSeasonStructure(SeasonContext c,SeasonRequest r,JavaScriptSerializer json){
        if(!r.confirmedStructure||(r.structure!="add-older"&&r.structure!="remove-oldest")||r.after!=null)throw new Exception("Invalid season structure request.");
        bool add=r.structure=="add-older";
        int last=-1,anchor=-1,anchorYear=0,previousYear=3000;
        for(int s=0;s<25;s++){
            int ix=c.Index(s);if(ix<0)continue;
            var row=c.m.Read(c.stats+(long)ix*64,64);int year=1900+row[17];
            if(year>=previousYear)throw new Exception("Season order is unsupported. No rows were changed.");
            previousYear=year;last=s;anchor=ix;anchorYear=year;
        }
        if(last<0||r.slot!=last||add&&(last>=24||anchorYear<=1900))throw new Exception("Only an older season after existing history is supported. Refresh seasons.");
        // Match year-offset slot policy; no middle insertion or compaction.
        int origin=anchorYear+last;
        for(int s=0;s<=last;s++){int ix=c.Index(s);if(ix>=0&&1900+c.m.Read(c.stats+(long)ix*64+17,1)[0]+s!=origin)throw new Exception("This history has unsupported gaps or ordering.");}
        var anchorRaw=c.m.Read(c.stats+(long)anchor*64,64);var links=c.Links(anchor);
        if(c.Token(last,anchor,anchorRaw,links)!=r.token)throw new Exception("Season history changed. Refresh before continuing.");
        if(c.statCount>=32768||c.I(c.owner+0x1C0)!=1)throw new Exception("Unsupported season pool.");
        long pool=c.Q(c.owner+0x1C8);var poolRaw=c.m.Read(pool,64);
        long[] callbacks={0x2EB59E0,0x2EB59B0,0x2F9A7A0,0x2F9A760};
        for(int i=0;i<4;i++)if(BitConverter.ToInt64(poolRaw,24+i*8)!=c.image+callbacks[i])throw new Exception("Season pool callback verification failed.");
        var players=c.m.Read(c.table,c.count*PlayerStride);var stats=c.m.Read(c.stats,c.statCount*64);var counts=new int[c.statCount];
        if(!SameBytes(anchorRaw,SliceSeason(stats,anchor*64,64))||!SameBytes(c.identity,SliceSeason(players,r.recordIndex*PlayerStride,PlayerStride)))throw new Exception("History changed during verification.");
        for(int i=0;i<c.count;i++){
            if(BitConverter.ToInt64(players,i*PlayerStride+0xE0)!=0)throw new Exception("Private season histories are not supported.");
            for(int s=0;s<33;s++){int ix=BitConverter.ToInt16(players,i*PlayerStride+0x130+s*2);if(ix<0)continue;if(ix>=counts.Length)throw new Exception("Invalid season reference.");counts[ix]++;}
        }
        int used=0;for(int i=0;i<counts.Length;i++){if(((BitConverter.ToUInt32(stats,i*64+60)>>13)&255)!=counts[i])throw new Exception("Season reference counts do not match.");if(counts[i]>0)used++;}
        ushort head=BitConverter.ToUInt16(poolRaw,0),cursor=head;var free=new HashSet<int>();
        while(cursor!=65535){if(cursor>=counts.Length||counts[cursor]!=0||!free.Add(cursor))throw new Exception("Season free pool is invalid.");cursor=BitConverter.ToUInt16(stats,cursor*64+18);}
        if(used+free.Count!=counts.Length)throw new Exception("Season pool is incomplete.");
        if(add&&free.Count==0)throw new Exception("No free season records remain. Existing history will not be evicted.");
        if(!add&&counts[anchor]!=1)throw new Exception("This season is shared. Shared-season removal is not supported yet.");
        int target=add?head:anchor,slot=add?last+1:last;
        var before=SliceSeason(stats,target*64,64);byte[] after;ushort next;
        if(add){
            long team=BitConverter.ToInt64(anchorRaw,0),tt=c.Q(c.owner+0x88);int tn=c.I(c.owner+0x80);
            if(tn<1||tn>1024||team<tt||(team-tt)%TeamStride!=0||(team-tt)/TeamStride>=tn)throw new Exception("The oldest season has no verified team to copy.");
            next=BitConverter.ToUInt16(before,18);
            if(!SameBytes(before,ClearSeasonForPool(before,next)))throw new Exception("Free season record is not initialized.");
            after=(byte[])before.Clone();Array.Copy(BitConverter.GetBytes(team),after,8);after[17]=(byte)(anchorYear-1-1900);
            after[18]=255;after[19]=255;PutSeason(after,60,(BitConverter.ToUInt32(after,60)&~0x1FE000u)|0x2000u);
        }else{next=(ushort)target;after=ClearSeasonForPool(before,head);}
        int playerOffset=r.recordIndex*PlayerStride+0x130+slot*2;
        var mutations=new[]{new SeasonMutation(pool,SliceSeason(poolRaw,0,2),BitConverter.GetBytes(next)),new SeasonMutation(c.stats+(long)target*64,before,after),new SeasonMutation(c.table+playerOffset,SliceSeason(players,playerOffset,2),BitConverter.GetBytes((short)(add?target:-1)))};
        var expectedPlayers=(byte[])players.Clone();Array.Copy(mutations[2].after,0,expectedPlayers,playerOffset,2);
        var expectedStats=(byte[])stats.Clone();Array.Copy(after,0,expectedStats,target*64,64);
        var expectedPool=(byte[])poolRaw.Clone();Array.Copy(mutations[0].after,expectedPool,2);
        c.Check();
        var backupRows=new List<object>();foreach(var change in mutations)backupRows.Add(new {address=change.address,before=SeasonHex(change.before),after=SeasonHex(change.after)});
        if(String.IsNullOrWhiteSpace(r.backupRoot)||!Path.IsPathRooted(r.backupRoot))throw new Exception("Season recovery folder must be absolute.");
        string backupDir=Path.GetFullPath(r.backupRoot);
        Directory.CreateDirectory(backupDir);
        string backup=Path.Combine(backupDir,DateTime.UtcNow.ToString("yyyyMMdd-HHmmss-fff")+"-"+Guid.NewGuid().ToString("N")+".json");
        File.WriteAllText(backup,json.Serialize(new {pid=c.m.ProcessId,operation=r.structure,player=r.recordIndex,changes=backupRows}));
        IntPtr handle=Native.OpenProcess(0x800,false,c.m.ProcessId);if(handle==IntPtr.Zero)throw new Exception("Could not pause the game safely. No season changes were made.");
        bool paused=false;int attempted=0;var timer=new Stopwatch();
        try{
            if(NtSuspendProcess(handle)<0)throw new Exception("Could not pause the game safely.");paused=true;timer.Start();
            try{
                c.Check();if(c.Q(c.owner+0x1C8)!=pool||!SameBytes(players,c.m.Read(c.table,players.Length))||!SameBytes(stats,c.m.Read(c.stats,stats.Length))||!SameBytes(poolRaw,c.m.Read(pool,64)))throw new Exception("Roster changed before editing.");
                if(timer.ElapsedMilliseconds>2000)throw new Exception("Season verification took too long.");
                foreach(var change in mutations){attempted++;c.m.Write(change.address,change.after);}
                if(!SameBytes(expectedPlayers,c.m.Read(c.table,players.Length))||!SameBytes(expectedStats,c.m.Read(c.stats,stats.Length))||!SameBytes(expectedPool,c.m.Read(pool,64)))throw new Exception("Season structure readback failed.");
            }catch{
                for(int i=attempted-1;i>=0;i--)c.m.Write(mutations[i].address,mutations[i].before);
                for(int i=0;i<attempted;i++)if(!SameBytes(mutations[i].before,c.m.Read(mutations[i].address,mutations[i].before.Length)))throw new Exception("Rollback failed. Reload the roster without saving.");
                throw;
            }finally{if(NtResumeProcess(handle)<0)throw new Exception("Game resume failed.");paused=false;}
        }finally{if(paused)NtResumeProcess(handle);Native.CloseHandle(handle);}
        Console.WriteLine(json.Serialize(new {ok=true,saved=true,operation=r.structure,year=add?anchorYear-1:anchorYear,backup=backup}));return 0;
    }
}}
