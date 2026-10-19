using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;

internal static class FreestylePolicy {
 internal const int Stride=0x1550;
 internal const ulong StockMask=0x401033, CombinedMask=0x68400033;
 internal static readonly ulong[] Pages={0x400031,0x400002,0x48000000,0x20400030,0x400000};
 internal static bool Stock(ulong mask,uint category){return mask==StockMask&&category<5;}
 internal static bool Applied(ulong mask,uint category){return mask==CombinedMask&&category==5;}
 internal static void Require(bool ok,string reason){if(!ok)throw new InvalidOperationException(reason);}
 internal static int VerifyTeams(byte[] rows,int count){
  Require(count>0&&count<=1024&&rows.Length==count*Stride,"Team table length rejected.");
  var women=new HashSet<int>();var national=new HashSet<int>();var counts=new Dictionary<int,int>();int eligible=0;
  for(int i=0;i<count;i++){
   int at=i*Stride,id=BitConverter.ToUInt16(rows,at+0xf20),kind=(int)(BitConverter.ToUInt32(rows,at+0x123c)>>26);
   if((CombinedMask&(1UL<<kind))!=0)eligible++;
   if(!counts.ContainsKey(kind))counts[kind]=0;counts[kind]++;
   if(kind==29||(kind==22&&(id==69||id==361))){
    string name=Encoding.Unicode.GetString(rows,at+0x2fa,48).Split('\0')[0];
    Require(name.Length>0&&name.All(c=>!Char.IsControl(c)),"Team names are not initialized.");
    Require((kind==29?women:national).Add(id),"Duplicate team identity rejected.");
   }
  }
  Require(women.SetEquals(Enumerable.Range(300,12).Concat(new[]{315,316,317})),"Waiting for the mixed NBA/WNBA roster.");
  Require(national.SetEquals(new[]{69,361}),"Waiting for both national teams.");
  Require(counts.ContainsKey(0)&&counts[0]==30&&counts.ContainsKey(1)&&counts[1]>0&&counts.ContainsKey(27)&&counts[27]>0,"NBA, Classic and All-Time teams are not ready.");
  return eligible;
 }
 internal static bool Same(FreestyleJournal j,int pid,long created,ulong basis,string image){
  return j!=null&&j.schema==1&&j.pid==pid&&j.created==created&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);
 }
 internal static void Validate(FreestyleJournal j){
  Require(j.schema==1&&j.pid>0&&j.created>0&&j.basis>=0x10000&&j.originalCategory<5&&
   new[]{"applied","prepared","mask-written","outside","failed"}.Contains(j.phase),"Unlock session data is invalid.");
 }
}
internal sealed class FreestyleJournal {
 public int schema=1,pid,count;public long created;public ulong basis,root,owner,table;public uint originalCategory;
 public string image,phase;public int eligible;
}
