using System;
using System.Collections.Generic;
using System.Text;
namespace NBA2K27RosterResearch {
internal sealed class RotationMember {public int id,index;public string name;}
internal static class TeamRotationSnapshot {
 internal static string Digest(byte[] bytes){using(var sha=System.Security.Cryptography.SHA256.Create())return BitConverter.ToString(sha.ComputeHash(bytes)).Replace("-","").ToLowerInvariant();}
 internal static string MemberDigest(List<RotationMember> members){return Digest(Encoding.UTF8.GetBytes(new System.Web.Script.Serialization.JavaScriptSerializer().Serialize(members)));}
 internal static List<RotationMember> Members(long team,byte[] records,int stride){
  if(team<=0||records==null||stride<0x12A||records.Length%stride!=0||records.Length/stride>8192)throw new Exception("Invalid rotation membership snapshot.");
  var rows=new List<RotationMember>();var ids=new HashSet<int>();
  for(int i=0;i<records.Length/stride;i++){int offset=i*stride;if(BitConverter.ToInt64(records,offset+0x60)!=team)continue;
   int id=BitConverter.ToUInt16(records,offset+0x128);if(!ids.Add(id))throw new Exception("Team player IDs are ambiguous. Rotation editing is unavailable.");
   string last=Name(records,offset),first=Name(records,offset+40);if(first.Length+last.Length==0)throw new Exception("An assigned player is unnamed.");
   rows.Add(new RotationMember{id=id,index=i,name=(first+" "+last).Trim()});
  }if(rows.Count<5||rows.Count>64)throw new Exception("Load a team with at least five verified roster members.");return rows;
 }
 static string Name(byte[] records,int at){int end=at;while(end<at+40&&BitConverter.ToUInt16(records,end)!=0)end+=2;if(end>=at+40)throw new Exception("Unterminated player name.");return Encoding.Unicode.GetString(records,at,end-at);}
 internal static int[][] Rows(byte[] raw){if(raw==null||raw.Length!=480)throw new Exception("Invalid advanced rotation data.");var rows=new int[48][];for(int i=0;i<48;i++){rows[i]=new int[5];for(int j=0;j<5;j++)rows[i][j]=BitConverter.ToUInt16(raw,i*10+j*2);}return rows;}
 internal static void Validate(byte[] before,byte[] after,List<RotationMember> members){
  var a=Rows(before);var b=Rows(after);var valid=new HashSet<int>();foreach(var m in members)if(!valid.Add(m.id))throw new Exception("Ambiguous team members.");
  for(int i=0;i<48;i++){bool changed=false;for(int j=0;j<5;j++)changed|=a[i][j]!=b[i][j];if(!changed)continue;
   var seen=new HashSet<int>();foreach(int id in b[i])if(!valid.Contains(id)||!seen.Add(id))throw new Exception("Each edited rotation segment needs five different players from this team.");
  }
 }
}
}
