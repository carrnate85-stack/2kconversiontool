using System;
using System.Linq;
internal static class CategoryPolicy {
 internal static uint Page=0x7a2b000;
 internal static readonly uint[][] Original={new uint[]{4,0,1,3,2,5},new uint[]{1,2,4,3,0,5}};
 internal static readonly uint[][] Enabled={new uint[]{4,0,1,2,3,5},new uint[]{1,2,3,4,0,5}};
 // Exact data-only allowlist: previous(3), previous(4), next(2), next(3).
 internal static uint[][] Changes={new uint[]{0x7a2bebc,3,2},new uint[]{0x7a2bec0,2,3},new uint[]{0x7a2bed8,4,3},new uint[]{0x7a2bedc,3,4}};
 internal static void SelectBuild(bool updated){
  Page=updated?0x7A2F000U:0x7A2B000U;
  Changes=updated?new[]{new uint[]{0x7A2FE2C,3,2},new uint[]{0x7A2FE30,2,3},new uint[]{0x7A2FE48,4,3},new uint[]{0x7A2FE4C,3,4}}:new[]{new uint[]{0x7a2bebc,3,2},new uint[]{0x7a2bec0,2,3},new uint[]{0x7a2bed8,4,3},new uint[]{0x7a2bedc,3,4}};
 }
 internal static bool Equal(uint[][] a,uint[][] b){return a.Length==2&&b.Length==2&&a[0].SequenceEqual(b[0])&&a[1].SequenceEqual(b[1]);}
 internal static bool Owned(uint[][] current){
  if(current.Length!=2||current.Any(r=>r.Length!=6))return false;
  for(int t=0;t<2;t++)for(int i=0;i<6;i++)if(current[t][i]!=Original[t][i]&&current[t][i]!=Enabled[t][i])return false;
  return true;
 }
 internal static bool Same(CategoryJournal j,int pid,long created,ulong basis,string image){return j!=null&&j.schema==1&&j.kind=="wnba-category"&&j.pid==pid&&j.created==created&&j.basis==basis&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);}
 internal static void Validate(CategoryJournal j){FreestylePolicy.Require(j.schema==1&&j.kind=="wnba-category"&&j.pid>0&&j.created>0&&j.basis>=0x10000&&new[]{"prepared","protecting","writing","applied","restored","failed"}.Contains(j.phase),"Invalid category session record.");}
}
internal sealed class CategoryJournal {
 public int schema=1,pid,writes;public long created;public ulong basis;public string kind="wnba-category",image,phase;
}
