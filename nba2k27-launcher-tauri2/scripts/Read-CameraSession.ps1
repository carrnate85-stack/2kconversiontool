param([string]$PortableRoot = (Join-Path (Split-Path $PSScriptRoot -Parent) 'Builds/0.53.0/Portable'))
$ErrorActionPreference = 'Stop'
# File-selected session identity, bounded reads, QUERY_INFORMATION | VM_READ only.
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class CameraReadOnly {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
 [DllImport("kernel32.dll", SetLastError=true)] static extern bool ReadProcessMemory(IntPtr h, IntPtr p, byte[] b, UIntPtr n, out UIntPtr read);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
 public static byte[] Read(IntPtr h, long at, int count) {
  if(at<65536 || count<1 || count>8192) throw new Exception("Invalid bounded read");
  byte[] b=new byte[count]; UIntPtr done;
  if(!ReadProcessMemory(h,new IntPtr(at),b,(UIntPtr)count,out done) || done.ToUInt64()!=(ulong)count) throw new Win32Exception();
  return b;
 }
 public static uint D(IntPtr h,long at) { return BitConverter.ToUInt32(Read(h,at,4),0); }
 public static long Q(IntPtr h,long at) { return BitConverter.ToInt64(Read(h,at,8),0); }
}
'@
$journalPath = Join-Path $PortableRoot 'Data/native-v2-features-session.jsonl'
$stream = [IO.File]::Open($journalPath,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
$reader = [IO.StreamReader]::new($stream)
try { $events = @($reader.ReadToEnd().Split("`n",[StringSplitOptions]::RemoveEmptyEntries) | ForEach-Object { $_ | ConvertFrom-Json }) } finally { $reader.Dispose() }
$identity = $events[0].game
$game = Get-Process -Id $identity.pid
if ($game.Path -ne $identity.path -or $game.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -ne $identity.started) { throw 'Game identity mismatch' }
$base = $game.MainModule.BaseAddress.ToInt64()
$handle = [CameraReadOnly]::OpenProcess(0x410,$false,$game.Id)
if ($handle -eq [IntPtr]::Zero) { throw 'Read-only access failed' }
try {
 $header = [CameraReadOnly]::Read($handle,$base,4096)
 $pe = [BitConverter]::ToInt32($header,60)
 if ($pe -lt 64 -or $pe -gt 3800 -or [BitConverter]::ToUInt32($header,$pe+8) -ne 0x6ac53765 -or [BitConverter]::ToUInt32($header,$pe+80) -ne 0x35bde000) { throw 'Not verified Version 2 image' }
 $snapshots = @()
 for ($sample=0; $sample -lt 3; $sample++) {
  $current = [CameraReadOnly]::D($handle,$base+0xAD07BD0)
  $alternate = [CameraReadOnly]::D($handle,$base+0xAD07BCC)
  if ($current -ge 128 -or $alternate -ge 128) { throw 'Transitioning mode index' }
  $owner = [CameraReadOnly]::Q($handle,$base+0xAB5CE20)
  $table = [CameraReadOnly]::Read($handle,$base+0xAB5CE30,2400)
  $loaded = 0
  for ($slot=0; $slot -lt 30; $slot++) {
   if ([BitConverter]::ToInt64($table,$slot*80+16) -ne 0 -or [BitConverter]::ToInt64($table,$slot*80+40) -ne 0) { $loaded++ }
  }
  $snapshots += [ordered]@{
   current=('{0:X8}' -f [CameraReadOnly]::D($handle,$base+0xAD07408+$current*64))
   alternate=('{0:X8}' -f [CameraReadOnly]::D($handle,$base+0xAD07408+$alternate*64))
   stage=[CameraReadOnly]::D($handle,$base+0xAF46870+0xEA8)
   active=[CameraReadOnly]::Read($handle,$base+0xAB5D790,1)[0]
   kind=[CameraReadOnly]::Read($handle,$owner,1)[0]
   gameMode=[CameraReadOnly]::D($handle,$owner+0xA04)
   loadedWrapperSlots=$loaded
  }
 }
 $intent = @($events | Where-Object { $_.event -eq 'write-intent' -and $_.feature -eq 'cameras' }) | Select-Object -Last 1
 $patches = @()
 foreach ($change in $intent.changes) {
  $actual = [CameraReadOnly]::Read($handle,$change.at,$change.before.Count)
  $value = [Convert]::ToHexString($actual)
  $before = [Convert]::ToHexString([byte[]]$change.before)
  $after = [Convert]::ToHexString([byte[]]$change.after)
  $patches += [ordered]@{rva=('{0:X}' -f ($change.at-$base));bytes=$actual.Length;state=$(if($value -eq $after){'after'}elseif($value -eq $before){'before'}else{'foreign'})}
 }
 $game.Refresh()
 if ($game.HasExited) { throw 'Game exited during snapshot' }
 [ordered]@{readOnly=$true;gameProcess=$game.Id;session=$events[0].session;context=$snapshots;publicChanges=$patches;gameFilesChanged=$false} | ConvertTo-Json -Depth 6
} finally { [void][CameraReadOnly]::CloseHandle($handle) }
