using System;
using System.Linq;
using System.Runtime.InteropServices;

// Execute the emitted getters only in synthetic memory in this test process.
internal static class BallRequestExperimentTests {
 [DllImport("kernel32.dll")] static extern IntPtr VirtualAlloc(IntPtr at,UIntPtr size,uint kind,uint protection);
 [DllImport("kernel32.dll")] static extern bool VirtualFree(IntPtr at,UIntPtr size,uint kind);
 [DllImport("kernel32.dll")] static extern bool VirtualProtect(IntPtr at,UIntPtr size,uint protection,out uint old);
 [DllImport("kernel32.dll")] static extern bool FlushInstructionCache(IntPtr process,IntPtr at,UIntPtr size);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [UnmanagedFunctionPointer(CallingConvention.Winapi)] delegate uint Getter();
 static IntPtr At(IntPtr p,long offset){return new IntPtr(p.ToInt64()+offset);}
 static void Check(bool value,string message){if(!value)throw new Exception(message);}
 static void Put(IntPtr p,long offset,uint value){Marshal.WriteInt32(At(p,offset),unchecked((int)value));}
 static uint Get(IntPtr p,long offset){return unchecked((uint)Marshal.ReadInt32(At(p,offset)));}
 public static int Main(){
  IntPtr image=IntPtr.Zero,data=IntPtr.Zero,code=IntPtr.Zero;
  const uint native=0xCB5691BE,selected=0xB58EF870;
  try{
   image=VirtualAlloc(IntPtr.Zero,(UIntPtr)0xB000000,0x2000,4);
   data=VirtualAlloc(IntPtr.Zero,(UIntPtr)0x3000,0x3000,4);
   code=VirtualAlloc(IntPtr.Zero,(UIntPtr)0x1000,0x3000,4);
   Check(image!=IntPtr.Zero&&data!=IntPtr.Zero&&code!=IntPtr.Zero,"allocations");
   foreach(uint rva in new[]{BallSelectorPolicy.SettingsRva,BallSelectorPolicy.BlacktopStageRva,BallSelectorPolicy.RendererRva})Check(VirtualAlloc(At(image,rva&0xfffff000U),(UIntPtr)0x3000,0x1000,4)!=IntPtr.Zero,"commit");
   IntPtr settings=data,control=At(data,0x2000),mesh=At(image,BallSelectorPolicy.RendererRva+0x38+127*48);
   Marshal.WriteInt64(At(image,BallSelectorPolicy.SettingsRva),settings.ToInt64());Put(settings,0xDA4,native);
   byte[] bytes=BallRequestExperimentPolicy.Code((ulong)control.ToInt64(),(ulong)image.ToInt64());
   foreach(uint site in BallRequestExperimentPolicy.Sites){var stub=BallRequestExperimentPolicy.Stub((ulong)control.ToInt64(),(ulong)image.ToInt64(),site);Check(stub.Take(3).SequenceEqual(new byte[]{0x9C,0x51,0x52})&&stub.Skip(stub.Length-6).SequenceEqual(new byte[]{0x89,0xC8,0x5A,0x59,0x9D,0xC3}),"register/flags save and restore");}
   Marshal.Copy(bytes,0,code,bytes.Length);uint old;Check(VirtualProtect(code,(UIntPtr)0x1000,0x20,out old),"code protection");Check(FlushInstructionCache(GetCurrentProcess(),code,(UIntPtr)0x1000),"flush");
   var load=(Getter)Marshal.GetDelegateForFunctionPointer(code,typeof(Getter));
   var release=(Getter)Marshal.GetDelegateForFunctionPointer(At(code,0x400),typeof(Getter));
   var actor=(Getter)Marshal.GetDelegateForFunctionPointer(At(code,0x800),typeof(Getter));
   Put(control,0,selected);Put(control,4,127);
   foreach(uint mode in new uint[]{0,1,2,3,4,5,7,8}){Put(settings,0xA04,mode);Put(At(image,BallSelectorPolicy.BlacktopStageRva),0,2);Check(load()==native&&actor()==native&&Get(control,28)==0,"non Blacktop pass through");}
   Put(settings,0xA04,6);
   foreach(uint stage in new uint[]{0,4,0xffffffff}){Put(At(image,BallSelectorPolicy.BlacktopStageRva),0,stage);Check(load()==native&&Get(control,28)==0,"invalid stage pass through");}
   foreach(uint stage in new uint[]{1,2,3}){
    Put(At(image,BallSelectorPolicy.BlacktopStageRva),0,stage);Put(control,0,selected);
    Check(load()==selected&&Get(control,8)==selected&&Get(control,28)==1,"native load substitution");
    Check(actor()==native,"unready mesh fallback");Marshal.WriteInt64(mesh,0x12345678);
    Check(actor()==selected&&Get(control,24)==selected,"ready actor substitution");
    Put(control,4,500);Check(actor()==native,"invalid registry index");Put(control,4,127);
    Put(control,0,0);Check(load()==selected&&Get(control,28)==2,"latch repeated request");
    Put(settings,0xA04,4);Check(actor()==native,"Freestyle actor unaffected");
    Check(release()==selected&&Get(control,28)==1,"paired first release after mode change");
    Check(release()==selected&&Get(control,28)==0&&Get(control,8)==0,"paired final release");
    Check(release()==native&&Get(control,28)==0,"no release underflow");
    Check(Get(settings,0xDA4)==native,"global ball setting unchanged");
    Marshal.WriteInt64(mesh,0);Put(settings,0xA04,6);
   }
   Check(load()==native&&Get(control,28)==0,"disabled pass through");
   Marshal.WriteInt64(At(image,BallSelectorPolicy.SettingsRva),0);Check(load()==native&&actor()==native&&release()==native,"null settings fallback");
   Console.WriteLine("Isolated ball request getter tests passed (synthetic process only).");return 0;
  }finally{foreach(IntPtr p in new[]{code,data,image})if(p!=IntPtr.Zero)VirtualFree(p,UIntPtr.Zero,0x8000);}
 }
}
