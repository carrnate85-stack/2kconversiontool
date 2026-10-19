using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading;
using System.Text.RegularExpressions;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;

namespace NBA2K27RosterResearch {
    public sealed class OutfitLayout {
        public int record, array; public string marker; public int[] strings; public string[] names;
    }
    public sealed class OutfitGroup {
        public string entry, hash; public int length, count, descriptor, outfitCount; public OutfitLayout[] records;
    }
    public sealed class OutfitOption { public string id, label; public string[] items; public long pointer; }
    public sealed class UniformContext { public long pointer; public string name; }
    public sealed class StreetGroup { public string pool; }
    public sealed class StreetPreset { public string id,top,bottom; public string[] original,shoes,accessories,layers; public bool separable; public StreetGroup sourceGroup; }
    public sealed class ExtraShoe { public string asset; public string[] pools; }
    public sealed class AccessoryEntry { public string asset; }
    public sealed class AccessoryCatalog { public AccessoryEntry[] added; }
    public sealed class StreetCatalog { public StreetPreset[] presets; public ExtraShoe[] additionalShoes; public AccessoryCatalog accessoryTest; public AccessoryEntry[] additionalAccessories; }
    public sealed class BlacktopPlayer {
        public int teamId=-1; public string teamName;
        public int slot, cyberface, count, streetIndex=-1; public string name, groupKey, uniformName,layer; public long description, model, roster, pointer, sourcePointer, uniform;
        public bool uniformVerified,rosterShoes; public List<UniformContext> uniformOptions = new List<UniformContext>();
        public string error; public List<OutfitOption> outfits = new List<OutfitOption>();
    }
    public sealed class BlacktopSnapshot {
        public int pid; public string started; public long controller; public List<BlacktopPlayer> players = new List<BlacktopPlayer>();
    }
    public sealed class BlacktopRequest {
        public int pid, slot, uniformProtocol, expectedCount, targetCount; public string started, top, bottom, streetId, shoe,sock,layer,accessory; public string[] items,accessories; public long controller, description, model, roster, expected, target, expectedUniform, targetUniform;
    }
    internal static class BlacktopTool {
        [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
        [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
        [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
        [DllImport("user32.dll")] static extern bool IsIconic(IntPtr window);
        [DllImport("user32.dll")] static extern bool ShowWindowAsync(IntPtr window,int command);
        [DllImport("kernel32.dll",SetLastError=true)] static extern IntPtr VirtualAllocEx(IntPtr process,IntPtr address,UIntPtr size,uint allocation,uint protection);
        [DllImport("kernel32.dll",SetLastError=true)] static extern bool VirtualProtectEx(IntPtr process,IntPtr address,UIntPtr size,uint protection,out uint previous);
        static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 64000000 };
        static Reader memory; static long image, root; static OutfitGroup[] groups;
        static StreetPreset[] street=new StreetPreset[0];
        static ExtraShoe[] additionalShoes=new ExtraShoe[0];
        static string[] accessoryAssets=new string[0];
        static int CatalogCount(BinaryReader r,int max) {int n=r.ReadInt32();Need(n>=0&&n<=max,"Invalid native wardrobe count.");return n;}
        static string CatalogString(BinaryReader r) {int n=r.ReadInt32();if(n==-1)return null;Need(n>=0&&n<=1024,"Invalid native wardrobe string.");var b=r.ReadBytes(n);Need(b.Length==n,"Incomplete native wardrobe.");return Encoding.UTF8.GetString(b);}
        static string[] CatalogArray(BinaryReader r) {int n=r.ReadInt32();if(n==-1)return null;Need(n>=0&&n<=100000,"Invalid native wardrobe list.");var a=new string[n];for(int i=0;i<n;i++)a[i]=CatalogString(r);return a;}
        static void LoadNativeCatalog(string path) {
            using(var r=new BinaryReader(File.OpenRead(path))) {
                Need(r.ReadInt32()==0x31435742,"Unsupported native wardrobe.");
                street=new StreetPreset[CatalogCount(r,100000)];
                for(int i=0;i<street.Length;i++)street[i]=new StreetPreset {
                    id=CatalogString(r),top=CatalogString(r),bottom=CatalogString(r),
                    original=CatalogArray(r),shoes=CatalogArray(r),accessories=CatalogArray(r),layers=CatalogArray(r),
                    separable=CatalogCount(r,1)==1,sourceGroup=new StreetGroup{pool=CatalogString(r)}
                };
                additionalShoes=new ExtraShoe[CatalogCount(r,100000)];
                for(int i=0;i<additionalShoes.Length;i++)additionalShoes[i]=new ExtraShoe{asset=CatalogString(r),pools=CatalogArray(r)};
                accessoryAssets=CatalogArray(r)??new string[0];
                Need(r.BaseStream.Position==r.BaseStream.Length,"Unexpected native wardrobe data.");
            }
        }
        static int gamePid;static long gameStarted;
        const long PrivateMagic=0x31464F4C43373243L; // C27CLOF1; data, never executable.
        const long StreetMagic=0x32464F4C43373243L; // C27CLOF2, variable clothing count.
        static bool IsSock(string name) {return name!=null && Regex.IsMatch(name,@"^(female_)?acc_sock_");}
        static string[] LayerChoices(StreetPreset p) {
            return street.Where(s=>s.top==p.top && s.sourceGroup.pool==p.sourceGroup.pool && s.layers!=null && s.layers.Length<=1).Select(s=>s.layers.Length==0?"none":s.layers[0]).Distinct().ToArray();
        }
        static string[] LayerItems(int index,string layer) {
            Need(index>=0 && index<street.Length,"Unknown streetwear style.");
            var p=street[index];var result=p.original.ToList();var layers=p.layers??new string[0];
            if(String.IsNullOrEmpty(layer))return result.ToArray();
            Need(layers.Length<=1 && LayerChoices(p).Contains(layer),"This layer has not been observed with the selected top.");
            if(layer==(layers.Length==0?"none":layers[0]))return result.ToArray();
            if(layers.Length==1) {
                int position=result.IndexOf(layers[0]);Need(position>=0,"Layer is not mapped.");
                if(layer=="none")result.RemoveAt(position);else result[position]=layer;
            }else {
                var donor=street.First(s=>s.top==p.top && s.sourceGroup.pool==p.sourceGroup.pool && s.layers!=null && s.layers.Length==1 && s.layers[0]==layer);
                int position=Array.IndexOf(donor.original,layer);bool afterTop=position==Array.IndexOf(donor.original,donor.top)+1;
                Need(afterTop || position==donor.original.Length-1,"Layer placement needs review.");
                result.Insert(afterTop?result.IndexOf(p.top)+1:result.Count,layer);
            }
            return result.ToArray();
        }
        static string[] StreetItems(int index,string bottom,string shoe=null,string sock=null,string layer=null) {
            Need(index>=0 && index<street.Length,"Unknown streetwear style.");
            var preset=street[index];var result=LayerItems(index,layer);
            if(shoe=="roster"){
                Need(preset.shoes!=null&&preset.shoes.Length==1&&result.Contains(preset.shoes[0]),"Roster shoe fallback requires one mapped shoe.");
                result=result.Where(s=>s!=preset.shoes[0]).ToArray();
            }else if(!String.IsNullOrEmpty(shoe)) {
                Need(preset.shoes!=null && preset.shoes.Length==1 && (street.Any(s=>s.sourceGroup.pool==preset.sourceGroup.pool && s.shoes!=null && s.shoes.Contains(shoe)) || additionalShoes.Any(s=>s.asset==shoe && s.pools!=null && s.pools.Contains(preset.sourceGroup.pool))),"Choose shoes from the same clothing collection.");
                int position=Array.IndexOf(result,preset.shoes[0]);Need(position>=0,"Streetwear shoes are not mapped.");result[position]=shoe;
            }
            if(!String.IsNullOrEmpty(sock)) {
                var socks=(preset.accessories??new string[0]).Where(IsSock).ToArray();
                Need(socks.Length<=1 && IsSock(sock) && street.Any(s=>s.sourceGroup.pool==preset.sourceGroup.pool && s.accessories!=null && s.accessories.Contains(sock)),"Choose socks from the same clothing collection.");
                if(socks.Length==1) {int position=Array.IndexOf(result,socks[0]);Need(position>=0,"Socks are not mapped.");result[position]=sock;}
                else {Need(result.Length<7,"This style has no room for extra socks.");result=result.Concat(new[]{sock}).ToArray();}
            }
            if(bottom!=null && bottom!=preset.bottom) {
                Need(preset.separable && street.Any(s=>s.separable && s.sourceGroup.pool==preset.sourceGroup.pool && s.bottom==bottom),"Choose a bottom from the same streetwear collection.");
                int position=Array.IndexOf(result,preset.bottom);Need(position>=0,"Streetwear bottom is not mapped.");result[position]=bottom;
            }
            Need(result.Length>=2 && result.Length<=7 && result.All(n=>n=="NO SHIRT" || (Regex.IsMatch(n,@"\A[a-zA-Z0-9_]+\z") && n.Length<256)),"Streetwear list is not supported.");
            return result;
        }
        static string[] StreetItemsWithAccessory(int index,string bottom,string shoe,string sock,string layer,string accessory) {
            var items=StreetItems(index,bottom,shoe,sock,layer);
            if(accessory==null)return items;
            Need((street[index].sourceGroup.pool=="male" || street[index].sourceGroup.pool=="female") && (accessory=="" || (accessoryAssets.Contains(accessory) && (accessory.StartsWith("female_")?"female":"male")==street[index].sourceGroup.pool)),"Choose an accessory from the same clothing collection.");
            var result=items.Where(s=>!accessoryAssets.Contains(s)).ToList();
            if(accessory!="")result.Add(accessory);
            Need(result.Count>=2 && result.Count<=7,"This outfit has no room for an accessory; required layers are preserved.");
            return result.ToArray();
        }
        static int AccessorySlot(string asset) {
            if(asset==null)return -1;if(asset.StartsWith("female_"))asset=asset.Substring(7);
            if(Regex.IsMatch(asset,@"^(facemask_|mask_|hat_hockey_|hat_knight_)"))return 4;
            if(asset.StartsWith("headband_"))return 2;
            if(Regex.IsMatch(asset,@"^(glasses_|sunglasses_|goggles_)"))return 3;
            if(asset.StartsWith("hat_"))return 0;
            if(asset.StartsWith("headphone_"))return 1;
            if(Regex.IsMatch(asset,@"^(necklace_|scarf_|tie_|bowtie_|towel_neck_)"))return 7;
            if(asset.StartsWith("bag_shoulder_"))return 8;
            if(Regex.IsMatch(asset,@"^(belt_|innertube_)"))return 9;
            if(Regex.IsMatch(asset,@"^(backpack_|jetpack_|wings_|cape_)"))return 10;
            if(Regex.IsMatch(asset,@"^nailpolish_(left|right)_"))return asset.StartsWith("nailpolish_left_")?15:16;
            bool left=asset.EndsWith("_left"),right=asset.EndsWith("_right");if(!left&&!right)return -1;
            if(asset.StartsWith("piercing_"))return left?5:6;
            if(Regex.IsMatch(asset,@"^(bracelet_|watch_)"))return left?13:14;
            if(Regex.IsMatch(asset,@"^(glove_|ring_|acc_fingertape_)"))return left?15:16;
            if(Regex.IsMatch(asset,@"^(cyborg_arm_|acc_(armsleeve|elbowband|elbowpad|elbowsleeve|wristband|wristtape|rubber|kinesiotape)_|arm_band|floaties_arm_)"))return left?11:12;
            if(Regex.IsMatch(asset,@"^(cyborg_leg_|acc_(kneebrace|kneepad|kneesleeve|kneestrap|legsleeve|anklebrace)_)"))return left?17:18;
            return -1;
        }
        static string[] StreetItemsWithAccessories(int index,string bottom,string shoe,string sock,string layer,string accessory,string[] accessories) {
            if(accessories==null)return StreetItemsWithAccessory(index,bottom,shoe,sock,layer,accessory);
            Need(accessories.Length<=7 && accessories.All(a=>a!=null && accessoryAssets.Contains(a) && AccessorySlot(a)>=0 && (a.StartsWith("female_")?"female":"male")==street[index].sourceGroup.pool),"Unknown or incompatible accessory.");
            Need(accessories.Select(AccessorySlot).Distinct().Count()==accessories.Length,"Choose only one accessory per slot; glasses and goggles share eyewear.");
            var items=StreetItemsWithAccessory(index,bottom,shoe,sock,layer,"");
            Need(items.Length+accessories.Length<=7,"Seven clothing pieces maximum; remove an accessory first.");
            return items.Concat(accessories.OrderBy(AccessorySlot)).ToArray();
        }
        static string ClothingPool(BlacktopPlayer player) {
            var names=player.outfits.Where(o=>o.id!="custom").SelectMany(o=>o.items).Where(n=>Regex.IsMatch(n,@"^(female_)?(tshirt_|hoodie_|jacket_|jogger_|jeans_|warmup_|pants_|shorts_|basketball_shorts_|canvas_shorts_|sweats_|cargo_|tanktop_|shirt_)")).ToArray();
            if(names.Length==0) return null;
            if(names.All(n=>n.StartsWith("female_",StringComparison.Ordinal))) return "female";
            if(names.All(n=>!n.StartsWith("female_",StringComparison.Ordinal))) return "male";
            return null;
        }
        static long Q(long a) { return BitConverter.ToInt64(memory.Read(a,8),0); }
        static int I(long a) { return BitConverter.ToInt32(memory.Read(a,4),0); }
        static string Text(long a, int bytes) { return Encoding.Unicode.GetString(memory.Read(a,bytes)).Split('\0')[0]; }
        static void Need(bool valid, string error) { if (!valid) throw new InvalidOperationException(error); }
        static bool RequiresUniform(OutfitOption option) { return option.items.Any(n=>n.StartsWith("UNIFORM",StringComparison.Ordinal)); }
        static string UniformName(long pointer) {
            if(pointer<=0) return null;
            try {
                byte[] data=memory.Read(pointer+8,64); int end=Array.IndexOf(data,(byte)0);
                if(end<2) return null;
                string name=Encoding.ASCII.GetString(data,0,end);
                return Regex.IsMatch(name,@"\Au[a-zA-Z0-9_]+\z") ? name : null;
            } catch(InvalidOperationException) { return null; }
        }
        static long BlacktopRootRva=0xAEB29D0;
        static long BlacktopVtable=0x78AABA8;
        static long PreviewModelVtable=0x6775FA8;
        static long PreviewDescriptionVtable=0x682A858;
        static long RosterRootRva=0x9a43eb8;
        static long LineupRva=0x9bbb0a0;
        static long RefreshRva=0x3ACA20;
        static void SelectBuild(bool updated) {
            BlacktopRootRva=updated?0xAEB69D0:0xAEB29D0;
            BlacktopVtable=updated?0x78AEA48:0x78AABA8;
            PreviewModelVtable=updated?0x6778FC8:0x6775FA8;
            PreviewDescriptionVtable=updated?0x682D858:0x682A858;
            RosterRootRva=updated?0x9A47EB8:0x9a43eb8;
            LineupRva=updated?0x9BBF0A0:0x9bbb0a0;
            RefreshRva=updated?0x3ACB20:0x3ACA20;
        }
        static void CheckGame(Process process, string gameRoot, bool requireSelection=true) {
            gamePid=process.Id;gameStarted=process.StartTime.ToUniversalTime().Ticks;
            Need(String.Equals(Path.GetFullPath(process.MainModule.FileName), Path.Combine(Path.GetFullPath(gameRoot),"NBA2K27.exe"), StringComparison.OrdinalIgnoreCase), "Game path does not match launcher settings.");
            foreach(Process p in Process.GetProcesses()) {
                using(p) { Need(p.ProcessName.IndexOf("easyanticheat",StringComparison.OrdinalIgnoreCase)<0, "Close the anti-cheat session; use the supported offline launch."); }
            }
            foreach(ProcessModule module in process.Modules) {
                Need(module.ModuleName.IndexOf("easyanticheat",StringComparison.OrdinalIgnoreCase)<0, "Anti-cheat module detected; edits disabled.");
            }
            image=process.MainModule.BaseAddress.ToInt64();
            Need(I(image)==0x00905A4D || BitConverter.ToUInt16(memory.Read(image,2),0)==0x5A4D,"Invalid executable header.");
            long pe=image+I(image+0x3c);
            bool updated=I(pe+8)==unchecked((int)0x6AA2013D) && I(pe+0x50)==0x36558000;
            Need(I(pe)==0x4550 && (updated || (I(pe+8)==unchecked((int)0x6A974D13) && I(pe+0x50)==0x368C3000)),"Unsupported game build; outfit controls disabled.");
            SelectBuild(updated); root=image+BlacktopRootRva;
            Need(memory.Read(image+RefreshRva,20).SequenceEqual(new byte[]{0x83,0xfa,0x1e,0x77,0x0e,0x89,0xd0,0x48,0x69,0xc0,0xb0,7,0,0,0xc6,0x44,1,0x30,1,0xc3}),"Native refresh signature changed.");
            if(requireSelection) Need(Q(root)==image+BlacktopVtable && I(root+0xea8)==3,"Open the Blacktop player-selection screen first.");
        }
        static bool PrivateList(BlacktopPlayer p,out string[] items) {
            items=null;long allocation=p.pointer-128;
            IntPtr handle=Native.OpenProcess(Native.QueryInformation|Native.VmRead,false,gamePid);
            if(handle==IntPtr.Zero) return false;
            try {
                Native.MemoryBasicInformation region;
                if(Native.VirtualQueryEx(handle,new IntPtr(allocation),out region,(UIntPtr)Marshal.SizeOf(typeof(Native.MemoryBasicInformation)))==UIntPtr.Zero
                    || region.AllocationBase.ToInt64()!=allocation || region.BaseAddress.ToInt64()!=allocation || region.RegionSize.ToUInt64()!=4096
                    || region.State!=0x1000 || region.Type!=0x20000 || region.Protect!=2) return false;
                long magic=Q(allocation), rawCount=Q(allocation+56);if(rawCount<2 || rawCount>7) return false;int count=(int)rawCount;
                if((magic!=PrivateMagic && magic!=StreetMagic) || Q(allocation+8)!=gamePid || Q(allocation+16)!=gameStarted
                    || Q(allocation+24)!=p.description || Q(allocation+32)!=p.model || Q(allocation+40)!=p.roster
                    || count<2 || count>7 || count!=p.count || (magic==PrivateMagic && count!=2)) return false;
                p.sourcePointer=Q(allocation+48);
                long index=magic==StreetMagic ? Q(allocation+64) : -1;if(index < -1 || index>=street.Length) return false;p.streetIndex=(int)index;
                long fallback=Q(allocation+72);if(fallback<0||fallback>1)return false;p.rosterShoes=fallback==1;
                items=new string[count];
                for(int i=0;i<count;i++) {
                    long address=allocation+256+i*(magic==PrivateMagic ? 768 : 512);
                    if(Q(p.pointer+i*8)!=address) return false;
                    items[i]=Text(address,512);
                }
                return items.All(n=>n.Length>0 && n.Length<256);
            } catch(InvalidOperationException) {return false;}
            finally {Native.CloseHandle(handle);}
        }
        static long CreatePrivateList(BlacktopPlayer p,string[] items,int streetIndex,bool rosterShoes=false) {
            Need(items.Length>=2 && items.Length<=7 && items.All(n=>n.Length>0 && n.Length<256),"Invalid composed clothing names.");
            IntPtr handle=Native.OpenProcess(Native.QueryInformation|Native.VmRead|Native.VmWrite|Native.VmOperation,false,gamePid);
            Need(handle!=IntPtr.Zero,"Cannot prepare private clothing data.");
            try {
                long allocation=VirtualAllocEx(handle,IntPtr.Zero,(UIntPtr)4096,0x3000,4).ToInt64();
                Need(allocation!=0,"Cannot allocate private clothing data.");
                var data=new byte[4096];
                long[] header={StreetMagic,gamePid,gameStarted,p.description,p.model,p.roster,p.sourcePointer,items.Length,streetIndex,rosterShoes?1:0};
                for(int i=0;i<header.Length;i++) Buffer.BlockCopy(BitConverter.GetBytes(header[i]),0,data,i*8,8);
                for(int i=0;i<items.Length;i++) {
                    int offset=256+i*512;var name=Encoding.Unicode.GetBytes(items[i]+"\0");
                    Buffer.BlockCopy(BitConverter.GetBytes(allocation+offset),0,data,128+i*8,8);
                    Buffer.BlockCopy(name,0,data,offset,name.Length);
                }
                memory.Write(allocation,data);
                Need(memory.Read(allocation,4096).SequenceEqual(data),"Private clothing data readback failed.");
                uint previous;Need(VirtualProtectEx(handle,new IntPtr(allocation),(UIntPtr)4096,2,out previous),"Cannot make clothing data read-only.");
                // Retain immutable pages until game exit: the game may retain string
                // pointers after a reload. Never free live buffers on app exit/Restore.
                return allocation+128;
            } finally {Native.CloseHandle(handle);}
        }
        static bool ValidateLayout(long basis, OutfitGroup group, OutfitLayout layout) {
            try {
                if(basis<=0 || Q(basis+16)!=group.count || unchecked((ulong)Q(basis+group.descriptor))!=UInt64.Parse(group.hash) || Q(basis+group.descriptor+8)!=group.outfitCount) return false;
                if(unchecked((ulong)Q(basis+layout.record))!=UInt64.Parse(layout.marker) || Q(basis+layout.record+8)!=2 || Q(basis+layout.record+16)!=basis+layout.array) return false;
                for(int j=0;j<2;j++) {
                    if(Q(basis+layout.array+j*8)!=basis+layout.strings[j] || Text(basis+layout.strings[j],(layout.names[j].Length+1)*2)!=layout.names[j]) return false;
                }
                return true;
            } catch(InvalidOperationException) { return false; }
        }
        static bool ValidPrivateItems(BlacktopPlayer p,string[] items,IEnumerable<OutfitOption> stock) {
            if(p.streetIndex>=0 && p.streetIndex<street.Length) {
                var preset=street[p.streetIndex];
                foreach(var layer in new string[]{null}.Concat(LayerChoices(preset))) {
                    try {
                        var template=LayerItems(p.streetIndex,layer).Where(s=>!p.rosterShoes||!(preset.shoes??new string[0]).Contains(s)).ToArray();int position=Array.IndexOf(template,preset.bottom);
                        if(position<0 || items.Length<template.Length || items.Length>template.Length+1)continue;
                        int shoePosition=preset.shoes!=null && preset.shoes.Length==1 ? Array.IndexOf(template,preset.shoes[0]) : -1;
                        int sockPosition=Array.FindIndex(template,IsSock);
                        string sock=sockPosition>=0?items[sockPosition]:items.Length==template.Length+1?items[items.Length-1]:null;
                        if(StreetItems(p.streetIndex,items[position],p.rosterShoes?"roster":shoePosition>=0?items[shoePosition]:null,sock,layer).SequenceEqual(items)) {p.layer=layer;return true;}
                    } catch(InvalidOperationException) { /* Try the next observed layer arrangement. */ }
                }
                foreach(var layer in new string[]{null}.Concat(LayerChoices(preset))) {
                    try {
                        var template=LayerItems(p.streetIndex,layer).Where(s=>!accessoryAssets.Contains(s)&&(!p.rosterShoes||!(preset.shoes??new string[0]).Contains(s))).ToArray();
                        var accessories=items.Where(a=>accessoryAssets.Contains(a)).ToArray();
                        int actualCount=items.Length-accessories.Length;
                        if(actualCount<template.Length || actualCount>template.Length+1)continue;
                        int position=Array.IndexOf(template,preset.bottom);
                        int shoePosition=preset.shoes!=null && preset.shoes.Length==1?Array.IndexOf(template,preset.shoes[0]):-1;
                        int sockPosition=Array.FindIndex(template,IsSock);
                        if(position<0)continue;
                        string sock=sockPosition>=0?items[sockPosition]:actualCount==template.Length+1?items[actualCount-1]:null;
                        if(StreetItemsWithAccessories(p.streetIndex,items[position],p.rosterShoes?"roster":shoePosition>=0?items[shoePosition]:null,sock,layer,null,accessories).SequenceEqual(items)){p.layer=layer;return true;}
                    }catch(InvalidOperationException){ /* Reject unknown or malformed accessory lists. */ }
                }
                return false;
            }
            return p.streetIndex==-1 && items.Length==2 && stock.Any(o=>o.items.Length==2 && o.items[0]==items[0]) && stock.Any(o=>o.items.Length==2 && o.items[1]==items[1]);
        }
        static OutfitOption HistoricalOption(BlacktopPlayer p,long pointer,int count) {
            var old=new BlacktopPlayer {description=p.description,model=p.model,roster=p.roster,pointer=pointer,count=count};string[] items;
            Need(PrivateList(old,out items) && p.outfits.Any(o=>o.id!="custom" && o.pointer==old.sourcePointer) && ValidPrivateItems(old,items,p.outfits.Where(o=>o.id!="custom")),"The saved clothing list is no longer valid for this player.");
            return new OutfitOption {id="saved-original",pointer=pointer,items=items,label="Saved original"};
        }
        static BlacktopPlayer ReadPlayer(long controller,int slot) {
            long entry=controller+0x20+slot*0x7b0, model=Q(entry), desc=Q(entry+8);
            if(model==0 || desc==0) return null;
            Need(Q(model)==image+PreviewModelVtable && Q(desc)==image+PreviewDescriptionVtable,"Player model class changed.");
            long roster=Q(desc+0xb8);
            var player=new BlacktopPlayer {slot=slot,description=desc,model=model,roster=roster,pointer=Q(desc+0xc8),uniform=Q(desc+0xf0),
                cyberface=BitConverter.ToUInt16(memory.Read(roster+0x110,2),0),name=(Text(roster+0x28,40)+" "+Text(roster,40)).Trim()};
            // Team identity comes from the active roster, never the player's name.
            long rosterRoot=Q(image+RosterRootRva),teamOwner=Q(rosterRoot+0xa8),teamTable=Q(teamOwner+0x88),team=Q(roster+0x60);
            int teamCount=I(teamOwner+0x80);
            if(teamCount>0 && teamCount<=1024 && team>=teamTable && team<teamTable+teamCount*0x1550L && (team-teamTable)%0x1550==0){
                player.teamId=BitConverter.ToUInt16(memory.Read(team+0xf20,2),0);player.teamName=Text(team+0x2fa,48);
            }
            player.uniformName=UniformName(player.uniform);player.uniformVerified=player.uniformName!=null;
            player.count=I(desc+0xd0);
            if(player.count<2 || player.count>7) { player.error="This outfit count is not supported."; return player; }
            player.sourcePointer=player.pointer;string[] privateItems;bool custom=PrivateList(player,out privateItems);
            if(!custom) { player.sourcePointer=player.pointer;player.streetIndex=-1;if(player.count!=2) {player.error="Unrecognized multi-item clothing list.";return player;} }
            OutfitGroup found=null; long basis=0;
            foreach(var g in groups) {
                foreach(var r in g.records) {
                    long candidate=player.sourcePointer-r.array;
                    if(ValidateLayout(candidate,g,r)) { found=g;basis=candidate;break; }
                }
                if(found!=null) break;
            }
            if(found==null) {player.error="Loaded outfit table does not match a verified preset layout.";return player;}
            player.groupKey=basis.ToString()+":"+found.descriptor.ToString();
            foreach(var r in found.records) if(ValidateLayout(basis,found,r)) {
                player.outfits.Add(new OutfitOption {id=r.array.ToString(),pointer=basis+r.array,items=r.names,
                    label=String.Join(" + ",r.names.Select(n=>n.Replace('_',' ')).ToArray())});
            }
            if(custom) {
                bool valid=ValidPrivateItems(player,privateItems,player.outfits);
                if(!valid) {
                    player.error="Custom clothes no longer match this player's verified wardrobe.";return player;
                }
                player.outfits.Add(new OutfitOption {id="custom",pointer=player.pointer,items=privateItems,label="Custom combination"});
            }
            return player;
        }
        static BlacktopSnapshot Scan(Process process) {
            long controller=Q(root+0x1080);
            Need(controller!=0 && memory.Read(controller+0x18,1)[0]==1,"Blacktop models are not ready. Select your players first.");
            var result=new BlacktopSnapshot {pid=process.Id,started=process.StartTime.ToUniversalTime().Ticks.ToString(),controller=controller};
            for(int slot=0;slot<10;slot++) {
                try {var p=ReadPlayer(controller,slot);if(p!=null) result.players.Add(p);}
                catch(InvalidOperationException) { /* Do not expose an unverified slot for editing. */ }
            }
            Need(result.players.Count>0,"No verified selected Blacktop players found.");
            // Never borrow another player's uniform. Independent uniform creation
            // has not been validated; absent contexts remain unavailable.
            return result;
        }
        static BlacktopPlayer VerifyRequest(Process process,BlacktopRequest request) {
            Need(process.Id==request.pid && process.StartTime.ToUniversalTime().Ticks.ToString()==request.started,"Game restarted; refresh the player list.");
            Need(Q(root+0x1080)==request.controller && I(root+0xea8)==3,"Blacktop screen changed; refresh the player list.");
            Need(Q(root)==image+BlacktopVtable && memory.Read(request.controller+0x18,1)[0]==1,"Blacktop controller is not ready.");
            Need(request.slot>=0 && request.slot<10,"Invalid player slot.");
            Need(request.uniformProtocol==5,"Outfit helper protocol changed; reopen the updated launcher.");
            var p=Scan(process).players.FirstOrDefault(candidate=>candidate.slot==request.slot);
            Need(p!=null && p.description==request.description && p.model==request.model && p.roster==request.roster && p.pointer==request.expected,"Player or outfit changed; refresh before applying.");
            Need(p.error==null,"Player wardrobe could not be verified.");
            Need(p.count==request.expectedCount,"Clothing count changed; refresh players.");
            OutfitOption option;
            if(request.target==0) {
                if(!String.IsNullOrEmpty(request.streetId)) option=new OutfitOption {items=StreetItemsWithAccessories(Array.FindIndex(street,s=>s.id==request.streetId),request.bottom,request.shoe,request.sock,request.layer,request.accessory,request.accessories)};
                else {
                    Need(p.outfits.Any(o=>o.items.Length==2 && o.items[0]==request.top) && p.outfits.Any(o=>o.items.Length==2 && o.items[1]==request.bottom),"Choose verified pieces from this player's wardrobe.");
                    option=new OutfitOption {items=new[]{request.top,request.bottom}};
                }
            } else {
                option=p.outfits.FirstOrDefault(o=>o.pointer==request.target) ?? HistoricalOption(p,request.target,request.targetCount);
            }
            Need(option.items.Length==request.targetCount,"Target clothing count does not match the verified list.");
            if(!String.IsNullOrEmpty(request.streetId)) Need(request.items!=null && option.items.SequenceEqual(request.items),"Streetwear catalog versions disagree; reopen the updated launcher.");
            Need(p.uniform==request.expectedUniform,"Uniform context changed; refresh before applying.");
            if(request.targetUniform!=p.uniform) {
                if(request.targetUniform==0) Need(!RequiresUniform(option),"Cannot remove uniform context while selecting uniform-dependent clothes.");
                else Need(false,"Borrowing or adding an unverified uniform is disabled. Choose pants until independent loading is available.");
            }
            if(RequiresUniform(option)) Need(UniformName(request.targetUniform)!=null,"This outfit needs a verified uniform context; choose pants for now.");
            Need(memory.Read(request.controller+0x30+request.slot*0x7b0,1)[0]==0,"A clothing refresh is already pending.");
            return p;
        }
        public static int Main(string[] args) {
            try {
                Need(args.Length==3 || args.Length==4,"Expected scan/apply, game root, layout file, optional request file.");
                if(args[0]=="detect") {
                    var games=Process.GetProcessesByName("NBA2K27");
                    if(games.Length==0) {Console.WriteLine(Json.Serialize(new {selection=false,session="",foreground=false}));return 0;}
                    Need(games.Length==1,"No unique game session.");
                    using(var game=games[0]) using(memory=new Reader(game.Id,false)) {
                        CheckGame(game,args[1],false);
                        uint foreground;GetWindowThreadProcessId(GetForegroundWindow(),out foreground);
                        bool selection=Q(root)==image+BlacktopVtable && I(root+0xea8)==3;
                        int required=0,selected=0,loaded=0;bool lineupReady=false,playersReady=false;
                        var lineupPlayers=new List<object>();
                        if(selection) {
                            long lineup=image+LineupRva,controller=Q(root+0x1080);
                            required=I(lineup+4);selected=I(lineup+0xb8);
                            if(required>=2 && required<=10 && required%2==0 && I(lineup+8)==required
                                && selected>=0 && selected<=required && controller!=0 && memory.Read(controller+0x18,1)[0]==1) {
                                for(int slot=0;slot<10;slot++) {
                                    long entry=controller+0x20+slot*0x7b0,model=Q(entry),desc=Q(entry+8);
                                    if(model!=0 && desc!=0 && Q(model)==image+PreviewModelVtable && Q(desc)==image+PreviewDescriptionVtable){loaded++;lineupPlayers.Add(new {slot=slot,description=desc,roster=Q(desc+0xb8)});}
                                }
                                // Recheck after reading models; a transition must never open the panel.
                                playersReady=loaded==selected && I(lineup+4)==required && I(lineup+8)==required
                                    && I(lineup+0xb8)==selected && Q(root+0x1080)==controller && I(root+0xea8)==3;
                                lineupReady=playersReady && selected==required;
                            }
                        }
                        Console.WriteLine(Json.Serialize(new {selection=selection,lineupReady=lineupReady,requiredPlayers=required,selectedPlayers=selected,
                            playersReady=playersReady,lineupPlayers=playersReady?lineupPlayers:null,session=gamePid+":"+gameStarted,foreground=foreground==(uint)gamePid}));return 0;
                    }
                }
                if(args[0]=="focus") {
                    var games=Process.GetProcessesByName("NBA2K27");
                    Need(games.Length==1,"No unique NBA 2K27 window to return to.");
                    using(var game=games[0]) {
                        Need(String.Equals(Path.GetFullPath(game.MainModule.FileName),Path.Combine(Path.GetFullPath(args[1]),"NBA2K27.exe"),StringComparison.OrdinalIgnoreCase),"Game window path changed.");
                        var window=game.MainWindowHandle;Need(window!=IntPtr.Zero,"NBA 2K27 window is not available.");
                        if(IsIconic(window)) ShowWindowAsync(window,9);
                        Console.WriteLine(Json.Serialize(new {focused=SetForegroundWindow(window)}));return 0;
                    }
                }
                bool apply=args[0]=="apply", original=args[0]=="original";Need(apply || original || args[0]=="scan","Invalid action.");
                Need(!(apply||original) || args.Length==4,"Missing guarded request.");
                groups=Json.Deserialize<OutfitGroup[]>(File.ReadAllText(args[2]));
                string streetPath=Path.Combine(Path.GetDirectoryName(Path.GetFullPath(args[2])),"streetwear-components.json");
                if(File.Exists(Path.ChangeExtension(streetPath,".bin"))&&(!File.Exists(streetPath)||File.GetLastWriteTimeUtc(Path.ChangeExtension(streetPath,".bin"))>=File.GetLastWriteTimeUtc(streetPath)))LoadNativeCatalog(Path.ChangeExtension(streetPath,".bin"));
                else if(File.Exists(streetPath)) {var catalog=Json.Deserialize<StreetCatalog>(File.ReadAllText(streetPath));street=catalog.presets;additionalShoes=catalog.additionalShoes??new ExtraShoe[0];accessoryAssets=(catalog.accessoryTest==null?new AccessoryEntry[0]:(catalog.accessoryTest.added??new AccessoryEntry[0])).Concat(catalog.additionalAccessories??new AccessoryEntry[0]).Select(a=>a.asset).ToArray();}
                var processes=Process.GetProcessesByName("NBA2K27");Need(processes.Length==1,"Exactly one NBA 2K27 process must be running.");
                using(var process=processes[0]) using(memory=new Reader(process.Id,apply)) {
                    CheckGame(process,args[1]);
                    if(!apply && !original) {Console.WriteLine(Json.Serialize(Scan(process)));return 0;}
                    var request=Json.Deserialize<BlacktopRequest>(File.ReadAllText(args[3]));
                    if(original) {
                        var verified=VerifyRequest(process,request);
                        var saved=verified.outfits.FirstOrDefault(o=>o.pointer==request.target) ?? HistoricalOption(verified,request.target,request.targetCount);
                        Console.WriteLine(Json.Serialize(saved));return 0;
                    }
                    var player=VerifyRequest(process,request);
                    bool composed=request.target==0;
                    if(composed) {
                        int streetIndex=String.IsNullOrEmpty(request.streetId) ? -1 : Array.FindIndex(street,s=>s.id==request.streetId);
                        if(streetIndex>=0) Need(street[streetIndex].sourceGroup.pool==ClothingPool(player),"This clothing collection does not match the player's original wardrobe.");
                        var items=streetIndex<0 ? new[]{request.top,request.bottom} : StreetItemsWithAccessories(streetIndex,request.bottom,request.shoe,request.sock,request.layer,request.accessory,request.accessories);
                        long prepared=CreatePrivateList(player,items,streetIndex,request.shoe=="roster");
                        // Recheck the selected model and original preset after preparation.
                        VerifyRequest(process,request);
                        request.target=prepared;
                    }
                    // Journal is durably saved by the main process before invoking this helper.
                    long field=request.description+0xc8, uniformField=request.description+0xf0, flag=request.controller+0x30+request.slot*0x7b0;
                    // Add a required context before the source update. On Restore,
                    // switch away from uniform clothes before removing the context.
                    if(request.targetUniform!=0 && request.targetUniform!=request.expectedUniform) {
                        memory.Write(uniformField,BitConverter.GetBytes(request.targetUniform));
                        Need(Q(uniformField)==request.targetUniform,"Uniform context readback failed; use Restore.");
                    }
                    var source=new byte[12];Buffer.BlockCopy(BitConverter.GetBytes(request.target),0,source,0,8);Buffer.BlockCopy(BitConverter.GetBytes(request.targetCount),0,source,8,4);
                    memory.Write(field,source);
                    Need(Q(field)==request.target && I(field+8)==request.targetCount,"Outfit source readback failed; use Restore.");
                    if(request.targetUniform==0 && request.expectedUniform!=0) {
                        memory.Write(uniformField,BitConverter.GetBytes(0L));
                        Need(Q(uniformField)==0,"Uniform context restore readback failed; use Restore.");
                    }
                    memory.Write(flag,new byte[]{1});
                    bool consumed=false;
                    for(int i=0;i<30;i++) {
                        Thread.Sleep(100);
                        Need(Q(root+0x1080)==request.controller && Q(request.controller+0x28+request.slot*0x7b0)==request.description,"Player changed while waiting; refresh the list.");
                        if(memory.Read(flag,1)[0]==0) {consumed=true;break;}
                    }
                    Console.WriteLine(Json.Serialize(new {consumed=consumed, pointerMatches=Q(field)==request.target,
                        countMatches=I(field+8)==request.targetCount,uniformMatches=Q(uniformField)==request.targetUniform,composed=composed,
                        uniformAdded=request.expectedUniform==0 && request.targetUniform!=0}));return 0;
                }
            } catch(Exception e) {Console.Error.WriteLine(e.Message);return 1;}
        }
    }
}
