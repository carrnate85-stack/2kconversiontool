using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Web.Script.Serialization;

namespace NBA2K27RosterResearch {
internal static partial class Program {
    private static int RunTeamStaff(string[] args) {
        bool write=Array.IndexOf(args,"--team-staff-write")>=0;
        bool rotationWrite=Array.IndexOf(args,"--team-rotations-write")>=0;
        bool rotations=rotationWrite||Array.IndexOf(args,"--team-rotations")>=0;
        int staffIndex=-1,slot=-1;byte[] before=null,after=null;
        if(write){
            staffIndex=Int32.Parse(Argument(args,"--staff-index"));slot=Int32.Parse(Argument(args,"--slot"));
            string a=Argument(args,"--before"),b=Argument(args,"--after");
            if(a.Length!=608||b.Length!=608||staffIndex<0||staffIndex>=8192||slot<0||slot>=32)throw new Exception("Invalid staff edit.");
            before=Convert.FromBase64String(a);after=Convert.FromBase64String(b);TeamStaffPatch.Validate(before,after);
        }
        int index=Int32.Parse(Argument(args,"--record-index")), id=Int32.Parse(Argument(args,"--team-id"));
        string expected=Argument(args,"--expected-name");
        if(index<0||index>=1024||id<0||id>65535||expected.Length>24)throw new Exception("Invalid staff inspection request.");
        using(var m=new Reader(ProcessIdFromArgs(args),write||rotationWrite)) {
            long image=Process.GetProcessById(m.ProcessId).MainModule.BaseAddress.ToInt64();
            var spans=new int[,]{{0x6B5900,17580},{0x2EA5C10,46},{0x2F49F60,5},{0x64F840,12},{0x2EDFA80,16}};
            string[] hashes={"d98bbcc9e87c3cdde5125b0c8a5589828bf036d904e429eb4deda3833532e8cd","000bb80747b3d4ebe9e165fa47b106547c8f25f69ef31ec51b8f21cf43089b6f","0c6b8858e139e1a255688dfc8b1fca61c89ccea689e8da8bcd4276a9918f1dbe","b53ef12038b7b2fd6fa24737335056ce957f830e9f93b455b67488eda32bf1c5","78c7c43480f991a77429c97df422b8be8ba90acd0978b05b87e5cf9d1de43a8c"};
            for(int i=0;i<hashes.Length;i++)using(var sha=SHA256.Create())
                if(SeasonHex(sha.ComputeHash(m.Read(image+spans[i,0],spans[i,1])))!=hashes[i])throw new Exception("This game's staff layout is unsupported. No staff data read.");
            if(rotations)using(var sha=SHA256.Create()){
                if(SeasonHex(sha.ComputeHash(m.Read(image+0x2F50D70,215)))!="940d426855bd950c76ded48d05805eed942dc87a31761b78a759ed9fba177b92"
                    ||SeasonHex(sha.ComputeHash(m.Read(image+0x3299990,0x4DA)))!="193823f2f5b3b8719f23c6b9ef1c274ceb09640ddab0385159bcc46994c252ef")throw new Exception("This game's rotation layout is unsupported.");
            }
            long region,size,players;int playerCount;
            if(!LocateDatabase(m,out region,out size,out playerCount,out players))throw new Exception("Refresh the live roster before inspecting staff.");
            long root=BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0), owner=BitConverter.ToInt64(m.Read(root+0xA8,8),0);
            byte[] ownerPlayers=m.Read(owner+0x10,16), ownerTeams=m.Read(owner+0x80,16), ownerStaff=m.Read(owner+0x180,16);
            int teamCount=BitConverter.ToInt32(ownerTeams,0), staffCount=BitConverter.ToInt32(ownerStaff,0);
            long teams=BitConverter.ToInt64(ownerTeams,8),staff=BitConverter.ToInt64(ownerStaff,8);
            if(BitConverter.ToInt32(ownerPlayers,0)!=playerCount||BitConverter.ToInt64(ownerPlayers,8)!=players
                ||!ValidTableRange(region,size,teams,teamCount,TeamStride,1024)||index>=teamCount
                ||!ValidTableRange(region,size,staff,staffCount,456,8192))throw new Exception("The staff directory owner is unsupported.");
            long address=teams+(long)index*TeamStride;byte[] team=m.Read(address,TeamStride);
            if(BitConverter.ToUInt16(team,TeamUniqueIdOffset)!=id||DecodeUtf16(team,TeamNameOffset,48)!=expected)throw new Exception("The selected team changed. Refresh before inspecting staff.");
            byte[] records=m.Read(staff,staffCount*456);
            // Pure snapshot validation: aligned membership, role, reciprocal team.
            var rows=TeamStaffSnapshot.Read(team,address,records,staff);
            if(BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner
                ||!SameBytes(ownerPlayers,m.Read(owner+0x10,16))||!SameBytes(ownerTeams,m.Read(owner+0x80,16))
                ||!SameBytes(ownerStaff,m.Read(owner+0x180,16))||!SameBytes(team,m.Read(address,TeamStride))
                ||!SameBytes(records,m.Read(staff,records.Length)))throw new Exception("The roster changed during staff inspection. Refresh and try again.");
            if(rotations){
                byte[] playerRows=m.Read(players,playerCount*PlayerStride),rotationRaw=Slice(team,0x1360,480);
                var members=TeamRotationSnapshot.Members(address,playerRows,PlayerStride);
                if(!SameBytes(playerRows,m.Read(players,playerRows.Length))||!SameBytes(team,m.Read(address,TeamStride)))throw new Exception("The rotation roster changed during inspection.");
                string memberDigest=TeamRotationSnapshot.MemberDigest(members),teamDigest=TeamRotationSnapshot.Digest(team);
                if(!rotationWrite){Console.WriteLine(new JavaScriptSerializer().Serialize(new {ok=true,schema="nba2k27-team-rotations-v1",readOnly=true,writes=0,recordIndex=index,teamId=id,expectedName=expected,raw=Convert.ToBase64String(rotationRaw),members=members,memberDigest=memberDigest,teamDigest=teamDigest}));return 0;}
                if(Argument(args,"--member-digest")!=memberDigest||Argument(args,"--team-digest")!=teamDigest)throw new Exception("The team or its players changed. Refresh rotations before saving.");
                before=Convert.FromBase64String(Argument(args,"--before"));after=Convert.FromBase64String(Argument(args,"--after"));
                if(!SameBytes(before,rotationRaw))throw new Exception("The rotations changed. Refresh before saving.");TeamRotationSnapshot.Validate(before,after,members);
                var applied=new List<int>();byte[] expectedTeam=(byte[])team.Clone();
                try{
                    for(int i=0;i<480;i+=2)if(before[i]!=after[i]||before[i+1]!=after[i+1]){
                        if(BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerTeams,m.Read(owner+0x80,16))||!SameBytes(ownerPlayers,m.Read(owner+0x10,16))||!SameBytes(expectedTeam,m.Read(address,TeamStride)))throw new Exception("The rotation owner changed during Save.");
                        applied.Add(i);m.Write(address+0x1360+i,Slice(after,i,2));Array.Copy(after,i,expectedTeam,0x1360+i,2);
                        if(!SameBytes(Slice(after,i,2),m.Read(address+0x1360+i,2)))throw new Exception("Rotation read-back failed.");
                    }
                    if(!SameBytes(expectedTeam,m.Read(address,TeamStride))||!SameBytes(playerRows,m.Read(players,playerRows.Length))||BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerTeams,m.Read(owner+0x80,16))||!SameBytes(ownerPlayers,m.Read(owner+0x10,16)))throw new Exception("Rotation final verification failed.");
                }catch(Exception error){
                    bool restored=true;for(int k=applied.Count-1;k>=0;k--)try{int i=applied[k];
                        if(BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerTeams,m.Read(owner+0x80,16))||!SameBytes(ownerPlayers,m.Read(owner+0x10,16))||BitConverter.ToUInt16(m.Read(address+TeamUniqueIdOffset,2),0)!=id)throw new Exception("Rotation recovery owner changed.");
                        byte[] current=m.Read(address+0x1360+i,2);if(!SameBytes(current,Slice(after,i,2))&&!SameBytes(current,Slice(before,i,2)))throw new Exception("Foreign rotation edit; recovery refused.");
                        m.Write(address+0x1360+i,Slice(before,i,2));restored&=SameBytes(Slice(before,i,2),m.Read(address+0x1360+i,2));
                    }catch{restored=false;}
                    throw new Exception(error.Message+(restored?" Applied rotation cells restored.":" Rotation recovery needs attention."));
                }
                Console.WriteLine(new JavaScriptSerializer().Serialize(new {ok=true,schema="nba2k27-team-rotations-write-v1",writes=applied.Count,readBackVerified=true,recordIndex=index,teamId=id,teamDigest=TeamRotationSnapshot.Digest(expectedTeam)}));return 0;
            }
            if(write){
                long target=staff+(long)staffIndex*456;
                if(staffIndex>=staffCount||BitConverter.ToInt64(team,0xF70+slot*8)!=target||!SameBytes(before,m.Read(target,456)))throw new Exception("The staff record changed. Refresh before saving.");
                var applied=new List<int>();
                try{
                    for(int i=0;i<456;i++)if(before[i]!=after[i]){
                        if(BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerStaff,m.Read(owner+0x180,16))||!SameBytes(ownerTeams,m.Read(owner+0x80,16))||!SameBytes(team,m.Read(address,TeamStride)))throw new Exception("The staff owner changed during Save.");
                        if(m.Read(target+i,1)[0]!=before[i])throw new Exception("A staff byte changed before Save.");
                        applied.Add(i);m.Write(target+i,new byte[]{after[i]});
                        if(m.Read(target+i,1)[0]!=after[i])throw new Exception("Staff read-back failed.");
                    }
                    if(!SameBytes(after,m.Read(target,456))||!SameBytes(team,m.Read(address,TeamStride))||BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerStaff,m.Read(owner+0x180,16)))throw new Exception("Staff final verification failed.");
                }catch(Exception error){
                    bool restored=true;
                    for(int k=applied.Count-1;k>=0;k--)try{
                        int i=applied[k];
                        if(BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0)!=root||BitConverter.ToInt64(m.Read(root+0xA8,8),0)!=owner||!SameBytes(ownerStaff,m.Read(owner+0x180,16))||!SameBytes(team,m.Read(address,TeamStride)))throw new Exception("Owner changed; recovery write refused.");
                        byte current=m.Read(target+i,1)[0];if(current!=after[i]&&current!=before[i])throw new Exception("Foreign edit; recovery write refused.");
                        if(current!=before[i])m.Write(target+i,new byte[]{before[i]});restored&=m.Read(target+i,1)[0]==before[i];
                    }catch{restored=false;}
                    throw new Exception(error.Message+(restored?" Applied bytes restored.":" Recovery needs attention; use the saved snapshot."));
                }
                Console.WriteLine(new JavaScriptSerializer().Serialize(new {ok=true,schema="nba2k27-team-staff-write-v1",writes=applied.Count,readBackVerified=true,recordIndex=index,teamId=id,staffIndex=staffIndex}));
            }else Console.WriteLine(new JavaScriptSerializer().Serialize(new {ok=true,schema="nba2k27-team-staff-v1",writes=0,readOnly=true,recordIndex=index,teamId=id,expectedName=expected,rows=rows}));return 0;
        }
    }
}
internal static class TeamStaffSnapshot {
    internal static List<object> Read(byte[] team,long address,byte[] records,long table) {
        if(team==null||team.Length!=0x1550||records==null||records.Length%456!=0||records.Length>8192*456||table<=0||address<=0)throw new Exception("Invalid staff snapshot.");
        var rows=new List<object>();var seen=new HashSet<long>();
        for(int slot=0;slot<32;slot++) {
            long pointer=BitConverter.ToInt64(team,0xF70+slot*8);if(pointer==0)continue;
            if(pointer<table||(pointer-table)%456!=0||(pointer-table)/456>=records.Length/456||!seen.Add(pointer))throw new Exception("A team staff slot is not a unique aligned staff record.");
            int offset=checked((int)(pointer-table));
            if(BitConverter.ToInt64(records,offset+0x18)!=address||(records[offset+0x182]&31)!=slot)throw new Exception("A staff role or reciprocal team link does not match.");
            byte[] row=new byte[456];Array.Copy(records,offset,row,0,456);
            rows.Add(new {slot=slot,recordIndex=offset/456,raw=Convert.ToBase64String(row)});
        }
        return rows;
    }
}
}
