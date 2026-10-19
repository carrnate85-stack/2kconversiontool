// Offline reflection tests against the actual compiled helper; no game process access.
using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Collections.Generic;
using System.Web.Script.Serialization;
using NBA2K27RosterResearch;
class StreetwearCompositionTests {
    static void Check(bool condition,string message) {if(!condition)throw new Exception(message);}
    static int Main(string[] args) {
        try {
            var catalog=new JavaScriptSerializer {MaxJsonLength=64000000}.Deserialize<StreetCatalog>(File.ReadAllText(args[0]));
            var type=typeof(StreetCatalog).Assembly.GetType("NBA2K27RosterResearch.BlacktopTool");
            var flags=BindingFlags.NonPublic|BindingFlags.Static;
            type.GetField("street",flags).SetValue(null,catalog.presets);
            type.GetField("additionalShoes",flags).SetValue(null,catalog.additionalShoes??new ExtraShoe[0]);
            var accessories=catalog.accessoryTest.added.Concat(catalog.additionalAccessories??new AccessoryEntry[0]).Select(a=>a.asset).ToArray();
            type.GetField("accessoryAssets",flags).SetValue(null,accessories);
            var compose=type.GetMethod("StreetItems",flags);var verify=type.GetMethod("ValidPrivateItems",flags);
            int checks=0;
            int bareIndex=Array.FindIndex(catalog.presets,p=>p.id=="shirtless-male");
            Check(bareIndex>=0,"Missing shirtless QA preset");
            var bare=(string[])compose.Invoke(null,new object[]{bareIndex,catalog.presets[bareIndex].bottom,null,null,null});
            Check(bare.SequenceEqual(catalog.presets[bareIndex].original)&&bare.Contains("NO SHIRT"),"Stock shirtless marker lost");checks++;
            var multi=type.GetMethod("StreetItemsWithAccessories",flags);
            int baseIndex=Array.FindIndex(catalog.presets,p=>p.id=="g1-o0");var basePreset=catalog.presets[baseIndex];
            string head=accessories.First(a=>a.StartsWith("headband_")),bag=accessories.First(a=>a.StartsWith("backpack_")),arm="cyborg_arm_solid_slv_left";
            var full=(string[])multi.Invoke(null,new object[]{baseIndex,basePreset.bottom,null,null,null,null,new[]{arm,bag,head}});
            Check(full.SequenceEqual(basePreset.original.Concat(new[]{head,bag,arm})),"Multi accessory ordering/base mismatch");checks++;
            Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=baseIndex},full,new List<OutfitOption>()}),"Multi accessory rescan failed");checks++;
            foreach(var invalid in new[]{new[]{arm,arm},new[]{"glasses_3d_wht","goggles_current_solid_blk"},new[]{head,bag,arm,"cyborg_arm_solid_slv_right"},new[]{"unknown"}}){
                bool rejected=false;try{multi.Invoke(null,new object[]{baseIndex,basePreset.bottom,null,null,null,null,invalid});}catch(TargetInvocationException){rejected=true;}
                Check(rejected,"Invalid multiple accessories accepted");checks++;
            }
            var withAccessory=type.GetMethod("StreetItemsWithAccessory",flags);
            foreach(var candidate in catalog.presets.Select((p,i)=>new {p,i}).Where(v=>v.p.sourceGroup.pool=="male"&&!v.p.id.StartsWith("accessory-test-")&&v.p.original.Length<=6&&v.p.shoes.Length==1).GroupBy(v=>v.p.original.Length).Select(g=>g.First())) {
                foreach(var accessory in accessories.Where(a=>!a.StartsWith("female_")).Concat(new[]{""})) {
                    var result=(string[])withAccessory.Invoke(null,new object[]{candidate.i,candidate.p.bottom,null,null,null,accessory});
                    Check(result.SequenceEqual(candidate.p.original.Concat(accessory==""?new string[0]:new[]{accessory})),"Accessory damaged base outfit");checks++;
                    Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=candidate.i},result,new List<OutfitOption>()}),"Accessory rescan rejected");checks++;
                }
                foreach(var bad in new[]{"unknown_accessory","../headband","female_cyborg_arm_solid_blk_left"}) {
                    bool rejected=false;try{withAccessory.Invoke(null,new object[]{candidate.i,candidate.p.bottom,null,null,null,bad});}catch(TargetInvocationException){rejected=true;}
                    Check(rejected,"Invalid accessory accepted");checks++;
                }
            }
            for(int i=0;i<catalog.presets.Length;i++) {
                var p=catalog.presets[i];
                var original=(string[])compose.Invoke(null,new object[]{i,p.bottom,null,null,null});
                Check(original.SequenceEqual(p.original),"Legacy composition changed");checks++;
                if(p.shoes==null||p.shoes.Length!=1)continue;
                var donor=catalog.presets.FirstOrDefault(s=>s.sourceGroup.pool==p.sourceGroup.pool&&s.shoes!=null&&s.shoes.Length==1&&s.shoes[0]!=p.shoes[0]);
                if(donor==null)continue;
                var result=(string[])compose.Invoke(null,new object[]{i,p.bottom,donor.shoes[0],null,null});
                var expected=(string[])p.original.Clone();expected[Array.IndexOf(expected,p.shoes[0])]=donor.shoes[0];
                Check(result.SequenceEqual(expected),"Non-shoe component changed");checks++;
                Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=i},result,new List<OutfitOption>()}),"Rescan rejected custom shoes");checks++;
                var sock=catalog.presets.Where(s=>s.sourceGroup.pool==p.sourceGroup.pool).SelectMany(s=>s.accessories??new string[0]).First(s=>s.Contains("acc_sock_"));
                var withSocks=(string[])compose.Invoke(null,new object[]{i,p.bottom,donor.shoes[0],sock,null});
                var expectedSocks=expected.ToList();int sockIndex=expectedSocks.FindIndex(s=>s.Contains("acc_sock_"));
                if(sockIndex>=0)expectedSocks[sockIndex]=sock;else expectedSocks.Add(sock);
                Check(withSocks.SequenceEqual(expectedSocks),"Sock substitution damaged another piece");checks++;
                Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=i},withSocks,new List<OutfitOption>()}),"Rescan rejected custom socks");checks++;
                var alternatives=catalog.presets.Where(s=>s.top==p.top&&s.sourceGroup.pool==p.sourceGroup.pool&&s.layers.Length<=1).Select(s=>s.layers.Length==0?"none":s.layers[0]).Distinct();
                foreach(var layer in alternatives) {
                    var layered=(string[])compose.Invoke(null,new object[]{i,p.bottom,donor.shoes[0],sock,layer});
                    var state=new BlacktopPlayer {streetIndex=i};
                    Check((bool)verify.Invoke(null,new object[]{state,layered,new List<OutfitOption>()}),"Layer rescan failed");checks++;
                    Check(layered.Contains(p.top)&&layered.Contains(p.bottom)&&layered.Contains(sock)&&layered.Contains(donor.shoes[0]),"Layer lost independent pieces");checks++;
                    if(layer=="none")Check(!p.layers.Any(layered.Contains),"Removed layer remained");else Check(layered.Contains(layer),"Chosen layer missing");checks++;
                }
            }
            int male=Array.FindIndex(catalog.presets,p=>p.id=="g2-o0");
            foreach(var extra in catalog.additionalShoes??new ExtraShoe[0]) {
                var result=(string[])compose.Invoke(null,new object[]{male,catalog.presets[male].bottom,extra.asset,null,null});
                Check(result.Last()==extra.asset,"Additional shoe not composed");checks++;
                Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=male},result,new List<OutfitOption>()}),"Additional shoe rescan rejected");checks++;
            }
            foreach(var invalid in new[]{"unknown_shoes","../shoe",catalog.presets.First(p=>p.shoes!=null&&p.shoes.Any(s=>s.StartsWith("female_"))).shoes[0]}) {
                bool rejected=false;try {compose.Invoke(null,new object[]{male,catalog.presets[male].bottom,invalid,null,null});}catch(TargetInvocationException){rejected=true;}
                Check(rejected,"Invalid shoe accepted");checks++;
            }
            foreach(var invalid in new[]{"unknown_sock","../sock","female_acc_sock_solid_wht_crew"}) {
                bool rejected=false;try {compose.Invoke(null,new object[]{male,catalog.presets[male].bottom,null,invalid,null});}catch(TargetInvocationException){rejected=true;}
                Check(rejected,"Invalid sock accepted");checks++;
            }
            foreach(var invalid in new[]{"none","tshirt_ss_solid_teal_muted","../unknown"}) {
                bool rejected=false;try {compose.Invoke(null,new object[]{male,catalog.presets[male].bottom,null,null,invalid});}catch(TargetInvocationException){rejected=true;}
                Check(rejected,"Unobserved layer accepted");checks++;
            }
            int woman=Array.FindIndex(catalog.presets,p=>p.sourceGroup.pool=="female"&&p.original.Length==3);
            string femaleArm=accessories.First(a=>a.StartsWith("female_acc_armsleeve_")&&a.EndsWith("_left"));
            var femaleFit=(string[])multi.Invoke(null,new object[]{woman,catalog.presets[woman].bottom,"roster",null,null,null,new[]{femaleArm}});
            Check(femaleFit.Contains(femaleArm),"Female accessory was not composed");checks++;
            Check((bool)verify.Invoke(null,new object[]{new BlacktopPlayer {streetIndex=woman,rosterShoes=true},femaleFit,new List<OutfitOption>()}),"Female accessory rescan rejected");checks++;
            bool crossRejected=false;try{multi.Invoke(null,new object[]{male,catalog.presets[male].bottom,"roster",null,null,null,new[]{femaleArm}});}catch(TargetInvocationException){crossRejected=true;}
            Check(crossRejected,"Male accepted female accessory");checks++;
            Console.WriteLine("PASS: "+checks+" native composition checks; game access 0.");return 0;
        } catch(Exception e) {Console.Error.WriteLine(e);return 1;}
    }
}
