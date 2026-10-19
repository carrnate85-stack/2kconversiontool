# Read-only probe of the loaded VERSION.dll proxy's live file-route counters in
# the running NBA2K27.exe (PROCESS_QUERY_INFORMATION | PROCESS_VM_READ only).
# Counter RVAs come from the installed DLL's own COFF symbol table, so the
# probe follows whichever proxy build the launcher installed. Writes nothing.
param([string]$Label = 'counters')
$ErrorActionPreference = 'Stop'
$game = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27'
$dllPath = Join-Path $game 'VERSION.dll'
$names = @(
  'g_live_file_info_calls', 'g_live_stock_file_info_successes', 'g_live_local_file_info_successes',
  'g_live_actual_open_calls', 'g_live_stock_open_successes', 'g_live_local_open_successes',
  'g_live_transfer_calls', 'g_live_close_calls', 'g_provider_query_calls'
)
$bytes = [IO.File]::ReadAllBytes($dllPath)
$pe = [BitConverter]::ToInt32($bytes, 0x3C)
$symPtr = [BitConverter]::ToInt32($bytes, $pe + 12); $nSym = [BitConverter]::ToInt32($bytes, $pe + 16)
$nSec = [BitConverter]::ToUInt16($bytes, $pe + 6); $opt = [BitConverter]::ToUInt16($bytes, $pe + 20)
$imageSize = [BitConverter]::ToUInt32($bytes, $pe + 24 + 56)
$secVa = @(); for ($k = 0; $k -lt $nSec; $k++) { $secVa += [BitConverter]::ToUInt32($bytes, $pe + 24 + $opt + 40 * $k + 12) }
$strtab = $symPtr + 18 * $nSym
$rva = @{}
for ($i = 0; $i -lt $nSym; $i++) {
  $e = $symPtr + 18 * $i
  if ([BitConverter]::ToUInt32($bytes, $e) -eq 0) {
    $o = $strtab + [BitConverter]::ToUInt32($bytes, $e + 4); $end = [Array]::IndexOf($bytes, [byte]0, $o)
    $name = [Text.Encoding]::ASCII.GetString($bytes, $o, $end - $o)
  } else { $name = [Text.Encoding]::ASCII.GetString($bytes, $e, 8).TrimEnd([char]0) }
  $sec = [BitConverter]::ToInt16($bytes, $e + 12)
  foreach ($n in $names + @('g_forced_override_path')) {
    if ($sec -gt 0 -and $name -match "\d$n`E$") { $rva[$n] = $secVa[$sec - 1] + [BitConverter]::ToUInt32($bytes, $e + 8) }
  }
  $i += $bytes[$e + 17]
}
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class ProxyReader {
  [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] public static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("psapi.dll")] public static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] public static extern uint GetModuleFileNameExW(IntPtr h, IntPtr m, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct ModInfo { public IntPtr Base; public uint Size; public IntPtr Entry; }
  [DllImport("psapi.dll")] public static extern bool GetModuleInformation(IntPtr h, IntPtr m, out ModInfo i, int cb);
}
"@
$p = @(Get-Process NBA2K27 -ErrorAction Stop)[0]
$h = [ProxyReader]::OpenProcess(0x0400 -bor 0x0010, $false, $p.Id)
if ($h -eq [IntPtr]::Zero) { throw 'OpenProcess (read-only) failed' }
try {
  $mods = New-Object IntPtr[] 1024; $need = 0
  if (-not [ProxyReader]::EnumProcessModulesEx($h, $mods, 8 * 1024, [ref]$need, 3)) { throw 'Module query failed' }
  $base = [IntPtr]::Zero
  foreach ($m in $mods[0..([Math]::Min(1023, $need / 8 - 1))]) {
    $sb = New-Object Text.StringBuilder 520
    [void][ProxyReader]::GetModuleFileNameExW($h, $m, $sb, 520)
    if ($sb.ToString() -ieq $dllPath) {
      $info = New-Object ProxyReader+ModInfo
      [void][ProxyReader]::GetModuleInformation($h, $m, [ref]$info, 24)
      if ($info.Size -ne $imageSize) { throw "Loaded proxy differs from the installed DLL ($($info.Size) vs $imageSize)" }
      $base = $m
    }
  }
  if ($base -eq [IntPtr]::Zero) { throw 'VERSION.dll proxy is not loaded' }
  $read = { param($r, $n) $b = New-Object byte[] $n; $got = [IntPtr]::Zero
    if (-not [ProxyReader]::ReadProcessMemory($h, [IntPtr]($base.ToInt64() + $r), $b, [IntPtr]$n, [ref]$got)) { throw 'read failed' }; ,$b }
  $report = [ordered]@{ label = $Label; time = (Get-Date).ToString('o'); pid = $p.Id; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; writes = 0 }
  foreach ($n in $names) { if ($rva.ContainsKey($n)) { $report[$n] = [BitConverter]::ToUInt32((& $read $rva[$n] 4), 0) } }
  $s = & $read $rva['g_forced_override_path'] 261
  $report['forced_override'] = [Text.Encoding]::ASCII.GetString($s, 0, [Math]::Max(0, [Array]::IndexOf($s, [byte]0)))
  $report | ConvertTo-Json
} finally { [void][ProxyReader]::CloseHandle($h) }
