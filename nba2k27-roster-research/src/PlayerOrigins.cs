using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;
namespace NBA2K27RosterResearch {
 internal static partial class Program {
  private sealed class OriginDirectory {
   internal long Image,Root,Owner,Players,Table;
   internal int PlayerCount,Count;
   internal byte[] Bytes;
   internal string[] Names;
   internal bool[] Selectable;
   internal bool Stable(Reader m) {
    return BitConverter.ToInt64(m.Read(Image+0x9A47EB8,8),0)==Root
     && BitConverter.ToInt64(m.Read(Root+0xA8,8),0)==Owner
     && BitConverter.ToInt64(m.Read(Owner+0x18,8),0)==Players
     && BitConverter.ToInt32(m.Read(Owner+0x10,4),0)==PlayerCount
     && BitConverter.ToInt64(m.Read(Owner+0x98,8),0)==Table
     && BitConverter.ToInt32(m.Read(Owner+0x90,4),0)==Count;
   }
   internal int Index(long pointer) {
    long delta=pointer-Table;
    return pointer==0?-1:delta>=0 && delta<(long)Count*0x48 && delta%0x48==0?(int)(delta/0x48):-2;
   }
   internal string OptionsJson() {
    var s=new StringBuilder("[");bool first=true;
    for(int i=0;i<Count;i++)if(Selectable[i] && !String.IsNullOrEmpty(Names[i])) {
     if(!first)s.Append(',');first=false;
     s.Append("{\"index\":").Append(i).Append(",\"name\":").Append(Json(Names[i])).Append('}');
    }
    return s.Append(']').ToString();
   }
  }
  private static OriginDirectory ReadOriginDirectory(Reader m,long players) {
   long image=Process.GetProcessById(m.ProcessId).MainModule.BaseAddress.ToInt64();
   byte[] header=m.Read(image,4096);int pe=BitConverter.ToInt32(header,60);
   if(pe<64||pe>3800||BitConverter.ToUInt32(header,pe+8)!=0x6AA2013D||BitConverter.ToUInt32(header,pe+80)!=0x36558000)throw new InvalidOperationException("Unsupported From layout.");
   using(var hash=System.Security.Cryptography.SHA256.Create()) {
    if(BitConverter.ToString(hash.ComputeHash(m.Read(image+RatingSchema.Start,RatingSchema.Length))).Replace("-","").ToLowerInvariant()!=RatingSchema.Fingerprint)throw new InvalidOperationException("From build validation failed.");
   }
   byte[] setter={0x89,0xF9,0x48,0x8D,0x0C,0xC9,0x48,0xC1,0xE1,0x03,0x48,0x03,0x88,0x98,0,0,0,0x74,0x07,0x48,0x89,0x8E,0xB0,0,0,0};
   if(!SameBytes(m.Read(image+0x2DABB48,setter.Length),setter))throw new InvalidOperationException("From setter layout changed.");
   var dir=new OriginDirectory();dir.Image=image;dir.Root=BitConverter.ToInt64(m.Read(image+0x9A47EB8,8),0);
   dir.Owner=BitConverter.ToInt64(m.Read(dir.Root+0xA8,8),0);
   dir.Players=BitConverter.ToInt64(m.Read(dir.Owner+0x18,8),0);dir.PlayerCount=BitConverter.ToInt32(m.Read(dir.Owner+0x10,4),0);
   dir.Table=BitConverter.ToInt64(m.Read(dir.Owner+0x98,8),0);dir.Count=BitConverter.ToInt32(m.Read(dir.Owner+0x90,4),0);
   if(dir.Players!=players||dir.PlayerCount<100||dir.PlayerCount>8192||dir.Table<=0||dir.Count<1||dir.Count>4096)throw new InvalidOperationException("From directory ownership could not be verified.");
   dir.Bytes=m.Read(dir.Table,checked(dir.Count*0x48));dir.Names=new string[dir.Count];dir.Selectable=new bool[dir.Count];
   var utf8=new UTF8Encoding(false,true);
   for(int i=0;i<dir.Count;i++) {
    int start=i*0x48,end=Array.IndexOf(dir.Bytes,(byte)0,start,48);
    if(end<0)continue;
    try { string name=utf8.GetString(dir.Bytes,start,end-start);bool valid=true;foreach(char c in name)if(Char.IsControl(c))valid=false;
     if(valid)dir.Names[i]=name;
    } catch(DecoderFallbackException) { }
    dir.Selectable[i]=(BitConverter.ToUInt32(dir.Bytes,start+0x34)&0xC000000)!=0xC000000;
   }
   if(!dir.Stable(m)||!SameBytes(dir.Bytes,m.Read(dir.Table,dir.Bytes.Length)))throw new InvalidOperationException("Roster origins changed during read.");
   return dir;
  }
  private static string ReadPlayerOrigins(Reader m,long table,List<Player> players) {
   try {
    var dir=ReadOriginDirectory(m,table);var indices=new List<int?>();var names=new List<string>();
    foreach(var player in players) {
     byte[] r=m.Read(player.Address,PlayerStride);int index=dir.Index(BitConverter.ToInt64(r,0xB0));
     if(DecodeUtf16(r,0,40)!=player.LastName||DecodeUtf16(r,40,40)!=player.FirstName||!SameBytes(r,m.Read(player.Address,PlayerStride))) { indices.Add(null);names.Add(null);continue; }
     indices.Add(index>=-1?(int?)index:null);names.Add(index>=0?dir.Names[index]:null);
    }
    if(!dir.Stable(m)||!SameBytes(dir.Bytes,m.Read(dir.Table,dir.Bytes.Length)))return "[]";
    for(int i=0;i<players.Count;i++){players[i].FromIndex=indices[i];players[i].CollegeName=names[i];}
    return dir.OptionsJson();
   } catch { return "[]"; }
  }
 }
}
