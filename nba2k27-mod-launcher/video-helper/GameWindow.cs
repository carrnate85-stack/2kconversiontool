using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
// Read-only readiness observation: no focus changes, hooks or game memory access.
class GameWindow {
 delegate bool EnumProc(IntPtr window,IntPtr parameter);
 [StructLayout(LayoutKind.Sequential)]struct Rect{public int Left,Top,Right,Bottom;}
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumProc callback,IntPtr parameter);
 [DllImport("user32.dll")]static extern bool IsWindowVisible(IntPtr window);
 [DllImport("user32.dll")]static extern bool IsIconic(IntPtr window);
 [DllImport("user32.dll")]static extern bool GetClientRect(IntPtr window,out Rect rect);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr window,out uint pid);
 [DllImport("user32.dll")]static extern IntPtr GetWindow(IntPtr window,uint command);
 static volatile bool stop;
 static IntPtr VisibleWindow(uint pid){IntPtr found=IntPtr.Zero;
  EnumWindows(delegate(IntPtr window,IntPtr parameter){uint owner;Rect rect;GetWindowThreadProcessId(window,out owner);
   if(owner==pid&&IsWindowVisible(window)&&!IsIconic(window)&&GetWindow(window,4)==IntPtr.Zero&&GetClientRect(window,out rect)&&rect.Right-rect.Left>=640&&rect.Bottom-rect.Top>=360){found=window;return false;}return true;
  },IntPtr.Zero);return found;
 }
 static int Main(string[] args){try{
  int parentId;if(args.Length!=3||args[0]!="--wait"||!int.TryParse(args[1],out parentId)||parentId<=0)return 64;
  string expected=Path.GetFullPath(args[2]);if(!String.Equals(Path.GetFileName(expected),"NBA2K27.exe",StringComparison.OrdinalIgnoreCase))return 64;
  ThreadPool.QueueUserWorkItem(delegate{try{while(Console.ReadLine()!=null){}}finally{stop=true;}});
  using(var parent=Process.GetProcessById(parentId)){
   var clock=Stopwatch.StartNew();Process game=null;IntPtr prior=IntPtr.Zero;long stableAt=0;
   try{while(!stop&&!parent.HasExited&&clock.ElapsedMilliseconds<120000){
    if(game==null)foreach(var candidate in Process.GetProcessesByName("NBA2K27")){
     try{if(game==null&&String.Equals(Path.GetFullPath(candidate.MainModule.FileName),expected,StringComparison.OrdinalIgnoreCase)){game=candidate;continue;}}catch{}candidate.Dispose();
    }
    if(game!=null){
     if(game.HasExited){Console.WriteLine("{\"event\":\"game-exited\"}");return 0;}
     IntPtr current=VisibleWindow((uint)game.Id);
     if(current!=prior){prior=current;stableAt=clock.ElapsedMilliseconds;}
     if(current!=IntPtr.Zero&&clock.ElapsedMilliseconds-stableAt>=250){Console.WriteLine("{\"event\":\"window-ready\",\"pid\":"+game.Id+"}");return 0;}
    }Thread.Sleep(50);
   }}finally{if(game!=null)game.Dispose();}
   Console.WriteLine(stop?"{\"event\":\"cancelled\"}":"{\"event\":\"timeout\"}");
  }return 0;
 }catch(Exception error){Console.Error.WriteLine(error.Message);return 1;}}
}
