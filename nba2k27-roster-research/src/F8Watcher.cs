using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

// Session-only F8 polling. No hooks, input injection, game memory, or key logging.
internal static class F8Watcher {
    [DllImport("user32.dll")] static extern short GetAsyncKeyState(int key);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint pid);
    [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern bool QueryFullProcessImageName(IntPtr process, uint flags, StringBuilder name, ref uint size);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    static volatile bool stopping;
    static bool IsTarget(uint pid, int parentPid, string gamePath) {
        if (pid == parentPid) return true;
        if (pid == 0) return false;
        // Query image identity only; never request VM_READ or VM_WRITE.
        IntPtr handle = OpenProcess(0x1000, false, pid);
        if (handle == IntPtr.Zero) return false;
        try {
            var name = new StringBuilder(32768); uint size = (uint)name.Capacity;
            return QueryFullProcessImageName(handle, 0, name, ref size) &&
                String.Equals(name.ToString(), gamePath, StringComparison.OrdinalIgnoreCase);
        } finally { CloseHandle(handle); }
    }
    public static int Main(string[] args) {
        try {
            int parentPid;
            if (args.Length != 2 || !Int32.TryParse(args[0], out parentPid) || parentPid <= 0)
                throw new ArgumentException("Expected launcher PID and game root.");
            string gamePath = Path.Combine(Path.GetFullPath(args[1]), "NBA2K27.exe");
            using (var parent = Process.GetProcessById(parentPid)) {
                // Hold this exact process handle; a reused PID cannot keep us alive.
                var parentHandle = parent.Handle;
                var input = new Thread(() => { try { Console.ReadLine(); } catch { } finally { stopping = true; } });
                input.IsBackground = true; input.Start();
                bool held = GetAsyncKeyState(0x77) < 0;
                Console.WriteLine("READY"); Console.Out.Flush();
                int ticks = 0;
                while (!stopping) {
                    if (ticks++ % 25 == 0 && parent.HasExited) break;
                    bool down = GetAsyncKeyState(0x77) < 0;
                    if (down && !held) {
                        uint pid; GetWindowThreadProcessId(GetForegroundWindow(), out pid);
                        if (IsTarget(pid, parentPid, gamePath)) { Console.WriteLine("F8"); Console.Out.Flush(); }
                    }
                    held = down;
                    Thread.Sleep(20);
                }
            }
            return 0;
        } catch (Exception error) { Console.Error.WriteLine(error.Message); return 1; }
    }
}
