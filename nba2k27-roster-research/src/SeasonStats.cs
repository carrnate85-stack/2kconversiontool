using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
using System.Security.Cryptography;
using System.Web.Script.Serialization;

namespace NBA2K27RosterResearch {
internal static partial class Program {
    // Current native getter 2DD2500. Only mapped totals and verified award bits may change.
    private static readonly int[,] SeasonAwardFields={{20,7,1,1},{43,6,1,1},{43,7,1,1},{27,5,1,1},{23,5,2,3},{47,5,2,2},{27,2,2,2},{27,6,1,1},{27,7,1,1},{47,7,1,1},{39,7,1,1},{27,4,1,1}};
    private static readonly int[,] SeasonFields={{0x20,0,11},{0x20,11,11},{0x18,13,13},{0x14,15,14},{0x24,12,12},{0x24,0,12},{0x20,22,10},{0x28,0,12},{0x1C,0,11},{0x1C,11,11},{0x38,0,12},{0x28,12,11},{0x2C,0,14},{0x14,0,7},{0x38,12,7},{0x18,0,13},{0x1C,22,10},{0x34,25,7},{0x14,8,7},{0x2C,14,15}};
    public sealed class SeasonRequest { public int recordIndex; public string firstName,lastName; public int cyberfaceId; public int slot=-1; public string token,after,structure,backupRoot; public bool confirmedShared,confirmedScoring,confirmedStructure; }
    private static string SeasonHex(byte[] b){return BitConverter.ToString(b).Replace("-","").ToLowerInvariant();}
    private static byte[] SeasonBytes(string s){if(s==null||s.Length!=128)throw new Exception("Invalid season snapshot.");var b=new byte[64];for(int i=0;i<64;i++)b[i]=Convert.ToByte(s.Substring(i*2,2),16);return b;}
    private static int SeasonValue(byte[] b,int index){return (int)((BitConverter.ToUInt32(b,SeasonFields[index,0])>>SeasonFields[index,1])&((1u<<SeasonFields[index,2])-1));}
    private static void ValidateSeasonPatch(byte[] before,byte[] after){
        byte[] allowed=new byte[64];
        for(int i=0;i<SeasonFields.GetLength(0);i++)for(int bit=0;bit<SeasonFields[i,2];bit++){int at=SeasonFields[i,0]*8+SeasonFields[i,1]+bit;allowed[at/8]|=(byte)(1<<(at%8));}
        for(int i=0;i<SeasonAwardFields.GetLength(0);i++){
            int offset=SeasonAwardFields[i,0],shift=SeasonAwardFields[i,1],mask=((1<<SeasonAwardFields[i,2])-1)<<shift;
            allowed[offset]|=(byte)mask;
            if((before[offset]&mask)!=(after[offset]&mask)&&((after[offset]&mask)>>shift)>SeasonAwardFields[i,3])throw new Exception("Unsupported award tier.");
        }
        if((((before[39]^after[39])&128)!=0||((before[27]^after[27])&16)!=0)&&(after[39]&128)!=0&&(after[27]&16)!=0)throw new Exception("Choose All-Star starter or reserve, not both.");
        for(int i=0;i<64;i++)if(((before[i]^after[i])&~allowed[i])!=0)throw new Exception("Season edit touches an unsupported field.");
        if(SeasonValue(after,14)>SeasonValue(after,13))throw new Exception("Games started exceeds games played.");
        if(SeasonValue(after,17)>SeasonValue(after,13)||SeasonValue(after,18)>SeasonValue(after,13))throw new Exception("Double/triple-doubles exceeds games played.");
        if(SeasonValue(after,0)>SeasonValue(after,1)||SeasonValue(after,2)>SeasonValue(after,3)||SeasonValue(after,4)>SeasonValue(after,5)||SeasonValue(after,4)>SeasonValue(after,2)||SeasonValue(after,5)>SeasonValue(after,3))throw new Exception("Invalid shooting totals.");
    }
    private sealed class SeasonContext {
        internal Reader m; internal long image,root,owner,table,stats,player; internal int count,statCount; internal byte[] identity;
        internal SeasonContext(Reader memory,SeasonRequest r){
            m=memory;image=Process.GetProcessById(m.ProcessId).MainModule.BaseAddress.ToInt64();
            var h=m.Read(image,4096);int pe=BitConverter.ToInt32(h,60);
            if(pe<64||pe>3800||BitConverter.ToUInt32(h,pe+8)!=0x6AA2013D)throw new Exception("Season stats are unsupported on this game build.");
            using(var sha=SHA256.Create())if(SeasonHex(sha.ComputeHash(m.Read(image+0x2DD2500,0x241)))!="49fc52f24163285b8d5a260125e897f6c694e4c898aa9c4030b456eb3741e743")throw new Exception("Season-stat layout verification failed.");
            root=Q(image+0x9A47EB8);owner=Q(root+0xA8);table=Q(owner+0x18);count=I(owner+0x10);stats=Q(owner+0x1D8);statCount=I(owner+0x1D0);
            if(count<100||count>8192||statCount<1||statCount>1000000||r.recordIndex<0||r.recordIndex>=count)throw new Exception("Invalid season directory.");
            player=table+(long)r.recordIndex*PlayerStride;identity=m.Read(player,PlayerStride);
            if(DecodeUtf16(identity,40,40)!=r.firstName||DecodeUtf16(identity,0,40)!=r.lastName||BitConverter.ToUInt16(identity,0x110)!=r.cyberfaceId)throw new Exception("The player changed. Refresh the roster.");
            if(BitConverter.ToInt64(identity,0xE0)!=0)throw new Exception("Private season histories are not supported yet.");Check();
        }
        internal long Q(long a){return BitConverter.ToInt64(m.Read(a,8),0);} internal int I(long a){return BitConverter.ToInt32(m.Read(a,4),0);}
        internal void Check(){if(Q(image+0x9A47EB8)!=root||Q(root+0xA8)!=owner||Q(owner+0x18)!=table||I(owner+0x10)!=count||Q(owner+0x1D8)!=stats||I(owner+0x1D0)!=statCount||!SameBytes(identity,m.Read(player,PlayerStride)))throw new Exception("The roster changed. Refresh season stats.");}
        internal int Index(int slot){if(slot<0||slot>24)throw new Exception("Invalid regular-season slot.");int index=BitConverter.ToInt16(identity,0x130+slot*2);if(index>=statCount)throw new Exception("Invalid season index.");return index;}
        internal List<int> Links(int index){var result=new List<int>();for(int i=0;i<count;i++){var record=m.Read(table+(long)i*PlayerStride+0xE0,0x92);if(BitConverter.ToInt64(record,0)!=0)continue;for(int s=0;s<33;s++)if(BitConverter.ToInt16(record,0x50+s*2)==index){result.Add(i);break;}}return result;}
        internal string Token(int slot,int index,byte[] raw,List<int> links){using(var sha=SHA256.Create())return SeasonHex(sha.ComputeHash(Encoding.UTF8.GetBytes(m.ProcessId+":"+root+":"+owner+":"+table+":"+stats+":"+player+":"+slot+":"+index+":"+SeasonHex(identity)+":"+SeasonHex(raw)+":"+String.Join(",",links))));}
        internal string Team(byte[] raw,int offset=0){long pointer=BitConverter.ToInt64(raw,offset),tt=Q(owner+0x88);int n=I(owner+0x80);long delta=pointer-tt;if(pointer==0)return offset==8?null:"—";if(n<1||n>1024||delta<0||delta%TeamStride!=0||delta/TeamStride>=n)return "Unknown team";var name=m.Read(pointer+TeamNameOffset,48);if(Q(owner+0x88)!=tt||I(owner+0x80)!=n||!SameBytes(name,m.Read(pointer+TeamNameOffset,48)))throw new Exception("The season team directory changed. Refresh before exporting.");return DecodeUtf16(name,0,48);}
    }
    private static int RunSeasonStats(string[] args){
        var json=new JavaScriptSerializer();string encoded=Argument(args,"--season-stats");if(encoded.Length>8192)throw new Exception("Season request is too large.");
        var r=json.Deserialize<SeasonRequest>(Encoding.UTF8.GetString(Convert.FromBase64String(encoded)));if(r==null)throw new Exception("Invalid season request.");
        bool write=r.after!=null||r.structure!=null;
        using(var m=new Reader(ProcessIdFromArgs(args),write)){
            var c=new SeasonContext(m,r);
            if(r.structure!=null)return ChangeSeasonStructure(c,r,json);
            if(write){
                int index=c.Index(r.slot);if(index<0)throw new Exception("This season no longer exists.");long address=c.stats+(long)index*64;var before=m.Read(address,64);var links=c.Links(index);
                if(c.Token(r.slot,index,before,links)!=r.token)throw new Exception("Season history or linked players changed. Refresh before saving.");
                if(links.Count>1&&!r.confirmedShared)throw new Exception("Confirm the shared-history warning before saving.");
                var after=SeasonBytes(r.after);ValidateSeasonPatch(before,after);
                if(SeasonValue(after,12)!=2*SeasonValue(after,2)+SeasonValue(after,4)+SeasonValue(after,0)&&!r.confirmedScoring)throw new Exception("Confirm that points differ from shooting totals.");
                c.Check();if(!SameBytes(before,m.Read(address,64)))throw new Exception("Season changed before saving.");
                bool attempted=false;
                try{attempted=true;m.Write(address,after);c.Check();if(!SameBytes(after,m.Read(address,64)))throw new Exception("Season readback failed.");}
                catch(Exception error){bool restored=false;if(attempted)try{c.Check();if(SameBytes(after,m.Read(address,64))){m.Write(address,before);restored=SameBytes(before,m.Read(address,64));}}catch{}throw new Exception(error.Message+(restored?" Original season restored.":" Restoration could not be verified; reload the roster without saving."));}
                Console.WriteLine(json.Serialize(new {ok=true,saved=true,linkedCount=links.Count}));return 0;
            }
            var rows=new List<object>();
            for(int slot=0;slot<25;slot++){int index=c.Index(slot);if(index<0)continue;var raw=m.Read(c.stats+(long)index*64,64);var links=c.Links(index);c.Check();if(!SameBytes(raw,m.Read(c.stats+(long)index*64,64)))throw new Exception("Season changed while reading.");rows.Add(new {slot=slot,year=1900+raw[0x11],team=c.Team(raw),secondaryTeam=c.Team(raw,8),raw=SeasonHex(raw),linkedPlayers=links.ConvertAll(i=>new {recordIndex=i}),token=c.Token(slot,index,raw,links)});}
            c.Check();Console.WriteLine(json.Serialize(new {ok=true,writes=0,rows=rows}));return 0;
        }
    }
}}
