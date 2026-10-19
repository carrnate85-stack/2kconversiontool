# Read-only Version 2 Freestyle music-event probe. Opens NBA2K27.exe with
# QUERY_INFORMATION | VM_READ only: no write, operation, thread or injection
# rights. Nothing is written to the game. Samples the bank-34 music-event
# object WPF's freestyle_redirect read (V2 0xA31E7D0, see
# tools/jukebox_v2_freestyle_cover_audit.py) and the event-name key table.
# Run it while Freestyle music is audible; output JSON goes to artifacts.
param(
  [int]$Seconds = 90,
  [int]$IntervalMs = 250,
  [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) ("artifacts/v2-freestyle-music-event-{0}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss')))
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public sealed class FreestyleEventReader : IDisposable {
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  IntPtr h; public ulong Base;
  public FreestyleEventReader(int pid) {
    h = OpenProcess(0x0400 | 0x0010, false, pid); // QUERY_INFORMATION | VM_READ
    if (h == IntPtr.Zero) throw new Exception("OpenProcess (read-only) failed");
    var mods = new IntPtr[1]; int need;
    if (!EnumProcessModulesEx(h, mods, IntPtr.Size, out need, 3)) throw new Exception("Module query failed");
    Base = (ulong)mods[0].ToInt64();
  }
  public byte[] Read(ulong at, int n) {
    if (at < 0x10000 || at > 0x7fffffffffffUL || n < 1 || n > 65536) return null;
    var b = new byte[n]; IntPtr r;
    return ReadProcessMemory(h, (IntPtr)(long)at, b, (IntPtr)n, out r) && r.ToInt64() == n ? b : null;
  }
  public void Dispose() { if (h != IntPtr.Zero) CloseHandle(h); h = IntPtr.Zero; }
}
"@
$EVENT = 0x0A31E7D0; $KEYS = 0x0677F930; $ANCHOR = 0x0025C4A7
$ANCHOR_BYTES = '488d0d22230c0a'
$game = @(Get-Process NBA2K27 -ErrorAction Stop)
if ($game.Count -ne 1) { throw "Expected exactly one NBA2K27 process" }
$r = New-Object FreestyleEventReader($game[0].Id)
try {
  $base = $r.Base
  $hdr = $r.Read($base, 4096); $pe = [BitConverter]::ToInt32($hdr, 0x3C)
  $stamp = [BitConverter]::ToUInt32($hdr, $pe + 8); $size = [BitConverter]::ToUInt32($hdr, $pe + 80)
  if ($stamp -ne 0x6AC53765 -or $size -ne 0x35BDE000) { throw ("Not the pinned Version 2 build ({0:X8}/{1:X8})" -f $stamp, $size) }
  $anchor = -join ($r.Read($base + $ANCHOR, 7) | ForEach-Object { $_.ToString('x2') })
  if ($anchor -ne $ANCHOR_BYTES) { throw "Anchor bytes differ ($anchor); stop" }
  $key992 = [BitConverter]::ToUInt32($r.Read($base + $KEYS + 992 * 4, 4), 0)
  $states = [ordered]@{}; $samples = 0
  $deadline = [DateTime]::UtcNow.AddSeconds($Seconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    $e = $r.Read($base + $EVENT, 0x150)
    if ($e) {
      $samples++
      $index = [BitConverter]::ToUInt32($e, 0x40); $flag = $e[0x44]; $active = $e[0x10]
      $bank = [BitConverter]::ToUInt32($r.Read($base + $EVENT + 0x14C, 4), 0)
      $keyNow = if ($index -lt 0x1957) { '0x{0:X8}' -f [BitConverter]::ToUInt32($r.Read($base + $KEYS + $index * 4, 4), 0) } else { $null }
      $id = "{0}|{1}|{2}|{3}" -f $index, $flag, $active, $bank
      if (-not $states.Contains($id)) {
        $states[$id] = [ordered]@{ index = $index; flag44 = $flag; active10 = $active; bank14C = $bank; indexKey = $keyNow
          firstUtc = [DateTime]::UtcNow.ToString('o'); count = 0; raw = (-join ($e | ForEach-Object { $_.ToString('x2') })) }
      }
      $states[$id].count++
    }
    Start-Sleep -Milliseconds $IntervalMs
  }
  $result = [ordered]@{
    readOnly = $true; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; pid = $game[0].Id; capturedUtc = [DateTime]::UtcNow.ToString('o')
    eventRva = '0x{0:X}' -f $EVENT; keysRva = '0x{0:X}' -f $KEYS; anchor = $anchor
    key992 = '0x{0:X8}' -f $key992; key992Expected = '0xFAB3211A'
    samples = $samples; states = @($states.Values)
    freestyleSeen = [bool](@($states.Values | Where-Object { $_.index -eq 992 -and $_.flag44 -eq 1 -and $_.bank14C -eq 34 }).Count)
  }
  New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
  $result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $Out -Encoding utf8
  "Wrote $Out"
  $result.states = @($result.states | ForEach-Object { $s = [ordered]@{}; foreach ($k in $_.Keys) { if ($k -ne 'raw') { $s[$k] = $_[$k] } }; $s })
  $result | ConvertTo-Json -Depth 4
} finally { $r.Dispose() }
