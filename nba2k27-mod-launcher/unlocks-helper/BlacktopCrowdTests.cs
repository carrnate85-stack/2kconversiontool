using System;
internal static class BlacktopCrowdTests {
 static void Check(bool value){if(!value)throw new Exception("Crowd scope policy assertion failed.");}
 static void Journals(){
  string directory=System.IO.Path.Combine(System.IO.Path.GetTempPath(),"2k-crowd-journal-test-"+Guid.NewGuid().ToString());
  System.IO.Directory.CreateDirectory(directory);string file=System.IO.Path.Combine(directory,"crowd.json");
  var serializer=new System.Web.Script.Serialization.JavaScriptSerializer();
  var fields=new System.Collections.Generic.Dictionary<string,object>{{"schema",1},{"pid",123},{"created",134353536640988922L},{"basis",0x140000000L},{"owner",0x20000},{"image",@"C:\game\NBA2K27.exe"},{"phase","applied"},{"timestamp",0x6AA2013D},{"size",0x36558000},{"original",1},{"expected",0}};
  var read=typeof(FreestyleTeams).GetMethod("CrowdLoad",System.Reflection.BindingFlags.NonPublic|System.Reflection.BindingFlags.Static);
  Action<bool> verify=valid=>{
   System.IO.File.WriteAllText(file,serializer.Serialize(fields));bool accepted=true;
   try{read.Invoke(null,new object[]{file});}catch(System.Reflection.TargetInvocationException){accepted=false;}
   Check(accepted==valid);
  };
  try{
   verify(true);fields["schema"]=2;verify(false);
   var front=new byte[24];front[1]=0x4C;front[8]=1;fields["sourceOwner"]=0x30000;fields["frontOriginal"]=front;verify(true);
   fields["extra"]=0;verify(false);fields.Remove("extra");
   fields["sourceOwner"]=0;verify(false);fields["sourceOwner"]=0x30000;
   front[2]=1;verify(true);front[8]=2;verify(false);front[8]=1;
   fields["expected"]=1;verify(false);fields["expected"]=0;
   fields["original"]=2;verify(false);fields["original"]=1;
   fields["schema"]=3;verify(false);
   fields.Remove("sourceOwner");fields.Remove("frontOriginal");
   var records=new object[5];
   for(int i=0;i<5;i++){var values=(byte[])front.Clone();values[8]=(byte)(i%2);records[i]=new System.Collections.Generic.Dictionary<string,object>{{"index",175+i},{"owner",0x30000L+i*0xC810L},{"original",values}};}
   fields["records"]=records;verify(true);fields["phase"]="partial";verify(true);fields["phase"]="arbitrary";verify(false);fields["phase"]="applied";
   var last=(System.Collections.Generic.Dictionary<string,object>)records[4];
   last["index"]=180;verify(false);last["index"]=179;
   last["owner"]=0x30000L;verify(false);last["owner"]=0x30000L+4*0xC810L;
   last["extra"]=0;verify(false);last.Remove("extra");
   fields["records"]=new object[]{records[0]};verify(false);fields["records"]=records;
   ((byte[])last["original"])[8]=2;verify(false);((byte[])last["original"])[8]=0;verify(true);
  }finally{System.IO.File.Delete(file);System.IO.Directory.Delete(directory);}
 }
 static void Snapshots(){
  var flags=System.Reflection.BindingFlags.NonPublic|System.Reflection.BindingFlags.Instance;
  Func<string,Type> nested=name=>typeof(FreestyleTeams).GetNestedType(name,System.Reflection.BindingFlags.NonPublic);
  Action<object,string,object> set=(obj,name,value)=>obj.GetType().GetField(name,flags).SetValue(obj,value);
  Func<object> snapshot=()=>{
   var s=Activator.CreateInstance(nested("CrowdSnapshot"),true);set(s,"owner",0x10000UL);set(s,"preloadTable",0x20000UL);set(s,"window",new byte[16]);
   var context=Activator.CreateInstance(nested("CrowdContext"),true);set(context,"current",0x92DF06DEU);set(context,"alternate",0x92DF06DEU);set(context,"stage",3U);set(context,"gameOwner",0x30000UL);set(s,"context",context);
   var rows=Array.CreateInstance(nested("CrowdSource"),5);for(int i=0;i<5;i++){var row=Activator.CreateInstance(nested("CrowdSource"),true);set(row,"index",(uint)(175+i));set(row,"owner",0x40000UL+(ulong)i*0xC810);var front=new byte[24];front[1]=0x4C;set(row,"front",front);set(row,"ambient",new byte[24]);rows.SetValue(row,i);}set(s,"sources",rows);return s;
  };
  var before=snapshot();var after=snapshot();var data=nested("CrowdSnapshot").GetMethod("DataExact",flags);var exact=nested("CrowdSnapshot").GetMethod("Exact",flags);
  Check((bool)data.Invoke(before,new[]{after}));Check((bool)exact.Invoke(before,new[]{after}));
  Check(nested("CrowdSnapshot").GetField("gameWindow",flags)==null);
  var contextAfter=nested("CrowdSnapshot").GetField("context",flags).GetValue(after);set(contextAfter,"active",(byte)1);Check((bool)data.Invoke(before,new[]{after}));Check(!(bool)exact.Invoke(before,new[]{after}));
  set(contextAfter,"active",(byte)0);((byte[])nested("CrowdSnapshot").GetField("window",flags).GetValue(after))[6]=1;Check(!(bool)data.Invoke(before,new[]{after}));
  after=snapshot();var rowsAfter=(Array)nested("CrowdSnapshot").GetField("sources",flags).GetValue(after);var third=rowsAfter.GetValue(2);((byte[])nested("CrowdSource").GetField("front",flags).GetValue(third))[9]=1;Check(!(bool)data.Invoke(before,new[]{after}));
  after=snapshot();set(after,"preloadTable",0x20008UL);Check(!(bool)data.Invoke(before,new[]{after}));
 }
 public static void Main(){
  const uint selection=0x92DF06DE,menu=0x6509837B;
  Check(BlacktopCrowdPolicy.Selection(selection,selection,3,0,0));
  Check(!BlacktopCrowdPolicy.Selection(selection,selection,3,1,0));
  Check(!BlacktopCrowdPolicy.Selection(selection,selection,3,0,1));
  Check(BlacktopCrowdPolicy.MainMenu(menu,menu,1,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(menu,menu,1,1,0));
  Check(!BlacktopCrowdPolicy.MainMenu(menu,selection,1,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(menu,menu,3,0,0));
  const uint playNow=0x079964C0;
  Check(BlacktopCrowdPolicy.MainMenu(playNow,playNow,1,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(playNow,menu,1,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(playNow,playNow,1,1,0));
  Check(!BlacktopCrowdPolicy.MainMenu(playNow,playNow,1,0,1));
  Check(!BlacktopCrowdPolicy.MainMenu(playNow,playNow,3,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(0x7C602DF8,0x7C602DF8,1,0,0));
  Check(!BlacktopCrowdPolicy.MainMenu(0xB4D37B22,0xB4D37B22,1,0,0));
  Check(!BlacktopCrowdPolicy.ValidOwner(0));Check(!BlacktopCrowdPolicy.ValidOwner(ulong.MaxValue));
  for(uint index=175;index<=179;index++){
   Check(BlacktopCrowdPolicy.SourceRow(0,index)==0);
   Check(BlacktopCrowdPolicy.SourceRow(ulong.MaxValue,index)==0);
   Check(BlacktopCrowdPolicy.SourceRow(0x7fffffffffffUL,index)==0);
   Check(BlacktopCrowdPolicy.SourceRow(0x10001,index)==0);
   Check(BlacktopCrowdPolicy.SourceRow(0x10000,index)==0x10000+index*0xC810UL+8);
   Check(BlacktopCrowdPolicy.SourceTagPhase(false,index,index)=="ready");
   Check(BlacktopCrowdPolicy.SourceTagPhase(true,index,index)=="ready");
   foreach(uint tag in new uint[]{0,1,174,180,uint.MaxValue}){
    Check(BlacktopCrowdPolicy.SourceTagPhase(false,index,tag)=="waiting");
    Check(BlacktopCrowdPolicy.SourceTagPhase(true,index,tag)=="conflict");
   }
  }
  Check(BlacktopCrowdPolicy.SourceRow(0x10000,174)==0);Check(BlacktopCrowdPolicy.SourceRow(0x10000,180)==0);
  Check(BlacktopCrowdPolicy.SourceRow(0x2419FB60050UL,175)==0x241A03EC348UL);
  Check(BlacktopCrowdPolicy.MaxWrites==6);
  Check(BlacktopCrowdPolicy.Decide(false,false,false,false,true,true,false,false)=="wait");
  Check(BlacktopCrowdPolicy.Decide(false,false,true,false,true,true,false,false)=="apply");
  Check(BlacktopCrowdPolicy.Decide(false,false,true,false,true,true,false,true)=="unchanged");
  Check(BlacktopCrowdPolicy.Decide(false,true,false,false,true,true,false,true)=="observe");
  Check(BlacktopCrowdPolicy.Decide(false,true,false,false,true,true,true,false)=="wait-reset");
  Check(BlacktopCrowdPolicy.Decide(false,true,true,false,true,true,true,false)=="reapply");
  Check(BlacktopCrowdPolicy.Decide(false,true,false,true,true,true,false,true)=="restore");
  Check(BlacktopCrowdPolicy.Decide(false,true,false,true,true,true,true,false)=="reset");
  Check(BlacktopCrowdPolicy.Decide(false,true,true,false,false,true,false,true)=="conflict");
  Check(BlacktopCrowdPolicy.Decide(false,true,false,true,true,false,false,true)=="conflict");
  Check(BlacktopCrowdPolicy.Decide(true,true,true,false,true,true,false,true)=="wait-exit");
  Check(BlacktopCrowdPolicy.Decide(true,true,false,false,true,true,false,true)=="wait-exit");
  Check(BlacktopCrowdPolicy.Decide(true,true,false,true,true,true,false,true)=="restore");
  Check(BlacktopCrowdPolicy.Decide(true,false,false,false,true,true,false,false)=="noop");
  var original=new byte[24];original[1]=0x4C;original[8]=1;
  var removed=(byte[])original.Clone();removed[8]=0;
  Check(BlacktopCrowdPolicy.OwnedValues(1,0,original,removed));
  Check(BlacktopCrowdPolicy.OwnedValues(1,1,original,removed));
  Check(!BlacktopCrowdPolicy.OwnedValues(0,1,original,removed));
  removed[2]=1;Check(!BlacktopCrowdPolicy.OwnedValues(1,0,original,removed));
  removed[2]=0;removed[8]=2;Check(!BlacktopCrowdPolicy.OwnedValues(1,0,original,removed));
  var stand=new byte[16];var after=(byte[])stand.Clone();after[1]=1;
  Check(BlacktopCrowdPolicy.UnselectedEqual(stand,after,1));after[3]=1;
  Check(!BlacktopCrowdPolicy.UnselectedEqual(stand,after,1));
  Check(BlacktopCrowdPolicy.Owns("partial"));Check(BlacktopCrowdPolicy.Owns("applied"));Check(!BlacktopCrowdPolicy.Owns("failed"));Check(!BlacktopCrowdPolicy.Owns("prepared"));
  Check(BlacktopCrowdPolicy.FailurePhase(true,true,"applied")=="partial");
  Check(BlacktopCrowdPolicy.FailurePhase(true,false,"partial")=="partial");
  Check(BlacktopCrowdPolicy.FailurePhase(true,false,null)=="restored");
  Check(BlacktopCrowdPolicy.FailurePhase(false,true,"applied")=="failed");
  Check(BlacktopCrowdPolicy.FailurePhase(false,false,"applied")=="applied");
  Snapshots();
  Journals();
  Console.WriteLine("PASS: Blacktop five-preset allowlist, strict schema 3 originals, selection-only apply, no gameplay writes, menu restore, ownership and foreign-neighbor rejection");
 }
}
