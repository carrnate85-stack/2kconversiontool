using System;
using System.Linq;
using System.Text;
internal static class FreestylePolicyTests {
 static void Check(bool b,string message){if(!b)throw new Exception(message);}
 static void Reject(Action fn){try{fn();}catch(InvalidOperationException){return;}throw new Exception("Invalid data accepted");}
 public static int Main(){
  Check(FreestylePolicy.Pages.Aggregate(0UL,(a,b)=>a|b)==FreestylePolicy.CombinedMask,"All existing pages must survive");
  for(int kind=0;kind<64;kind++)Check(((FreestylePolicy.CombinedMask>>kind)&1)!=0==new[]{0,1,4,5,22,27,29,30}.Contains(kind),"Unexpected team category");
  for(uint c=0;c<5;c++)Check(FreestylePolicy.Stock(FreestylePolicy.StockMask,c),"Stock page rejected");
  Check(!FreestylePolicy.Stock(FreestylePolicy.CombinedMask,5)&&!FreestylePolicy.Stock(FreestylePolicy.StockMask,5),"Do not adopt unknown combined mode");
  Check(FreestylePolicy.Applied(FreestylePolicy.CombinedMask,5)&&!FreestylePolicy.Applied(FreestylePolicy.CombinedMask,0),"Applied state must match both fields");
  int n=49,s=FreestylePolicy.Stride;var rows=new byte[n*s];int[] ids=Enumerable.Range(300,12).Concat(new[]{315,316,317,69,361}).Concat(Enumerable.Range(0,30)).Concat(new[]{400,401}).ToArray();
  for(int i=0;i<n;i++){int type=i<15?29:i<17?22:i<47?0:i==47?1:27;BitConverter.GetBytes((ushort)ids[i]).CopyTo(rows,i*s+0xf20);BitConverter.GetBytes((uint)type<<26).CopyTo(rows,i*s+0x123c);Encoding.Unicode.GetBytes("Club").CopyTo(rows,i*s+0x2fa);}
  Check(FreestylePolicy.VerifyTeams(rows,n)==49,"Mixed roster rejected");Reject(()=>FreestylePolicy.VerifyTeams(rows,n-1));Reject(()=>FreestylePolicy.VerifyTeams(new byte[0],0));
  var wrong=(byte[])rows.Clone();BitConverter.GetBytes((ushort)301).CopyTo(wrong,0xf20);Reject(()=>FreestylePolicy.VerifyTeams(wrong,n));
  wrong=(byte[])rows.Clone();Array.Clear(wrong,0x2fa,48);Reject(()=>FreestylePolicy.VerifyTeams(wrong,n));
  wrong=(byte[])rows.Clone();BitConverter.GetBytes(28U<<26).CopyTo(wrong,0x123c);Reject(()=>FreestylePolicy.VerifyTeams(wrong,n));
  wrong=(byte[])rows.Clone();BitConverter.GetBytes(12U<<26).CopyTo(wrong,48*s+0x123c);Reject(()=>FreestylePolicy.VerifyTeams(wrong,n));
  var j=new FreestyleJournal{pid=12,created=345,basis=0x100000,image="game",phase="applied"};FreestylePolicy.Validate(j);
  Check(FreestylePolicy.Same(j,12,345,0x100000,"GAME"),"Exact session rejected");Check(!FreestylePolicy.Same(j,12,346,0x100000,"game"),"PID reuse accepted");Check(!FreestylePolicy.Same(j,13,345,0x100000,"game"),"Different process accepted");Check(!FreestylePolicy.Same(j,12,345,0x100001,"game"),"Different image base accepted");
  j.originalCategory=5;Reject(()=>FreestylePolicy.Validate(j));
  Console.WriteLine("PASS: category preservation, roster validation, duplicate/missing rejection, ownership and session identity");return 0;
 }
}
