using System;
using System.Collections.Generic;
using System.Linq;
internal static class CategoryPolicyTests {
 static void Check(bool value,string message){if(!value)throw new Exception(message);}
 public static int Main(){
  var old=CategoryPolicy.Original;var enabled=CategoryPolicy.Enabled;
  foreach(var direction in enabled){
   Check(direction[5]==5,"Combined fallback changed");
   for(uint start=0;start<5;start++){var visited=new HashSet<uint>();uint current=start;for(int n=0;n<5;n++){visited.Add(current);current=direction[current];}Check(current==start&&visited.SetEquals(new uint[]{0,1,2,3,4}),"Category cycle lost a page");}
  }
  for(int i=0;i<6;i++)Check(enabled[0][enabled[1][i]]==i&&enabled[1][enabled[0][i]]==i,"Category directions disagree");
  for(int subset=0;subset<16;subset++){
   var partial=old.Select(row=>row.ToArray()).ToArray();
   for(int n=0;n<4;n++)if((subset&(1<<n))!=0){var change=CategoryPolicy.Changes[n];int table=change[0]<0x7a2bed0?0:1;int at=(int)(change[0]-(table==0?0x7a2beb0U:0x7a2bed0U))/4;Check(partial[table][at]==change[1],"Wrong original entry");partial[table][at]=change[2];}
   Check(CategoryPolicy.Owned(partial),"Interrupted owned state cannot be restored");
   Check(CategoryPolicy.Equal(partial,old)==(subset==0),"False original state");Check(CategoryPolicy.Equal(partial,enabled)==(subset==15),"False enabled state");
  }
  for(int table=0;table<2;table++)for(int index=0;index<6;index++){var invalid=old.Select(row=>row.ToArray()).ToArray();invalid[table][index]=99;Check(!CategoryPolicy.Owned(invalid),"Foreign entry accepted");}
  var j=new CategoryJournal{pid=12,created=34,basis=0x100000,image="game",phase="applied"};CategoryPolicy.Validate(j);Check(CategoryPolicy.Same(j,12,34,0x100000,"GAME"),"Owned session rejected");Check(!CategoryPolicy.Same(j,12,35,0x100000,"game"),"PID reuse accepted");Check(!CategoryPolicy.Same(j,12,34,0x100001,"game"),"Image change accepted");Check(!CategoryPolicy.Same(j,12,34,0x100000,"other"),"Foreign image accepted");
  Check(CategoryPolicy.Changes.All(c=>c[0]>=CategoryPolicy.Page&&c[0]+4<=CategoryPolicy.Page+4096),"Write crosses the only allowed static page");
  CategoryPolicy.SelectBuild(true);
  Check(CategoryPolicy.Page==0x7A2F000,"Updated protection page incorrect");
  Check(CategoryPolicy.Changes.Select(c=>c[0]).SequenceEqual(new uint[]{0x7A2FE2C,0x7A2FE30,0x7A2FE48,0x7A2FE4C}),"Updated write allowlist incorrect");
  Check(CategoryPolicy.Changes.All(c=>c[0]>=CategoryPolicy.Page&&c[0]+4<=CategoryPolicy.Page+4096),"Updated write crosses page");
  CategoryPolicy.SelectBuild(false);Check(CategoryPolicy.Page==0x7A2B000&&CategoryPolicy.Changes[0][0]==0x7A2BEBC,"Original build profile not restored");
  Console.WriteLine("PASS: native five-category cycle, both directions, all 16 partial-write states, foreign-data refusal and exact ownership; both build address profiles");return 0;
 }
}
