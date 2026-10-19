using System;
using System.Collections.Generic;
namespace NBA2K27RosterResearch {
internal static class TeamArenaPair {
 internal static Arena Target(Arena donor,string floor,List<Arena> arenas,HashSet<long> used){
  if(donor==null||donor.EmptyCandidate||String.IsNullOrEmpty(donor.ArenaFile)||String.IsNullOrEmpty(floor))throw new Exception("Choose a populated arena and floor.");
  if(floor==donor.FloorFile)return donor;
  if(!arenas.Exists(a=>!a.EmptyCandidate&&a.FloorFile==floor))throw new Exception("The floor is not in the live arena catalog.");
  foreach(var a in arenas)if(a.EmptyCandidate&&a.Id>=996&&a.Id<=1045&&!used.Contains(a.Address)&&arenas.FindAll(b=>b.Id==a.Id).Count==1)return a;
  throw new Exception("No unreferenced empty reserved arena record is available. No arena or floor was changed.");
 }
}
}
