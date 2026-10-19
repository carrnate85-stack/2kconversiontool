using System;
using System.Runtime.InteropServices;
using System.Collections.Generic;

// Executes the real emitted stub against synthetic memory in THIS test process.
// No game process is opened or modified.
internal static class BallSelectorNativeTests {
 [DllImport("kernel32.dll")] static extern IntPtr VirtualAlloc(IntPtr at,UIntPtr size,uint allocation,uint protection);
 [DllImport("kernel32.dll")] static extern bool VirtualFree(IntPtr at,UIntPtr size,uint kind);
 [DllImport("kernel32.dll")] static extern bool VirtualProtect(IntPtr at,UIntPtr size,uint protection,out uint old);
 [DllImport("kernel32.dll")] static extern bool FlushInstructionCache(IntPtr process,IntPtr at,UIntPtr size);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [UnmanagedFunctionPointer(CallingConvention.Winapi)] delegate uint Invoke(IntPtr ball,IntPtr stub);
 [UnmanagedFunctionPointer(CallingConvention.Winapi)] delegate ulong Lifetime(uint model,uint source,uint line);
 static IntPtr At(IntPtr start,long offset){return new IntPtr(start.ToInt64()+offset);}
 static void Ensure(bool value){if(!value)throw new Exception("Native ball-stub test failed.");}
 public static void Run(){
  IntPtr image=IntPtr.Zero,data=IntPtr.Zero,code=IntPtr.Zero;
  try{
   image=VirtualAlloc(IntPtr.Zero,(UIntPtr)0xB000000,0x2000,4);
   data=VirtualAlloc(IntPtr.Zero,(UIntPtr)0x4000,0x3000,4);
   code=VirtualAlloc(IntPtr.Zero,(UIntPtr)0x1000,0x3000,4);
   Ensure(image!=IntPtr.Zero&&data!=IntPtr.Zero&&code!=IntPtr.Zero);
   foreach(uint rva in new[]{BallSelectorPolicy.ModeTableRva,BallSelectorPolicy.SettingsRva,BallSelectorPolicy.BlacktopStageRva})
    Ensure(VirtualAlloc(At(image,rva&0xfffff000U),(UIntPtr)0x1000,0x1000,4)!=IntPtr.Zero);
   Ensure(VirtualAlloc(At(image,BallSelectorPolicy.RendererRva&0xfffff000U),(UIntPtr)0x2000,0x1000,4)!=IntPtr.Zero);
   IntPtr settings=At(data,0x1000),control=At(data,0x3000);
   Marshal.WriteInt32(At(data,0x34c),unchecked((int)0xCB5691BE));
   Marshal.WriteInt64(At(image,BallSelectorPolicy.SettingsRva),settings.ToInt64());
   Marshal.WriteInt32(control,unchecked((int)0xD1C5F442));
   Marshal.WriteInt32(At(control,4),4);
   IntPtr mesh=At(image,BallSelectorPolicy.RendererRva+0x38+4*48);
   Marshal.WriteInt64(mesh,0x12345678);
   byte[] stub=BallSelectorPolicy.Stub((ulong)control.ToInt64(),(ulong)image.ToInt64());
   // Preserve R14, provide the expected AI_BALL pointer, call the stub, return ECX.
   byte[] wrapper={0x41,0x56,0x49,0x89,0xCE,0x48,0x83,0xEC,0x20,0xFF,0xD2,0x89,0xC8,0x48,0x83,0xC4,0x20,0x41,0x5E,0xC3};
   Marshal.Copy(stub,0,code,stub.Length);Marshal.Copy(wrapper,0,At(code,0x800),wrapper.Length);
   uint old;Ensure(VirtualProtect(code,(UIntPtr)0x1000,0x20,out old));Ensure(FlushInstructionCache(GetCurrentProcess(),code,(UIntPtr)0x1000));
   var invoke=(Invoke)Marshal.GetDelegateForFunctionPointer(At(code,0x800),typeof(Invoke));
   foreach(uint hash in new uint[]{0xB4D37B22,0x3EEF98B9,0x7C602DF8,0x50BAD898}){
    Marshal.WriteInt32(At(image,BallSelectorPolicy.ModeTableRva),unchecked((int)hash));
    Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),1);
    foreach(int mode in new[]{0,1,2,3,4,5,6,7,8}){
     Marshal.WriteInt32(At(settings,BallSelectorPolicy.GameModeOffset),mode);
     Ensure(invoke(data,code)==(mode==6?0xD1C5F442U:0xCB5691BEU));
    }
   }
   Marshal.WriteInt32(At(image,BallSelectorPolicy.ModeTableRva),unchecked((int)0x92DF06DE));
   Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),3);
   Marshal.WriteInt32(At(settings,BallSelectorPolicy.GameModeOffset),0);
   Ensure(invoke(data,code)==0xD1C5F442U);
   Marshal.WriteInt64(mesh,0);Ensure(invoke(data,code)==0xCB5691BEU);
   Marshal.WriteInt64(mesh,0x12345678);Ensure(invoke(data,code)==0xD1C5F442U);
   Marshal.WriteInt32(At(control,4),500);Ensure(invoke(data,code)==0xCB5691BEU);
   Marshal.WriteInt32(At(control,4),4);
   Marshal.WriteInt32(control,0);Ensure(invoke(data,code)==0xCB5691BEU);
   Marshal.WriteInt32(control,unchecked((int)0xD1C5F442));
   Marshal.WriteInt32(At(image,BallSelectorPolicy.ModeTableRva),unchecked((int)0xB4D37B22));
   Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),1);
   Marshal.WriteInt64(At(image,BallSelectorPolicy.SettingsRva),0);
   Ensure(invoke(data,code)==0xCB5691BEU);
   TestLifetime(image,settings,control);
  }finally{
   foreach(IntPtr allocation in new[]{code,data,image})if(allocation!=IntPtr.Zero)VirtualFree(allocation,UIntPtr.Zero,0x8000);
  }
 }
 static void TestLifetime(IntPtr image,IntPtr settings,IntPtr control){
  var loaded=new List<uint>();var released=new List<uint>();
  Lifetime load=(model,source,line)=>{Ensure(source==0xFD8F114A&&line==0x383);loaded.Add(model);return 0x1234UL;};
  Lifetime release=(model,source,line)=>{Ensure(source==0xFD8F114A&&line==0x3D1);released.Add(model);return 0x5678UL;};
  IntPtr code=VirtualAlloc(IntPtr.Zero,(UIntPtr)0x1000,0x3000,4);
  try{
   Ensure(code!=IntPtr.Zero);
   foreach(var pair in new[]{Tuple.Create(BallSelectorPolicy.LoadFunctionRva,load),Tuple.Create(BallSelectorPolicy.ReleaseFunctionRva,release)}){
    IntPtr page=At(image,pair.Item1&0xfffff000U);Ensure(VirtualAlloc(page,(UIntPtr)0x1000,0x1000,4)!=IntPtr.Zero);
    var bridge=new List<byte>(new byte[]{0x48,0xB8});bridge.AddRange(BitConverter.GetBytes(Marshal.GetFunctionPointerForDelegate(pair.Item2).ToInt64()));bridge.AddRange(new byte[]{0xFF,0xE0});
    Marshal.Copy(bridge.ToArray(),0,At(image,pair.Item1),bridge.Count);uint old;Ensure(VirtualProtect(page,(UIntPtr)0x1000,0x20,out old));
    Ensure(FlushInstructionCache(GetCurrentProcess(),page,(UIntPtr)0x1000));
   }
   byte[] loadCode=BallSelectorPolicy.LifetimeStub((ulong)control.ToInt64(),(ulong)image.ToInt64(),false),releaseCode=BallSelectorPolicy.LifetimeStub((ulong)control.ToInt64(),(ulong)image.ToInt64(),true);
   Marshal.Copy(loadCode,0,code,loadCode.Length);Marshal.Copy(releaseCode,0,At(code,0x800),releaseCode.Length);
   uint protection;Ensure(VirtualProtect(code,(UIntPtr)0x1000,0x20,out protection));Ensure(FlushInstructionCache(GetCurrentProcess(),code,(UIntPtr)0x1000));
   var invokeLoad=(Lifetime)Marshal.GetDelegateForFunctionPointer(code,typeof(Lifetime));var invokeRelease=(Lifetime)Marshal.GetDelegateForFunctionPointer(At(code,0x800),typeof(Lifetime));
   Marshal.WriteInt64(At(image,BallSelectorPolicy.SettingsRva),settings.ToInt64());
   const uint native=0xCB5691BE,selected=0xB58EF870;
   foreach(int mode in new[]{0,4,6,7}){
    Marshal.WriteInt32(At(settings,BallSelectorPolicy.GameModeOffset),mode);
    Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),2);
    Marshal.WriteInt32(control,unchecked((int)selected));Marshal.WriteInt32(At(control,8),0);
    loaded.Clear();released.Clear();Ensure(invokeLoad(native,0xFD8F114A,0x383)==0x1234);
    Ensure(loaded.Count==(mode==6?2:1)&&loaded[0]==native);
    if(mode==6){Ensure(loaded[1]==selected);Ensure(unchecked((uint)Marshal.ReadInt32(At(control,8)))==selected);}
    // Selection and mode can change before release. Drain the captured ID once.
    Marshal.WriteInt32(control,0);Marshal.WriteInt32(At(settings,BallSelectorPolicy.GameModeOffset),4);
    Ensure(invokeRelease(native,0xFD8F114A,0x3D1)==0x5678);
    Ensure(released.Count==(mode==6?2:1)&&released[0]==native);
    if(mode==6)Ensure(released[1]==selected);
    Ensure(Marshal.ReadInt32(At(control,8))==0);invokeRelease(native,0xFD8F114A,0x3D1);Ensure(released.Count==(mode==6?3:2));
   }
   Marshal.WriteInt32(At(settings,BallSelectorPolicy.GameModeOffset),6);
   foreach(uint selectedId in new[]{0U,native}){
    loaded.Clear();Marshal.WriteInt32(control,unchecked((int)selectedId));invokeLoad(native,0xFD8F114A,0x383);Ensure(loaded.Count==1&&Marshal.ReadInt32(At(control,8))==0);
   }
   Marshal.WriteInt32(control,unchecked((int)selected));Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),0);
   loaded.Clear();invokeLoad(native,0xFD8F114A,0x383);Ensure(loaded.Count==1);
   Marshal.WriteInt32(At(image,BallSelectorPolicy.BlacktopStageRva),2);
   loaded.Clear();invokeLoad(native,0xFD8F114A,0x383);invokeLoad(native,0xFD8F114A,0x383);Ensure(loaded.Count==3);
   GC.KeepAlive(load);GC.KeepAlive(release);
  }finally{if(code!=IntPtr.Zero)VirtualFree(code,UIntPtr.Zero,0x8000);}
 }
}
