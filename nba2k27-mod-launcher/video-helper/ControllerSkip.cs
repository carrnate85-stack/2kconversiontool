using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

// Read-only controller observer. No hooks, input synthesis, game memory or vibration.
class ControllerSkip {
 [StructLayout(LayoutKind.Sequential)] struct Pad {public ushort Buttons;public byte LeftTrigger,RightTrigger;public short LX,LY,RX,RY;}
 [StructLayout(LayoutKind.Sequential)] struct State {public uint Packet;public Pad Gamepad;}
 [DllImport("xinput1_4.dll",EntryPoint="XInputGetState")] static extern uint Get14(uint index,out State state);
 [DllImport("xinput9_1_0.dll",EntryPoint="XInputGetState")] static extern uint Get91(uint index,out State state);
 static bool legacy=false;static volatile bool stop=false;
 static uint Read(uint index,out State state){try{return legacy?Get91(index,out state):Get14(index,out state);}catch(DllNotFoundException){legacy=true;return Get91(index,out state);}catch(EntryPointNotFoundException){legacy=true;return Get91(index,out state);}}
 internal sealed class Edge {
  bool neutral=false,pressed=false;long released=-1;
  public void Reset(){neutral=false;pressed=false;released=-1;}
  public bool Step(bool connected,bool down,long ms){
   if(!connected){Reset();return false;}
   if(!down)neutral=true;
   if(neutral&&down&&ms>=700){pressed=true;released=-1;}
   if(pressed&&!down){if(released<0)released=ms;if(ms-released>=200)return true;}
   return false;
  }
 }
 static int SelfTest(){
  var e=new Edge();if(e.Step(true,true,1000)||e.Step(true,false,1100)||e.Step(true,false,1500))return 1;
  if(e.Step(true,true,1600)||e.Step(true,false,1700)||e.Step(true,false,1899)||!e.Step(true,false,1900))return 2;
  e.Reset();e.Step(true,false,0);e.Step(true,true,100);if(e.Step(true,false,1000)||e.Step(true,false,1300))return 3;
  e.Step(true,true,1400);e.Step(false,false,1500);if(e.Step(true,false,2000)||e.Step(true,false,2500))return 4;
  Console.WriteLine("PASS: fresh press, held-at-open, release delay, opening guard and disconnect reset");return 0;
 }
 static int Main(string[] args){try{
  if(args.Length==1&&args[0]=="--self-test")return SelfTest();
  if(args.Length==1&&args[0]=="--probe"){
   for(uint i=0;i<4;i++){State s;uint code=Read(i,out s);Console.WriteLine("{\"event\":\"controller\",\"slot\":"+i+",\"connected\":"+(code==0?"true":"false")+"}");}return 0;
  }
  int pid;if(args.Length!=2||args[0]!="--watch"||!int.TryParse(args[1],out pid)||pid<=0)return 64;
  using(var parent=Process.GetProcessById(pid)){
   ThreadPool.QueueUserWorkItem(delegate{try{while(Console.ReadLine()!=null){} }finally{stop=true;}});
   var clock=Stopwatch.StartNew();var edges=new Edge[]{new Edge(),new Edge(),new Edge(),new Edge()};var prior=new int[]{-1,-1,-1,-1};
   while(!stop&&!parent.HasExited&&clock.ElapsedMilliseconds<615000){
    for(uint i=0;i<4;i++){State state;bool connected=Read(i,out state)==0;int value=connected?1:0;
     if(prior[i]!=value){prior[i]=value;Console.WriteLine("{\"event\":\"controller\",\"slot\":"+i+",\"connected\":"+(connected?"true":"false")+"}");}
     if(edges[i].Step(connected,(state.Gamepad.Buttons&0x3010)!=0,clock.ElapsedMilliseconds)){Console.WriteLine("{\"event\":\"skip\",\"slot\":"+i+"}");return 0;}
    }Thread.Sleep(16);
   }
  }return 0;
 }catch(Exception e){Console.Error.WriteLine(e.Message);return 1;}}
}
