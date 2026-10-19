using System;
using System.Linq;

internal static class ScalerPolicy {
 internal const uint Original=0x3f7ae148;
 internal static uint Target(int percent) {
  if(percent<95||percent>100)throw new ArgumentException("Player size must be 95–100%.");
  if(percent==100)return Original;
  float baseline=BitConverter.ToSingle(BitConverter.GetBytes(Original),0);
  return BitConverter.ToUInt32(BitConverter.GetBytes((float)((double)baseline*percent/100.0)),0);
 }
 internal static bool Owned(uint current,uint applied,uint requested){return current==Original||current==applied||current==requested;}
 internal static ulong InputRva(string build){
  if(build=="6A974D13:368C3000")return 0xA38F19C;
  if(build=="6AA2013D:36558000")return 0xA39319C;
  throw new ArgumentException("Unsupported scaler build.");
 }
 internal static bool Same(ScalerJournal j,int pid,long created,ulong basis,string image,string build="6A974D13:368C3000"){
  return j!=null&&j.schema==1&&j.phase!="ended"&&j.pid==pid&&j.created==created&&j.basis==basis&&j.build==build&&j.address==basis+InputRva(build)&&String.Equals(j.image,image,StringComparison.OrdinalIgnoreCase);
 }
 internal static void Validate(ScalerJournal j){
  if(j.schema!=1||j.original!=Original||j.address!=j.basis+InputRva(j.build)||!Enumerable.Range(95,6).Any(p=>Target(p)==j.applied)||!Enumerable.Range(95,6).Any(p=>Target(p)==j.requested)||j.coaches==null||j.coaches.Length>16||j.coaches.Any(c=>c==null||c.manager<0x10000||c.actor<0x10000||c.description!=c.actor+0xF20)||j.arenaCharacters==null||j.arenaCharacters.Length>220||j.arenaCharacters.Any(c=>c==null||c.manager<0x10000||c.actor<0x10000||c.description<0x10000||(c.kind!="generic"&&c.kind!="cheerleader")))throw new Exception("Player size recovery data is invalid; no memory was changed.");
 }
}
internal sealed class CoachOptIn {
 public ulong manager,actor,description; public byte originalFlag;
}
internal sealed class ArenaCharacterOptIn {
 public string kind; public ulong manager,actor,description; public byte originalFlag;
 public uint originalScale,originalInverse,targetScale,targetInverse;
 public bool cachePrepared;
}
internal sealed class ScalerJournal {
 public int schema=1,pid; public long created; public ulong basis,address;
 public string image,build="6A974D13:368C3000",phase;
 public uint original=ScalerPolicy.Original,applied=ScalerPolicy.Original,requested=ScalerPolicy.Original;
 public bool reloadRequired;
 public CoachOptIn[] coaches=new CoachOptIn[0];
 public ArenaCharacterOptIn[] arenaCharacters=new ArenaCharacterOptIn[0];
}
