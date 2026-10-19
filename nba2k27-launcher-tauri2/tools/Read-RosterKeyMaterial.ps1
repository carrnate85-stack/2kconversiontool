# Read-only VCBLOWFISH key capture for offline roster research. Opens
# NBA2K27.exe with QUERY_INFORMATION | VM_READ only and writes nothing to the
# game. Reads the roster-wrapper manager object (V2 RVA 0xBB9A8E8): setter
# 0x3C78030 copies up to 0x80 key bytes to +0x58 and stores pointer/length at
# +0xD8/+0xE0. Key bytes are saved to a local .bin under
# artifacts\roster-key-offline\ and are NEVER printed; the JSON report holds
# only length and SHA-256. Keep these files out of support bundles and git.
param(
  [string]$Label = 'freestyle',
  [string]$OutDir = (Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts/roster-key-offline')
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public sealed class RosterKeyReader : IDisposable {
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  IntPtr h; public ulong Base;
  public RosterKeyReader(int pid) {
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
  public ulong Q(ulong at) { var b = Read(at, 8); return b == null ? 0 : BitConverter.ToUInt64(b, 0); }
  public void Dispose() { if (h != IntPtr.Zero) CloseHandle(h); h = IntPtr.Zero; }
}
"@
$game = @(Get-Process NBA2K27 -ErrorAction Stop)
if ($game.Count -ne 1) { throw "Expected exactly one NBA2K27 process" }
$r = New-Object RosterKeyReader($game[0].Id)
try {
  $base = $r.Base
  $hdr = $r.Read($base, 4096); $pe = [BitConverter]::ToInt32($hdr, 0x3C)
  $stamp = [BitConverter]::ToUInt32($hdr, $pe + 8); $size = [BitConverter]::ToUInt32($hdr, $pe + 80)
  if ($stamp -ne 0x6AC53765 -or $size -ne 0x35BDE000) { throw ("Not the pinned Version 2 build ({0:X8}/{1:X8})" -f $stamp, $size) }
  $sha = { param($b) ([BitConverter]::ToString([Security.Cryptography.SHA256]::Create().ComputeHash($b))).Replace('-', '') }
  $stampText = Get-Date -Format 'yyyyMMdd-HHmmss'
  New-Item -ItemType Directory -Force $OutDir | Out-Null

  $candidates = @()
  $direct = $base + 0xBB9A8E8
  foreach ($c in @(@('direct', $direct), @('pointer', $r.Q($direct)))) {
    $obj = [uint64]$c[1]
    if ($obj -lt 0x10000) { continue }
    $ptr = $r.Q($obj + 0xD8); $len = $r.Q($obj + 0xE0)
    $inline = ($ptr -eq $obj + 0x58)
    $entry = [ordered]@{ kind = $c[0]; objectIsInImage = ($obj -ge $base -and $obj -lt $base + $size); keyPointerIsInline = $inline; keyLength = $len; saved = $null }
    if ($len -ge 1 -and $len -le 0x80 -and $ptr) {
      $key = $r.Read($ptr, [int]$len)
      if ($key) {
        $nonZero = @($key | Where-Object { $_ -ne 0 }).Count
        $entry.nonZeroBytes = $nonZero
        $entry.keySha256 = & $sha $key
        if ($nonZero -gt 0) {
          $file = Join-Path $OutDir ("vcblowfish-key-{0}-{1}-{2}.bin" -f $Label, $c[0], $stampText)
          [IO.File]::WriteAllBytes($file, $key)
          $entry.saved = Split-Path $file -Leaf
        }
      }
    }
    # Full manager snapshot (contains key; local only) for offline layout analysis.
    $snap = $r.Read($obj, 0x400)
    if ($snap) {
      $sfile = Join-Path $OutDir ("vcblowfish-manager-{0}-{1}-{2}.bin" -f $Label, $c[0], $stampText)
      [IO.File]::WriteAllBytes($sfile, $snap); $entry.snapshot = Split-Path $sfile -Leaf
    }
    $candidates += $entry
  }
  $report = [ordered]@{
    schema = 'nba2k27-roster-key-capture-v1'; label = $Label; time = (Get-Date).ToString('o')
    pid = $game[0].Id; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; writes = 0
    managerRva = '0xBB9A8E8'; candidates = $candidates
    note = 'Key bytes are in the .bin files only; never print or bundle them.'
  }
  $json = $report | ConvertTo-Json -Depth 5
  $out = Join-Path $OutDir ("key-capture-{0}-{1}.json" -f $Label, $stampText)
  [IO.File]::WriteAllText($out, $json, (New-Object Text.UTF8Encoding($false)))
  Write-Output $json
  Write-Output "Saved: $out"
} finally { $r.Dispose() }
