param([Parameter(Mandatory=$true)][int]$GamePid)
$ErrorActionPreference='Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class LoaderThreadReadOnly {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenThread(uint access, bool inherit, uint id);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
 [DllImport("ntdll.dll")] public static extern int NtQueryInformationThread(IntPtr thread, int kind, out IntPtr address, int bytes, IntPtr returned);
}
'@
$game=Get-Process -Id $GamePid
$module=@($game.Modules|Where-Object { $_.ModuleName -eq 'version.dll' -and $_.FileName -notlike '*\system32\*' })
if($module.Count -ne 1){throw 'Expected one game-side loader'}
$base=$module[0].BaseAddress.ToInt64()
$end=$base+$module[0].ModuleMemorySize
foreach($thread in $game.Threads){
 $handle=[LoaderThreadReadOnly]::OpenThread(0x0040,$false,[uint32]$thread.Id)
 if($handle -eq [IntPtr]::Zero){continue}
 try{
  $address=[IntPtr]::Zero
  $status=[LoaderThreadReadOnly]::NtQueryInformationThread($handle,9,[ref]$address,[IntPtr]::Size,[IntPtr]::Zero)
  if($status -eq 0 -and $address.ToInt64() -ge $base -and $address.ToInt64() -lt $end){
   [pscustomobject]@{thread=$thread.Id;loaderRva=('{0:X}' -f ($address.ToInt64()-$base));state=[string]$thread.ThreadState;wait=[string]$thread.WaitReason}|ConvertTo-Json -Compress
  }
 }finally{[void][LoaderThreadReadOnly]::CloseHandle($handle)}
}
