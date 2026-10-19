# Read-only Version 2 character scale inventory (coaches, cheerleaders, crowd,
# referees, players). Opens NBA2K27.exe with QUERY_INFORMATION | VM_READ only:
# no write, operation, thread or injection rights. Nothing is written to the
# game. Output: JSON next to this project's artifacts folder.
param([string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) ("artifacts/v2-character-scale-inventory-{0}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))))
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Text;
public sealed class GameReader : IDisposable {
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  IntPtr h; public ulong Base;
  public GameReader(int pid) {
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
  public byte B(ulong at) { var b = Read(at, 1); return b == null ? (byte)0 : b[0]; }
  public float F(ulong at) { var b = Read(at, 4); return b == null ? float.NaN : BitConverter.ToSingle(b, 0); }
  // MSVC x64 RTTI: vtable[-1] -> CompleteObjectLocator; +12 = TypeDescriptor RVA; name at TD+16.
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
$r = New-Object GameReader($game[0].Id)
try {
  $base = $r.Base
  $hdr = $r.Read($base, 4096); $pe = [BitConverter]::ToInt32($hdr, 0x3C)
  $stamp = [BitConverter]::ToUInt32($hdr, $pe + 8); $size = [BitConverter]::ToUInt32($hdr, $pe + 80)
  if ($stamp -ne 0x6AC53765 -or $size -ne 0x35BDE000) { throw ("Not the pinned Version 2 build ({0:X8}/{1:X8})" -f $stamp, $size) }
  $rva = { param($a) if ($a -ge $base -and $a -lt $base + 0x35BDE000) { '0x{0:X}' -f ($a - $base) } else { $null } }
  $INPUT = 0x0A41D07C; $TABLE = 0x0AB5CE30; $MANAGER = 0x068844D8; $MANAGER_IF = 0x06884898
  # Seed: a verified player manager from the mapped wrapper table.
  $seed = 0
  for ($i = 0; $i -lt 30 -and -not $seed; $i++) {
    $m = $r.Q($base + $TABLE + $i * 0x50 + 0x10)
    if ($m -and $r.Q($m) -eq $base + $MANAGER_IF) { $m -= 0x78 }
    if ($m -and $r.Q($m) -eq $base + $MANAGER) { $seed = $m }
  }
  if (-not $seed) { throw "No loaded player manager yet; enter a court and run again" }
  # Version 2 list shape (probed live): the MANAGER_INSTANCE itself is the node;
  # +8 = previous, +0x10 = next, actor at +0x110, interface (WPF "manager") at
  # +0x78, so WPF's manager+0xB8 state byte is node+0x130. The sentinel lives in
  # image data. Walk previous links to it.
  $node = $seed; $nodeVt = $r.Q($node); $sentinel = 0; $guard = 0; $cur = $node
  while ($guard -lt 4096) {
    $prev = $r.Q($cur + 8)
    if (-not $prev) { break }
    if ($prev -ge $base -and $prev -lt $base + 0x35BDE000) { $sentinel = $prev; break }
    if ($r.Q($prev) -ne $nodeVt) { break }
    $cur = $prev; $guard++
  }
  $rows = @()
  if ($sentinel) {
    $n = $r.Q($sentinel + 0x10); $count = 0
    while ($n -and $n -ne $sentinel -and $count -lt 512) {
      $mgr = $n; $actor = $r.Q($mgr + 0x110)
      $actorVt = if ($actor) { $r.Q($actor) } else { 0 }
      $ifVt = if ($actor) { $r.Q($actor + 0x78) } else { 0 }
      $getter = if ($ifVt) { $r.Q($ifVt + 0x158) } else { 0 }
      $desc = 0
      if ($getter) { $code = $r.Read($getter, 8); if ($code -and $code[0] -eq 0x48 -and $code[1] -eq 0x8D -and $code[2] -eq 0x81 -and $code[7] -eq 0xC3) { $desc = $actor + 0x78 + [BitConverter]::ToInt32($code, 3) } }
      $scale = if ($actor) { $r.F($actor + 0xBCC) } else { [float]::NaN }
      $inverse = if ($actor) { $r.F($actor + 0xBD0) } else { [float]::NaN }
      $rows += [ordered]@{
        managerClass = $r.ClassName($r.Q($mgr)); managerVtable = & $rva $r.Q($mgr); managerState130 = $r.B($mgr + 0x130)
        actorClass = $r.ClassName($actorVt); actorVtable = & $rva $actorVt
        descriptionOffset = $(if ($desc) { '0x{0:X}' -f ($desc - $actor) } else { $null })
        descriptionClass = $(if ($desc) { $r.ClassName($r.Q($desc)) } else { $null }); descriptionVtable = $(if ($desc) { & $rva $r.Q($desc) } else { $null })
        optInFlag = $(if ($desc) { '0x{0:X2}' -f $r.B($desc + 0x7D) } else { $null }); sharedOptIn = $(if ($desc) { [bool]($r.B($desc + 0x7D) -band 2) } else { $null })
        scaleMethod = $(if ($ifVt) { & $rva $r.Q($ifVt + 0x188) } else { $null })
        scale = [math]::Round($scale, 5); inverse = [math]::Round($inverse, 5); product = [math]::Round($scale * $inverse, 5)
      }
      $n = $r.Q($n + 0x10); $count++
    }
  }
  $groups = $rows | Group-Object { $_.actorClass } | ForEach-Object {
    $s = @($_.Group | Where-Object { -not [double]::IsNaN($_.scale) } | ForEach-Object { $_.scale })
    [ordered]@{ actorClass = $_.Name; count = $_.Count; sharedOptIn = @($_.Group | Where-Object { $_.sharedOptIn }).Count
      minScale = $(if ($s) { ($s | Measure-Object -Minimum).Minimum }); maxScale = $(if ($s) { ($s | Measure-Object -Maximum).Maximum }) }
  }
  $result = [ordered]@{
    readOnly = $true; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; pid = $game[0].Id; capturedUtc = [DateTime]::UtcNow.ToString('o')
    sharedInput = [math]::Round($r.F($base + $INPUT), 5)
    seedManagerClass = $r.ClassName($r.Q($seed)); nodeVtable = & $rva $nodeVt; nodeClass = $r.ClassName($nodeVt)
    listSentinel = $(if ($sentinel) { & $rva $sentinel } else { $null }); characters = $rows.Count
    groups = @($groups); rows = $rows
  }
  New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
  $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $Out -Encoding utf8
  "Wrote $Out"
  $result.Remove('rows'); $result | ConvertTo-Json -Depth 4
} finally { $r.Dispose() }
