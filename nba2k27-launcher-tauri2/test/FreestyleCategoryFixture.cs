using System;
using System.Linq;
using System.Collections.Generic;
using System.Web.Script.Serialization;
// Pure policy oracle: compile the two original policy sources unchanged. There
// are no process, game-launch, registry, filesystem-patch or native APIs here.
internal static class FreestyleCategoryFixture {
 static void Main() {
  var mixtures=new List<object>();
  for(int bits=0;bits<16;bits++){
   var rows=CategoryPolicy.Original.Select(r=>r.ToArray()).ToArray();
   var cells=new[]{new[]{0,3},new[]{0,4},new[]{1,2},new[]{1,3}};
   for(int i=0;i<4;i++)if((bits&(1<<i))!=0)rows[cells[i][0]][cells[i][1]]=CategoryPolicy.Enabled[cells[i][0]][cells[i][1]];
   mixtures.Add(new{bits,owned=CategoryPolicy.Owned(rows),original=CategoryPolicy.Equal(rows,CategoryPolicy.Original),enabled=CategoryPolicy.Equal(rows,CategoryPolicy.Enabled)});
  }
  var foreign=new List<object>();
  for(int t=0;t<2;t++)for(int i=0;i<6;i++)foreach(uint value in new uint[]{6,99,uint.MaxValue}){
   var rows=CategoryPolicy.Original.Select(r=>r.ToArray()).ToArray();rows[t][i]=value;
   foreign.Add(new{table=t,index=i,value,owned=CategoryPolicy.Owned(rows)});
  }
  Console.WriteLine(new JavaScriptSerializer().Serialize(new{schema="nba2k27-freestyle-category-oracle-v1",gameAccess=false,sourceUnmodified=true,original=CategoryPolicy.Original,enabled=CategoryPolicy.Enabled,mixtures,foreign}));
 }
}
