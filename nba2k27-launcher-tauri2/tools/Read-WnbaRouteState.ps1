# Read-only Version 2 WNBA route probe. Opens NBA2K27.exe with
# QUERY_INFORMATION | VM_READ only (no write, operation, thread or injection
# rights) and writes nothing to the game. Run it while sitting on a screen
# (Home, Freestyle team select, Blacktop team select) and compare captures.
# Reports: active play-now delegate class, league bytes, roster-hook slots,
# mode hashes, and the loaded team table (count, kinds, WNBA/national IDs).
# Output: JSON in this project's artifacts folder.
param(
  [string]$Label = 'capture',
  [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) ("artifacts/wnba-route-state-{0}-{1}.json" -f $Label, (Get-Date -Format 'yyyyMMdd-HHmmss')))
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text;
public sealed class WnbaRouteReader : IDisposable {
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  IntPtr h; public ulong Base;
  public WnbaRouteReader(int pid) {
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
  public uint D(ulong at) { var b = Read(at, 4); return b == null ? 0 : BitConverter.ToUInt32(b, 0); }
  public byte B(ulong at) { var b = Read(at, 1); return b == null ? (byte)0xFF : b[0]; }
  public string Wide(ulong at, int bytes) { var b = Read(at, bytes); if (b == null) return null; var s = Encoding.Unicode.GetString(b); int z = s.IndexOf('\0'); return z < 0 ? s : s.Substring(0, z); }
  public string ClassName(ulong vtable) {
    if (vtable < Base || vtable > Base + 0x40000000UL) return null;
    ulong col = Q(vtable - 8); if (col < Base || col > Base + 0x40000000UL || D(col) != 1) return null;
    ulong td = Base + D(col + 12); var raw = Read(td + 16, 160); if (raw == null) return null;
    int end = Array.IndexOf(raw, (byte)0); if (end < 0) end = raw.Length;
    return Encoding.ASCII.GetString(raw, 0, end);
  }
  public void Dispose() { if (h != IntPtr.Zero) CloseHandle(h); h = IntPtr.Zero; }
}
"@
$game = @(Get-Process NBA2K27 -ErrorAction Stop)
if ($game.Count -ne 1) { throw "Expected exactly one NBA2K27 process" }
$r = New-Object WnbaRouteReader($game[0].Id)
try {
  $base = $r.Base
  $hdr = $r.Read($base, 4096); $pe = [BitConverter]::ToInt32($hdr, 0x3C)
  $stamp = [BitConverter]::ToUInt32($hdr, $pe + 8); $size = [BitConverter]::ToUInt32($hdr, $pe + 80)
  if ($stamp -ne 0x6AC53765 -or $size -ne 0x35BDE000) { throw ("Not the pinned Version 2 build ({0:X8}/{1:X8})" -f $stamp, $size) }
  $hex = { param($v) '0x{0:X}' -f $v }
  $rva = { param($a) if ($a -ge $base -and $a -lt $base + 0x35BDE000) { '0x{0:X}' -f ($a - $base) } else { $null } }

  # Play-now delegate and league state (offline map: tools/wnba_v2_route_audit.py).
  $delegate = $r.Q($base + 0xAB3CE38)
  $delegateClass = if ($delegate) { $r.ClassName($r.Q($delegate)) } else { $null }
  $gmm = $r.Q($base + 0xAAE3300)
  $league = [ordered]@{
    frontEndIsWnba   = $r.B($base + 0xA313DD9)   # 0x249910 accessor; set by action 0x39D0E30
    frontEndPending  = $r.B($base + 0xA313DD8)
    freestyleLeague  = $r.D($base + 0xB1EA778)   # isWnba + 1, set by 0x36378A0
    gameModeLeague   = if ($gmm) { $r.D($gmm + 4) } else { $null }  # 0x2213A20 / 0x220B400
  }
  $slots = [ordered]@{}
  foreach ($entry in @(@('BLACKTOP_DELEGATE', 0x74CD158), @('TKU_DELEGATE', 0x74DB678), @('TKU_WNBA_DELEGATE', 0x74DDE38))) {
    $slots[$entry[0]] = [ordered]@{
      d30 = & $rva ($r.Q($base + $entry[1] + 0xD30))
      d38 = & $rva ($r.Q($base + $entry[1] + 0xD38))
    }
  }
  $stock = $slots.BLACKTOP_DELEGATE.d30 -eq '0x236E510' -and $slots.BLACKTOP_DELEGATE.d38 -eq '0x2350BF0' -and
    $slots.TKU_DELEGATE.d30 -eq '0x236FB30' -and $slots.TKU_WNBA_DELEGATE.d30 -eq '0x236FF70'

  # Mode hashes (rules_v2 constants) and Blacktop stage.
  $modeAt = { param($idxRva) $i = $r.D($base + $idxRva); if ($i -lt 128) { & $hex ($r.D($base + 0xAD07408 + $i * 64)) } else { $null } }
  $modes = [ordered]@{ current = & $modeAt 0xAD07BD0; alternate = & $modeAt 0xAD07BCC; blacktopStage = $r.D($base + 0xAF46870 + 0xEA8) }

  # Loaded team table (same chain as freestyle_v2.rs / WPF FsRosterRoot).
  $roster = [ordered]@{ readable = $false }
  $root = $r.Q($base + 0x9ACE338)
  $owner = if ($root) { $r.Q($root + 0xA8) } else { 0 }
  if ($owner) {
    $count = $r.D($owner + 0x80); $table = $r.Q($owner + 0x88)
    if ($count -ge 1 -and $count -le 1024 -and $table) {
      $kinds = @{}; $wnba = @(); $national = @(); $nba = @()
      for ($i = 0; $i -lt $count; $i++) {
        $row = $table + [uint64]($i * 0x1550)
        $idBytes = $r.Read($row + 0xF20, 2); $kindRaw = $r.D($row + 0x123C)
        if ($null -eq $idBytes) { continue }
        $id = [BitConverter]::ToUInt16($idBytes, 0); $kind = [int]($kindRaw -shr 26)
        if (-not $kinds.ContainsKey("$kind")) { $kinds["$kind"] = 0 }; $kinds["$kind"]++
        if ($kind -eq 29) { $wnba += [ordered]@{ index = $i; id = $id; name = $r.Wide($row + 0x2FA, 48) } }
        if ($kind -eq 22 -and ($id -eq 69 -or $id -eq 361)) { $national += $id }
        if ($kind -eq 0) { $nba += $id }
      }
      $roster = [ordered]@{
        readable = $true; root = & $hex $root; owner = & $hex $owner; table = & $hex $table; count = $count
        kinds = $kinds; nbaCount = $nba.Count; wnbaCount = $wnba.Count; wnba = $wnba; nationalIds = $national
        mixed = ($nba.Count -ge 30 -and $wnba.Count -ge 15)
      }
    }
  }
  $report = [ordered]@{
    schema = 'nba2k27-wnba-route-state-v1'; label = $Label; time = (Get-Date).ToString('o')
    pid = $game[0].Id; base = & $hex $base; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; writes = 0
    playNowDelegate = [ordered]@{ object = & $rva $delegate; class = $delegateClass }
    league = $league; rosterHookSlots = $slots; rosterHooksStock = $stock; modes = $modes; roster = $roster
  }
  $json = $report | ConvertTo-Json -Depth 6
  New-Item -ItemType Directory -Force (Split-Path $Out -Parent) | Out-Null
  [IO.File]::WriteAllText($Out, $json, (New-Object Text.UTF8Encoding($false)))
  Write-Output $json
  Write-Output "Saved: $Out"
} finally { $r.Dispose() }
