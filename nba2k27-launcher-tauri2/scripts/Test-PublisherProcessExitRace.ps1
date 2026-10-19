# Offline API reproduction only: starts one own short-lived PowerShell child,
# retains its query/synchronize handle, and observes image-name query failure
# after normal exit. No Main, game, kill, or publisher invocation.
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public static class PublisherExitProbe {
    [DllImport("kernel32.dll",SetLastError=true)]
    public static extern SafeProcessHandle OpenProcess(uint access,bool inherit,uint pid);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
    static extern bool QueryFullProcessImageNameW(SafeProcessHandle process,uint flags,StringBuilder name,ref uint length);
    [DllImport("kernel32.dll",SetLastError=true)]
    public static extern uint WaitForSingleObject(SafeProcessHandle process,uint timeout);
    [DllImport("kernel32.dll",SetLastError=true)]
    static extern SafeFileHandle CreateToolhelp32Snapshot(uint flags,uint pid);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
    static extern bool Process32FirstW(SafeFileHandle snapshot,ref Entry entry);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,SetLastError=true)]
    static extern bool Process32NextW(SafeFileHandle snapshot,ref Entry entry);
    [StructLayout(LayoutKind.Sequential,CharSet=CharSet.Unicode)]
    struct Entry {
        public uint Size,Usage,Pid;
        public UIntPtr Heap;
        public uint Module,Threads,Parent;
        public int Priority;
        public uint Flags;
        [MarshalAs(UnmanagedType.ByValTStr,SizeConst=260)] public string Name;
    }
    public static bool FreshPidPresent(uint pid) {
        using(var snapshot=CreateToolhelp32Snapshot(2,0)) {
            if(snapshot.IsInvalid)throw new Win32Exception(Marshal.GetLastWin32Error());
            var row=new Entry{Size=(uint)Marshal.SizeOf(typeof(Entry))};
            if(!Process32FirstW(snapshot,ref row))throw new Win32Exception(Marshal.GetLastWin32Error());
            do{if(row.Pid==pid)return true;}while(Process32NextW(snapshot,ref row));
            int error=Marshal.GetLastWin32Error();if(error!=18)throw new Win32Exception(error);
            return false;
        }
    }
    public static int Query(SafeProcessHandle process) {
        var name=new StringBuilder(32768);uint count=32768;
        return QueryFullProcessImageNameW(process,0,name,ref count)?0:Marshal.GetLastWin32Error();
    }
}
'@
$taskPwsh=(Get-Command pwsh -CommandType Application -ErrorAction Stop).Source
$taskChild=Start-Process -FilePath $taskPwsh -ArgumentList @('-NoLogo','-NoProfile','-NonInteractive','-Command','Start-Sleep -Milliseconds 1000') -WindowStyle Hidden -PassThru
$taskHandle=[PublisherExitProbe]::OpenProcess(0x101000,$false,$taskChild.Id)
try {
    if($taskHandle.IsInvalid){throw 'Could not retain fixture child'}
    $taskAlive=[PublisherExitProbe]::WaitForSingleObject($taskHandle,0)
    $taskAliveQuery=[PublisherExitProbe]::Query($taskHandle)
    if(-not $taskChild.WaitForExit(10000)){throw 'Own fixture did not exit naturally; it was not killed'}
    $taskExited=[PublisherExitProbe]::WaitForSingleObject($taskHandle,0)
    $taskExitedQuery=[PublisherExitProbe]::Query($taskHandle)
    $taskFresh=[PublisherExitProbe]::FreshPidPresent($taskChild.Id)
    if($taskAlive -ne 258 -or $taskAliveQuery -ne 0 -or $taskExited -ne 0 -or $taskExitedQuery -ne 31 -or $taskFresh){throw 'Expected exact retained-exit/fresh-absence reproduction was not observed'}
    [ordered]@{pass=$true;liveQueryError=$taskAliveQuery;retainedLiveWait=$taskAlive;exitedQueryError=$taskExitedQuery;retainedExitedWait=$taskExited;freshPidPresent=$taskFresh;childExitedNaturally=$true;mainAccess=$false;gameAccess=$false;publisherInvoked=$false}|ConvertTo-Json
} finally {$taskHandle.Dispose();$taskChild.Dispose()}
