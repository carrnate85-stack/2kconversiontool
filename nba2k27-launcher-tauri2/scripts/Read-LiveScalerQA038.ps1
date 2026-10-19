$ErrorActionPreference = 'Stop'
# Diagnostic only: PROCESS_VM_READ + QUERY_INFORMATION. No write/protect APIs.
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class ScaleRead038 {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint access, bool inherit, int pid);
 [DllImport("kernel32.dll", SetLastError=true)] static extern bool ReadProcessMemory(IntPtr h, IntPtr p, byte[] b, UIntPtr n, out UIntPtr read);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
 public static byte[] Read(IntPtr h, long p, int n) {
  if(p<65536 || n<1 || n>8192) throw new Exception("Invalid bounded read");
  var b=new byte[n]; UIntPtr read;
  if(!ReadProcessMemory(h,new IntPtr(p),b,(UIntPtr)n,out read) || read.ToUInt64()!=(ulong)n) throw new Win32Exception();
  return b;
 }
 public static long Q(IntPtr h,long p) {return BitConverter.ToInt64(Read(h,p,8),0);}
}
'@
$project = Split-Path $PSScriptRoot -Parent
$journal = Get-Content -LiteralPath (Join-Path $project 'Builds/0.38.0/Portable/Data/session-journal.json') -Raw | ConvertFrom-Json
$game = Get-Process -Id $journal.game.pid
if ($game.Path -ne $journal.game.path -or $game.StartTime.ToUniversalTime().ToFileTimeUtc().ToString() -ne $journal.game.started) { throw 'Game identity mismatch' }
$base = $game.MainModule.BaseAddress.ToInt64()
$handle = [ScaleRead038]::OpenProcess(0x410,$false,$game.Id)
if ($handle -eq [IntPtr]::Zero) { throw 'Read-only game access failed' }
try {
 $head = [ScaleRead038]::Read($handle,$base,4096)
 $pe = [BitConverter]::ToInt32($head,60)
 if ($pe -lt 64 -or $pe -gt 3800 -or [BitConverter]::ToUInt32($head,$pe+8) -ne 0x6ac53765 -or [BitConverter]::ToUInt32($head,$pe+80) -ne 0x35bde000) { throw 'Not verified Version 2 image' }
 $input = [ScaleRead038]::Read($handle,($base+0x0a41d07c),4)
 $owner = [ScaleRead038]::Q($handle,($base+0x0ab5ce20))
 $mode = [BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($owner+0xa04),4),0)
 $table = [ScaleRead038]::Read($handle,($base+0x0ab5ce30),2400)
 $current=[BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($base+0xAD07BD0),4),0)
 $alternate=[BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($base+0xAD07BCC),4),0)
 [ordered]@{diagnostic='live-header-and-mode';actualBase=('{0:X}' -f $base);headerImageBase=('{0:X}' -f [BitConverter]::ToUInt64($head,$pe+48));currentIndex=$current;alternateIndex=$alternate;currentMode=$(if($current-lt 128){'{0:X8}' -f [BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($base+0xAD07408+$current*64),4),0)});alternateMode=$(if($alternate-lt 128){'{0:X8}' -f [BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($base+0xAD07408+$alternate*64),4),0)})} | ConvertTo-Json
 $rows = @()
 if ($current -lt 128 -and [BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($base+0xAD07408+$current*64),4),0) -eq 0xF54D5D12L) {
  $rosterRoot=[ScaleRead038]::Q($handle,($base+0x9ACE338))
  $rosterOwner=[ScaleRead038]::Q($handle,($rosterRoot+0xA8))
  $rosterHeader=[ScaleRead038]::Read($handle,($rosterOwner+0x80),16)
  $teamCount=[BitConverter]::ToUInt32($rosterHeader,0)
  $teamTable=[BitConverter]::ToInt64($rosterHeader,8)
  if($teamCount -lt 1 -or $teamCount -gt 1024) { throw 'Unbounded roster refused' }
  $kinds=@{}; $relevant=@()
  for($t=0; $t -lt $teamCount; $t++) {
   $team=[ScaleRead038]::Read($handle,($teamTable+$t*0x1550),0x1550)
   $id=[BitConverter]::ToUInt16($team,0xF20)
   $kind=[BitConverter]::ToUInt32($team,0x123C)-shr 26
   $kinds["$kind"]++
   if($kind -eq 29 -or $id -in @(69,361) -or ($id-ge 300 -and $id-le 317)) {
    $name=[Text.Encoding]::Unicode.GetString($team,0x2FA,48).Split([char]0)[0]
    $relevant += [ordered]@{id=$id;kind=$kind;name=$name}
   }
  }
  if([Convert]::ToBase64String($rosterHeader) -ne [Convert]::ToBase64String([ScaleRead038]::Read($handle,($rosterOwner+0x80),16)) -or [ScaleRead038]::Q($handle,($base+0x9ACE338)) -ne $rosterRoot -or [ScaleRead038]::Q($handle,($rosterRoot+0xA8)) -ne $rosterOwner) { throw 'Roster changed during diagnostic' }
  [ordered]@{diagnostic='freestyle-roster-categories';count=$teamCount;kinds=$kinds;teams=$relevant} | ConvertTo-Json -Depth 5
 }
 for ($i=0; $i -lt 30; $i++) {
  $at=$i*80
  $vt=[BitConverter]::ToInt64($table,$at)
  $manager=[BitConverter]::ToInt64($table,$at+16)
  $roster=[BitConverter]::ToInt64($table,$at+40)
  if ($manager -eq 0 -and $roster -eq 0) { continue }
  $tableManager=$manager
  # Disk RTTI proof: MANAGER_INSTANCE's +0x78 interface vtable.
  # Normalize only for this read-only diagnostic, then verify primary identity.
  if ([ScaleRead038]::Q($handle,$manager) -eq ($base+0x06884898)) { $manager-=0x78 }
  if ($vt -ne ($base+0x0762d278) -or [ScaleRead038]::Q($handle,$manager) -ne ($base+0x068844d8)) { [ordered]@{slot=$i;wrapperRva=('{0:X}' -f ($vt-$base));managerRva=('{0:X}' -f ([ScaleRead038]::Q($handle,$manager)-$base));sharedInput=[BitConverter]::ToSingle($input,0);mode=$mode} | ConvertTo-Json; throw "Unknown wrapper/manager at slot $i" }
  $actor=[ScaleRead038]::Q($handle,($manager+0x110))
  if ([ScaleRead038]::Q($handle,$actor) -eq ($base+0x06890d88) -and [ScaleRead038]::Q($handle,($actor+0x78)) -eq ($base+0x06891598)) {
   $observedPair=[ScaleRead038]::Read($handle,($actor+0xbcc),8)
   [ordered]@{diagnostic='verified-player-object-scale-pair';slot=$i;scale=[BitConverter]::ToSingle($observedPair,0);inverse=[BitConverter]::ToSingle($observedPair,4)} | ConvertTo-Json
  }
  if ([ScaleRead038]::Q($handle,$actor) -ne $base+0x06890d88 -or [ScaleRead038]::Q($handle,($actor+0x78)) -ne $base+0x06891598 -or [ScaleRead038]::Q($handle,($actor+0xf30)) -ne $base+0x068920c8 -or [ScaleRead038]::Q($handle,($roster+0x518)) -notin @($manager,$tableManager)) { [ordered]@{slot=$i;actorVtableRva=('{0:X}' -f ([ScaleRead038]::Q($handle,$actor)-$base));interfaceVtableRva=('{0:X}' -f ([ScaleRead038]::Q($handle,($actor+0x78))-$base));descriptionVtableRva=('{0:X}' -f ([ScaleRead038]::Q($handle,($actor+0xf30))-$base));backreferenceMatches=([ScaleRead038]::Q($handle,($roster+0x518)) -in @($manager,$tableManager));sharedInput=[BitConverter]::ToSingle($input,0)} | ConvertTo-Json; throw "Unknown player identity at slot $i" }
  $pair=[ScaleRead038]::Read($handle,($actor+0xbcc),8)
  $flag=[ScaleRead038]::Read($handle,($actor+0xf30+0x7d),1)[0]
  if ([ScaleRead038]::Q($handle,($manager+0x110)) -ne $actor) { throw 'Player changed during read' }
  $rows += [ordered]@{slot=$i;scale=[BitConverter]::ToSingle($pair,0);inverse=[BitConverter]::ToSingle($pair,4);sharedInputOptIn=(($flag-band 2)-ne 0)}
 }
 $after=[ScaleRead038]::Read($handle,($base+0x0ab5ce30),2400)
 if ([Convert]::ToBase64String($table) -ne [Convert]::ToBase64String($after) -or [ScaleRead038]::Q($handle,($base+0x0ab5ce20)) -ne $owner -or [BitConverter]::ToUInt32([ScaleRead038]::Read($handle,($owner+0xa04),4),0) -ne $mode -or [Convert]::ToBase64String($input) -ne [Convert]::ToBase64String([ScaleRead038]::Read($handle,($base+0x0a41d07c),4))) { throw 'State changed; snapshot rejected' }
 $game.Refresh()
 if($game.HasExited) { throw 'Game exited during snapshot' }
 [ordered]@{readOnly=$true;pid=$game.Id;mode=$mode;requestedPercent=97;sharedInput=[BitConverter]::ToSingle($input,0);sharedInputBits=('{0:X8}' -f [BitConverter]::ToUInt32($input,0));players=$rows} | ConvertTo-Json -Depth 5
} finally { [void][ScaleRead038]::CloseHandle($handle) }
