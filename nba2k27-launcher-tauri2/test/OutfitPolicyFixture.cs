using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text;
using System.Web.Script.Serialization;

public sealed class OutfitPolicyFixture {
 public static int Main(string[] args) {
  var json=new JavaScriptSerializer{MaxJsonLength=16000000};
  var groups=json.Deserialize<OutfitGroup[]>(File.ReadAllText(args[0]));
  int records=0,valid=0,rejected=0;
  foreach(var group in groups)foreach(var r in group.records) {
   long basis=0x200000000L;
   OriginalPolicy.memory=new FixtureMemory(basis,group.length);
   Action<long,ulong> put=(at,n)=>OriginalPolicy.memory.Put(at,BitConverter.GetBytes(n));
   put(basis+16,(ulong)group.count);put(basis+group.descriptor,UInt64.Parse(group.hash));put(basis+group.descriptor+8,(ulong)group.outfitCount);
   put(basis+r.record,UInt64.Parse(r.marker));put(basis+r.record+8,2);put(basis+r.record+16,(ulong)(basis+r.array));
   for(int i=0;i<2;i++){put(basis+r.array+i*8,(ulong)(basis+r.strings[i]));OriginalPolicy.memory.Put(basis+r.strings[i],Encoding.Unicode.GetBytes(r.names[i]+"\0"));}
   records++;
   if(OriginalPolicy.ValidateLayout(basis,group,r))valid++;else throw new Exception("Original rejected fixture layout: "+group.entry);
   put(basis+r.record,UInt64.Parse(r.marker)^1UL);
   if(!OriginalPolicy.ValidateLayout(basis,group,r))rejected++;else throw new Exception("Original accepted changed marker");
  }
  var inputs=new[]{new[]{"tshirt_ss_white","pants_black"},new[]{"female_tshirt_ss_white","female_pants_black"},new[]{"female_tshirt_ss_white","pants_black"},new[]{"shoe_0000_allwhite","jersey_basketball_white"},new[]{"basketball_shorts_black","jersey_basketball_white"},new[]{"female_romper_test"},new[]{"NO SHIRT","acc_sock_white"},new[]{"hat_blue","female_jacket_red"}};
  var pools=inputs.Select(names=>new {names=names,pool=OriginalPolicy.ClothingPool(new BlacktopPlayer{outfits=names.Select(n=>new OutfitOption{id="stock",items=new[]{n}}).ToList()})}).ToArray();
  var started=new DateTime(2026,10,16,8,30,0,DateTimeKind.Utc);
  var player=new BlacktopPlayer{description=0x300000800L,model=0x300000000L,roster=0x400000000L,sourcePointer=0x500000000L};
  var pages=new List<object>();
  for(int count=2;count<=7;count++)foreach(bool rosterShoes in new[]{false,true}) {
   var names=Enumerable.Range(0,count).Select(i=>i==0&&rosterShoes?"NO SHIRT":"clothing_item_"+i).ToArray();
   var bytes=OriginalPolicy.Page(0x600000000L,311,started.Ticks,player,names,0,rosterShoes);
   pages.Add(new {items=names,rosterShoes=rosterShoes,allocation=0x600000000L,pid=311,started=started.ToFileTimeUtc().ToString(),controller=0x200000000L,slot=0,description=player.description,model=player.model,roster=player.roster,sourcePointer=player.sourcePointer,index=0,bytes=BitConverter.ToString(bytes).Replace("-","")});
  }
  Console.WriteLine(json.Serialize(new {schema="nba2k27-outfit-policy-oracle-v1",groups=groups.Length,records=records,validLayouts=valid,changedMarkersRejected=rejected,pools=pools,pages=pages,gameAccess=false}));
  return 0;
 }
}
public sealed class FixtureMemory {
 readonly long start,end;readonly Dictionary<long,byte> bytes=new Dictionary<long,byte>();
 public FixtureMemory(long at,long size){start=at;end=at+size;}
 public void Put(long at,byte[] data){if(at<start||at+data.Length>end)throw new Exception("Fixture range");for(int i=0;i<data.Length;i++)bytes[at+i]=data[i];}
 public byte[] Read(long at,int size){if(at<start||size<1||at+size>end)throw new InvalidOperationException("Unknown owned fixture address");var result=new byte[size];for(int i=0;i<size;i++){byte b;if(bytes.TryGetValue(at+i,out b))result[i]=b;}return result;}
}
