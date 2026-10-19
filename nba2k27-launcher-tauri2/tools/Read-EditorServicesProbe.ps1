# Read-only Version 2 probe for the roster editor services (staff, rotations,
# player->team pointer, season stats). See
# artifacts/editor-services-v2-research-20261017.md "Live checks".
#
# Opens NBA2K27.exe with PROCESS_QUERY_INFORMATION | PROCESS_VM_READ only: no
# write, operation, thread or injection rights. Nothing is written to the game.
# The roster owner and its allocation are located exactly as the proven V2 read
# path does (src/roster_v2_profile.rs + roster_v2_access.rs::allocation):
# pinned PE identity + code anchors -> ROOT -> root class -> owner(+0xA8) ->
# VirtualQueryEx walk of the private allocation holding the player table.
#
# Usage (game at a menu with a roster loaded):
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\Read-EditorServicesProbe.ps1
# Offline self-test against a synthetic memory fixture (no process access):
#   ... -File tools\Read-EditorServicesProbe.ps1 -Fixture <fixture.json> -Out <out.json>
param(
  [string]$Out = (Join-Path (Split-Path $PSScriptRoot -Parent) ("artifacts/editor-services-live-{0}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss'))),
  [string]$Fixture = '',
  [int]$DetailTeams = 2,
  [int]$DetailPlayers = 8
)
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @"
using System; using System.Collections; using System.Collections.Generic; using System.Collections.Specialized;
using System.Runtime.InteropServices; using System.Text;
public struct MemRegion { public ulong Base, Allocation, Len; public uint State, Type, Protect; }
public interface IMem { ulong Base { get; } byte[] Read(ulong at, int n); bool Query(ulong at, out MemRegion r); }
public sealed class LiveMem : IMem, IDisposable {
  [StructLayout(LayoutKind.Sequential)] struct MBI { public ulong BaseAddress, AllocationBase; public uint AllocationProtect, A1; public ulong RegionSize; public uint State, Protect, Type, A2; }
  [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr OpenProcess(uint a, bool i, int p);
  [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
  [DllImport("kernel32.dll")] static extern bool ReadProcessMemory(IntPtr h, IntPtr a, byte[] b, IntPtr n, out IntPtr r);
  [DllImport("kernel32.dll")] static extern IntPtr VirtualQueryEx(IntPtr h, IntPtr a, out MBI m, IntPtr n);
  [DllImport("psapi.dll", CharSet=CharSet.Unicode)] static extern bool EnumProcessModulesEx(IntPtr h, IntPtr[] m, int cb, out int need, uint flag);
  IntPtr h; ulong b;
  public ulong Base { get { return b; } }
  public LiveMem(int pid) {
    h = OpenProcess(0x0400 | 0x0010, false, pid); // QUERY_INFORMATION | VM_READ only
    if (h == IntPtr.Zero) throw new Exception("OpenProcess (read-only) failed");
    var mods = new IntPtr[1]; int need;
    if (!EnumProcessModulesEx(h, mods, IntPtr.Size, out need, 3)) throw new Exception("Module query failed");
    b = (ulong)mods[0].ToInt64();
  }
  public byte[] Read(ulong at, int n) {
    if (at < 0x10000 || at + (ulong)n > 0x7fffffffffffUL || n < 1 || n > 8192) return null;
    var buf = new byte[n]; IntPtr r;
    return ReadProcessMemory(h, (IntPtr)(long)at, buf, (IntPtr)n, out r) && r.ToInt64() == n ? buf : null;
  }
  public bool Query(ulong at, out MemRegion r) {
    MBI m; r = new MemRegion();
    if (VirtualQueryEx(h, (IntPtr)(long)at, out m, (IntPtr)Marshal.SizeOf(typeof(MBI))).ToInt64() != Marshal.SizeOf(typeof(MBI))) return false;
    r.Base = m.BaseAddress; r.Allocation = m.AllocationBase; r.Len = m.RegionSize; r.State = m.State; r.Type = m.Type; r.Protect = m.Protect; return true;
  }
  public void Dispose() { if (h != IntPtr.Zero) CloseHandle(h); h = IntPtr.Zero; }
}
// Synthetic memory for offline tests: sparse blocks inside declared regions (unset bytes read as zero).
public sealed class FixtureMem : IMem {
  readonly List<KeyValuePair<ulong, byte[]>> blocks = new List<KeyValuePair<ulong, byte[]>>();
  readonly List<MemRegion> regions = new List<MemRegion>(); ulong b;
  public FixtureMem(ulong imageBase) { b = imageBase; }
  public ulong Base { get { return b; } }
  public void AddBlock(ulong at, byte[] bytes) { blocks.Add(new KeyValuePair<ulong, byte[]>(at, bytes)); }
  public void AddRegion(ulong at, ulong alloc, ulong len, uint state, uint type, uint protect) {
    var r = new MemRegion(); r.Base = at; r.Allocation = alloc; r.Len = len; r.State = state; r.Type = type; r.Protect = protect; regions.Add(r);
  }
  public byte[] Read(ulong at, int n) {
    if (at < 0x10000 || n < 1 || n > 8192) return null;
    bool mapped = false;
    foreach (var r in regions) if (at >= r.Base && at + (ulong)n <= r.Base + r.Len && r.State == 0x1000) mapped = true;
    if (!mapped) return null;
    var buf = new byte[n];
    foreach (var kv in blocks) {
      ulong s = Math.Max(at, kv.Key), e = Math.Min(at + (ulong)n, kv.Key + (ulong)kv.Value.Length);
      if (s < e) Array.Copy(kv.Value, (long)(s - kv.Key), buf, (long)(s - at), (long)(e - s));
    }
    return buf;
  }
  public bool Query(ulong at, out MemRegion r) {
    foreach (var x in regions) if (at >= x.Base && at < x.Base + x.Len) { r = x; return true; }
    r = new MemRegion(); r.Base = at & ~0xFFFUL; r.Allocation = 0; r.Len = 0x1000; r.State = 0x10000; return true; // MEM_FREE
  }
}
public static class EditorProbe {
  const ulong ROOT = 0x09ACE338, ROOT_CLASS = 0x074EBE98;
  const int PLAYER = 0x4F8, TEAM = 0x1550, STAFF = 0x1C8, SEASON = 0x40;
  const ulong MAX_ALLOC = 128UL * 1024 * 1024;
  static readonly object[][] RIP = { new object[]{0x0211A0FBUL, "488b05", ROOT}, new object[]{0x0211A120UL, "488b05", ROOT}, new object[]{0x02406BF9UL, "488d3d", ROOT_CLASS} };
  static readonly object[][] FIELD = {
    new object[]{0x0211A102UL, "488b80a8000000"},
    new object[]{0x02CC96F1UL, "3b5110731289d04869c0f8040000480341" + "18"},
    new object[]{0x02DDD3B1UL, "3b9180000000731589d04869c05015000048038188000000"},
    new object[]{0x02ED9BD1UL, "3b5160731289d04869c078120000480341" + "68"},
    new object[]{0x02EDCB31UL, "3b9190020000731589d04869c0700100004803819802" + "0000"} };
  static byte[] Hex(string s) { var o = new byte[s.Length / 2]; for (int i = 0; i < o.Length; i++) o[i] = Convert.ToByte(s.Substring(i * 2, 2), 16); return o; }
  static string X(ulong v) { return "0x" + v.ToString("X"); }
  static byte[] R(IMem m, ulong at, int n) { var b = m.Read(at, n); if (b == null) throw new Exception("Read failed at " + X(at)); return b; }
  static ulong Q(IMem m, ulong at) { return BitConverter.ToUInt64(R(m, at, 8), 0); }
  static bool Ptr(ulong v) { return v >= 0x10000 && v <= 0x7ffffffffff8UL && v % 8 == 0; }
  static ulong P(IMem m, ulong at) { ulong v = Q(m, at); if (!Ptr(v)) throw new Exception("Invalid pointer at " + X(at)); return v; }
  static bool Same(byte[] a, byte[] b) { if (a.Length != b.Length) return false; for (int i = 0; i < a.Length; i++) if (a[i] != b[i]) return false; return true; }
  static string Name(byte[] row, int off, int len) {
    var sb = new StringBuilder();
    for (int i = off; i + 1 < off + len && i + 1 < row.Length; i += 2) { char c = (char)BitConverter.ToUInt16(row, i); if (c == 0) break; if (char.IsControl(c)) return null; sb.Append(c); }
    return sb.ToString().Trim();
  }
  static bool Private(MemRegion r) { return r.State == 0x1000 && r.Type == 0x20000 && (r.Protect == 2 || r.Protect == 4 || r.Protect == 8); }
  public static void VerifyIdentity(IMem m) {
    var h = R(m, m.Base, 4096);
    if (h[0] != 0x4D || h[1] != 0x5A) throw new Exception("Not an MZ image");
    int pe = BitConverter.ToInt32(h, 0x3C);
    if (pe < 64 || pe > 3800 || BitConverter.ToUInt32(h, pe) != 0x4550) throw new Exception("Invalid PE header");
    if (BitConverter.ToUInt16(h, pe + 4) != 0x8664 || BitConverter.ToUInt16(h, pe + 24) != 0x20b) throw new Exception("Not x64 PE32+");
    uint stamp = BitConverter.ToUInt32(h, pe + 8), size = BitConverter.ToUInt32(h, pe + 80);
    if (stamp != 0x6AC53765 || size != 0x35BDE000) throw new Exception(string.Format("Not the pinned Version 2 build ({0:X8}/{1:X8})", stamp, size));
    foreach (var a in RIP) {
      ulong rva = (ulong)a[0]; var pre = Hex((string)a[1]); var b = R(m, m.Base + rva, pre.Length + 4);
      for (int i = 0; i < pre.Length; i++) if (b[i] != pre[i]) throw new Exception("Roster code anchor differs at " + X(rva));
      long target = (long)rva + pre.Length + 4 + BitConverter.ToInt32(b, pre.Length);
      if (target != (long)(ulong)a[2]) throw new Exception("Roster anchor targets another object at " + X(rva));
    }
    foreach (var a in FIELD) { var want = Hex((string)a[1]); if (!Same(R(m, m.Base + (ulong)a[0], want.Length), want)) throw new Exception("Roster table descriptor differs at " + X((ulong)a[0])); }
    var vt = R(m, m.Base + ROOT_CLASS, 16);
    if (BitConverter.ToUInt64(vt, 0) != m.Base + 0x024E65F0 || BitConverter.ToUInt64(vt, 8) != m.Base + 0x024E6660) throw new Exception("Roster container class differs");
  }
  // roster_v2_access.rs::allocation: the private allocation holding the player table, 8..128 MiB, no holes.
  public static ulong[] Allocation(IMem m, ulong table) {
    MemRegion first; if (!m.Query(table, out first)) throw new Exception("Roster query failed");
    ulong b = first.Allocation;
    if (!Private(first) || b < 0x10000 || b % 4096 != 0 || table < first.Base || table >= first.Base + first.Len) throw new Exception("Active table is not committed non-executable private data");
    ulong at = b;
    for (int i = 0; i < 32770; i++) {
      MemRegion r; if (!m.Query(at, out r)) throw new Exception("Roster query failed");
      if (r.Allocation != b) {
        ulong len = at - b;
        if (len < 8UL * 1024 * 1024 || len > MAX_ALLOC || table >= b + len) throw new Exception("Active roster allocation size differs");
        return new ulong[] { b, len };
      }
      if (r.Base != at || !Private(r) || r.Len < 4096 || r.Len % 4096 != 0) throw new Exception("Roster allocation has a hole, executable page or unsupported protection");
      at += r.Len; if (at - b > MAX_ALLOC) throw new Exception("Roster allocation exceeds 128 MiB");
    }
    throw new Exception("Roster allocation query budget exceeded");
  }
  sealed class Dir { public ulong Count, Table; public int Stride; public bool In(ulong p) { return p >= Table && p < Table + Count * (ulong)Stride && (p - Table) % (ulong)Stride == 0; } public long Index(ulong p) { return In(p) ? (long)((p - Table) / (ulong)Stride) : -1; } }
  static Dir Directory(IMem m, ulong owner, int off, int stride, ulong min, ulong max, ulong rb, ulong rl, bool optional) {
    var h = R(m, owner + (ulong)off, 16); var d = new Dir(); d.Count = BitConverter.ToUInt64(h, 0); d.Table = BitConverter.ToUInt64(h, 8); d.Stride = stride;
    if (optional && d.Count == 0 && d.Table == 0) return d;
    if (d.Count < min || d.Count > max || !Ptr(d.Table)) throw new Exception("Owner directory +" + off.ToString("X") + " is not initialized");
    if (d.Table < rb + 0x680 || d.Table + d.Count * (ulong)stride > rb + rl) throw new Exception("Owner directory +" + off.ToString("X") + " exceeds its owned allocation");
    return d;
  }
  static ArrayList Top(Dictionary<int, int> h, int n) {
    var l = new List<KeyValuePair<int, int>>(h); l.Sort((a, b) => b.Value != a.Value ? b.Value.CompareTo(a.Value) : a.Key.CompareTo(b.Key));
    var o = new ArrayList(); for (int i = 0; i < l.Count && i < n; i++) o.Add("0x" + l[i].Key.ToString("X") + ":" + l[i].Value); return o;
  }
  static OrderedDictionary Check(string status, string reason) { var o = new OrderedDictionary(); o["status"] = status; o["reason"] = reason; return o; }
  // Bit field read (little-endian bit numbering from byte 0), optional two's-complement sign.
  static long Bits(byte[] b, int bit, int bits, bool signed) {
    long v = 0; for (int i = 0; i < bits; i++) { int n = bit + i; if ((b[n >> 3] & (1 << (n & 7))) != 0) v |= 1L << i; }
    return signed && bits < 64 && (v >> (bits - 1)) != 0 ? v - (1L << bits) : v;
  }
  // Field bit layout from the V2 StaffData serializer 0x6BBA00 (tools/editor_fields_v2_audit.py).
  static readonly string[] StaffRatings = { "BenchDepth", "LineupFormFactor", "RunPlayTendency", "DoubleTeamTendency", "ZoneUsage", "ShotInsideTendency",
    "ShotCloseTendency", "ShotMediumTendency", "ShotThreeTendency", "DriveTendency", "PostUpTendency", "AlleyOopTendency", "Tempo", "HelpDefense",
    "Motivation", "Intensity", "BasketballIQ", "WorkEthic", "Offense", "Defense", "Business", "Training", "Charisma", "Analytics" };
  static readonly string[] StaffSystems = { "Balanced", "GritAndGrind", "PaceAndSpace", "Perimeter", "Post", "Triangle", "SevenSeconds", "DefenseFirst" };
  static OrderedDictionary StaffFields(IMem m, ulong owner, ulong rb, ulong rl, Dir teams, Dictionary<long, byte[]> teamRows, Dictionary<long, string> teamNames, int detailTeams) {
    OrderedDictionary c;
    try {
      var staff = Directory(m, owner, 0x180, STAFF, 1, 8192, rb, rl, false);
      var seen = new HashSet<ulong>(); var uids = new Dictionary<int, int>(); long recs = 0, sysOk = 0, named = 0;
      var rmin = new int[32]; var rmax = new int[32]; for (int i = 0; i < 32; i++) { rmin[i] = 255; rmax[i] = 0; }
      var samples = new ArrayList();
      foreach (var kv in teamRows) {
        for (int s = 0; s < 32; s++) {
          ulong p = BitConverter.ToUInt64(kv.Value, 0xF70 + s * 8); if (p == 0 || !staff.In(p) || !seen.Add(p)) continue;
          var sr = R(m, p, STAFF); recs++;
          int uid = BitConverter.ToUInt16(sr, 0x120); int n; uids.TryGetValue(uid, out n); uids[uid] = n + 1;
          long pref = Bits(sr, 0x130 * 8 + 20, 4, true), act = Bits(sr, 0x130 * 8 + 24, 4, true);
          if (pref >= -1 && pref <= 7 && act >= -1 && act <= 7) sysOk++;
          string first = Name(sr, 0x50, 40), last = Name(sr, 0x78, 40); if (!string.IsNullOrEmpty(first) || !string.IsNullOrEmpty(last)) named++;
          for (int i = 0; i < 32; i++) { int v = sr[0x158 + i]; if (v < rmin[i]) rmin[i] = v; if (v > rmax[i]) rmax[i] = v; }
          if (samples.Count < Math.Max(3, detailTeams * 3)) {
            var e = new OrderedDictionary(); e["team"] = teamNames[kv.Key]; e["slot"] = s; e["staffIndex"] = staff.Index(p); e["name"] = (first + " " + last).Trim();
            e["uniqueId120"] = uid; e["playbookId122"] = Bits(sr, 0x122 * 8, 9, false); e["wage124"] = BitConverter.ToUInt32(sr, 0x124);
            e["soundId128"] = BitConverter.ToInt16(sr, 0x128); e["graphicId12A"] = Bits(sr, 0x12A * 8, 15, false); e["playerUniqueIdA8"] = BitConverter.ToUInt32(sr, 0xA8);
            e["growthPotential131"] = sr[0x131]; e["yearsWithTeam134"] = Bits(sr, 0x134 * 8, 7, false); e["yearsInLeague134_7"] = Bits(sr, 0x134 * 8 + 7, 8, false);
            e["yearsLeft13E"] = Bits(sr, 0x13E * 8, 4, false); e["preferredSystem"] = pref; e["activeSystem"] = act; e["job182"] = sr[0x182] & 0x1F;
            var r = new OrderedDictionary(); for (int i = 0; i < 24; i++) r[StaffRatings[i]] = sr[0x158 + i]; e["ratings158"] = r;
            var pr = new OrderedDictionary(); for (int i = 0; i < 8; i++) pr[StaffSystems[i]] = sr[0x170 + i]; e["proficiency170"] = pr;
            int badges = 0; for (int i = 0x1A4; i <= 0x1C1; i++) if (sr[i] != 0) badges++; e["nonZeroBadgeBytes1A4"] = badges;
            samples.Add(e);
          }
        }
      }
      long dup = 0; foreach (var kv in uids) if (kv.Value > 1) dup++;
      bool ok = recs > 0 && sysOk == recs && named == recs && dup == 0;
      c = Check(ok ? "PASS" : (recs == 0 ? "INCONCLUSIVE" : "FAIL"),
        "listed staff decode with the V2 serializer layout: unique +0x120 ids, systems in -1..7, names; compare samples with the in-game staff screen");
      c["records"] = recs; c["uniqueIdDuplicates"] = dup; c["systemsInRange"] = sysOk; c["named"] = named;
      var ranges = new OrderedDictionary(); for (int i = 0; i < 32; i++) ranges[i < 24 ? StaffRatings[i] : "proficiency" + StaffSystems[i - 24]] = rmin[i] + ".." + rmax[i];
      c["byteRanges158"] = ranges; c["samples"] = samples;
    } catch (Exception e) { c = Check("FAIL", e.Message); }
    return c;
  }
  // PLAYERSTATMANAGERDATA pool (owner+0x1C8). V2 callbacks set by Init 0x2F0B760/0x2F0B83A.
  static readonly ulong[] PoolCallbacks = { 0x2EEB980, 0x2EEB950, 0x2FD0870, 0x2FD0830 };
  static OrderedDictionary SeasonPool(IMem m, byte[] head, List<byte[]> rows) {
    OrderedDictionary c;
    try {
      ulong pool = BitConverter.ToUInt64(head, 0x1C8), table = BitConverter.ToUInt64(head, 0x1D8); int count = BitConverter.ToInt32(head, 0x1D0);
      if (!Ptr(pool) || !Ptr(table) || count < 1 || count >= 0xFFFF) throw new Exception("Season pool or table is not initialized");
      var pr = R(m, pool, 0x38); int cbOk = 0; var cbs = new ArrayList();
      for (int i = 0; i < 4; i++) { ulong v = BitConverter.ToUInt64(pr, 0x18 + i * 8); cbs.Add(X(v - m.Base)); if (v == m.Base + PoolCallbacks[i]) cbOk++; }
      var stats = new byte[count * SEASON];
      for (int o = 0; o < stats.Length; o += 8192) { int n = Math.Min(8192, stats.Length - o); Array.Copy(R(m, table + (ulong)o, n), 0, stats, o, n); }
      var counts = new int[count]; long privates = 0, bad = 0;
      foreach (var row in rows) {
        if (BitConverter.ToUInt64(row, 0xE0) != 0) { privates++; continue; }
        for (int k = 0; k < 33; k++) { int v = BitConverter.ToInt16(row, 0x130 + k * 2); if (v < 0) continue; if (v >= count) { bad++; continue; } counts[v]++; }
      }
      long refMismatch = 0, used = 0, yearBad = 0; var mism = new ArrayList();
      for (int i = 0; i < count; i++) {
        int rc = (int)((BitConverter.ToUInt32(stats, i * SEASON + 0x3C) >> 13) & 0xFF);
        if (rc != Math.Min(counts[i], 255)) { refMismatch++; if (mism.Count < 8) mism.Add(new object[] { i, rc, counts[i] }); }
        if (counts[i] > 0) { used++; int y = 1900 + stats[i * SEASON + 0x11]; if (y < 1946 || y > 2100) yearBad++; }
      }
      int h = BitConverter.ToUInt16(pr, 0); var free = new HashSet<int>(); bool chainOk = true;
      while (h != 0xFFFF) { if (h >= count || counts[h] != 0 || !free.Add(h)) { chainOk = false; break; } h = BitConverter.ToUInt16(stats, h * SEASON + 0x12); }
      bool ok = cbOk == 4 && bad == 0 && refMismatch == 0 && chainOk && used + free.Count == count && yearBad == 0;
      c = Check(ok ? "PASS" : "FAIL", "pool callbacks = V2 RVAs; refcount (+0x3C>>13&0xFF) = directory references; free chain (head u16 pool+0, next u16 +0x12) covers every unused record; years 1946..2100");
      c["pool"] = X(pool); c["callbacks"] = cbs; c["callbacksMatch"] = cbOk; c["poolTable8"] = X(BitConverter.ToUInt64(pr, 8)); c["poolTableMatches1D8"] = BitConverter.ToUInt64(pr, 8) == table;
      c["poolCount10"] = BitConverter.ToInt32(pr, 0x10); c["records"] = count; c["usedRecords"] = used; c["freeChain"] = free.Count; c["freeChainOk"] = chainOk;
      c["refcountMismatches"] = refMismatch; c["refcountSamples"] = mism; c["outOfRangeRefs"] = bad; c["playersWithPrivateE0"] = privates; c["yearsOutOfRange"] = yearBad;
    } catch (Exception e) { c = Check("FAIL", e.Message); }
    return c;
  }
  // Menu statics WPF's season import gated on (SeasonImportCommit.cs), V2 RVAs from
  // src/blacktop_observer_profile.rs VERSION_TWO. Informational: which screen is safe.
  static OrderedDictionary Menu(IMem m) {
    OrderedDictionary c;
    try {
      const ulong MODES = 0xAD07408, CUR = 0xAD07BD0, ALT = 0xAD07BCC, SETTINGS = 0xAB5CE20, ACTIVE = 0xAB5D790, ROOT2 = 0xAF46870;
      int ci = BitConverter.ToInt32(R(m, m.Base + CUR, 4), 0), ai = BitConverter.ToInt32(R(m, m.Base + ALT, 4), 0);
      uint cur = ci >= 0 && ci < 31 ? BitConverter.ToUInt32(R(m, m.Base + MODES + (ulong)ci * 64, 4), 0) : 0;
      uint alt = ai >= 0 && ai < 31 ? BitConverter.ToUInt32(R(m, m.Base + MODES + (ulong)ai * 64, 4), 0) : 0;
      int stage = BitConverter.ToInt32(R(m, m.Base + ROOT2 + 0xEA8, 4), 0);
      ulong settings = BitConverter.ToUInt64(R(m, m.Base + SETTINGS, 8), 0); int kind = Ptr(settings) ? R(m, settings, 1)[0] : -1;
      int active = R(m, m.Base + ACTIVE, 1)[0];
      bool safe = cur == alt && active == 0 && kind == 0 && ((cur == 0x6509837B && (stage == 0 || stage == 1)) || (cur == 0x079964C0 && stage == 1) || (cur == 0x0B888544 && stage == 0));
      c = Check("INFO", "WPF SafeMenu() inputs on V2; record which screen the capture was taken on");
      c["currentIndex"] = ci; c["alternateIndex"] = ai; c["currentMenu"] = "0x" + cur.ToString("X8"); c["alternateMenu"] = "0x" + alt.ToString("X8");
      c["stageRootEA8"] = stage; c["settingsKind"] = kind; c["activeGame"] = active; c["wpfSafeMenu"] = safe;
    } catch (Exception e) { c = Check("FAIL", e.Message); }
    return c;
  }
  public static OrderedDictionary Run(IMem m, int detailTeams, int detailPlayers) {
    var res = new OrderedDictionary();
    VerifyIdentity(m); res["identity"] = "PE 6AC53765/35BDE000 + roster code anchors verified";
    ulong root = P(m, m.Base + ROOT);
    if (Q(m, root) != m.Base + ROOT_CLASS) throw new Exception("Active roster container has a different class");
    ulong owner = P(m, root + 0xA8), ptable = P(m, owner + 0x18);
    var region = Allocation(m, ptable); ulong rb = region[0], rl = region[1];
    if (owner < rb || owner + 0x540 > rb + rl) throw new Exception("Active roster owner is outside its native allocation");
    var players = Directory(m, owner, 0x10, PLAYER, 100, 8192, rb, rl, false);
    var teams = Directory(m, owner, 0x80, TEAM, 1, 1024, rb, rl, false);
    var head = R(m, owner, 0x540);
    res["owner"] = X(owner); res["allocation"] = X(rb) + "+" + X(rl);
    res["players"] = players.Count; res["teams"] = teams.Count;
    // Player pass: names, UniqueId (+0x128 u16), team pointer (+0x60), history guard (+0xE0), season directory (+0x130 i16[33]).
    var uniq = new HashSet<int>(); var uniqueToTeam = new Dictionary<int, long>();
    var memberU = new Dictionary<long, HashSet<int>>(); var memberI = new Dictionary<long, HashSet<int>>(); var memberA = new Dictionary<long, HashSet<ulong>>();
    long zero = 0, teamRec = 0, other = 0; var otherSamples = new ArrayList(); var named = new List<int>();
    var rows = new List<byte[]>();
    for (ulong i = 0; i < players.Count; i++) {
      var row = R(m, players.Table + i * PLAYER, PLAYER); rows.Add(row);
      string last = Name(row, 0, 40), first = Name(row, 40, 40);
      if (string.IsNullOrEmpty(last) && string.IsNullOrEmpty(first)) continue;
      named.Add((int)i); int u = BitConverter.ToUInt16(row, 0x128); uniq.Add(u);
      ulong t = BitConverter.ToUInt64(row, 0x60);
      if (t == 0) zero++; else if (teams.In(t)) { teamRec++; long ti = teams.Index(t); uniqueToTeam[u] = ti;
        if (!memberU.ContainsKey(ti)) { memberU[ti] = new HashSet<int>(); memberI[ti] = new HashSet<int>(); memberA[ti] = new HashSet<ulong>(); }
        memberU[ti].Add(u); memberI[ti].Add((int)i); memberA[ti].Add(players.Table + i * PLAYER); }
      else { other++; if (otherSamples.Count < 5) otherSamples.Add(new object[] { (int)i, X(t) }); }
    }
    res["namedPlayers"] = named.Count;
    // 3. player+0x60 == team record
    var c3 = Check(teamRec >= 100 && other * 100 <= teamRec ? "PASS" : (teamRec == 0 ? "FAIL" : "INCONCLUSIVE"),
      "named players whose +0x60 is a team record start / zero / other");
    c3["teamRecord"] = teamRec; c3["zero"] = zero; c3["other"] = other; c3["otherSamples"] = otherSamples;
    // Teams with names (detail on the first few that have staff slots).
    var teamNames = new Dictionary<long, string>(); var teamRows = new Dictionary<long, byte[]>();
    for (ulong t = 0; t < teams.Count; t++) {
      var row = new byte[TEAM];
      for (int o = 0; o < TEAM; o += 4096) { int n = Math.Min(4096, TEAM - o); Array.Copy(R(m, teams.Table + t * TEAM + (ulong)o, n), 0, row, o, n); }
      string nm = Name(row, 0x2FA, 48); if (string.IsNullOrEmpty(nm)) continue; teamNames[(long)t] = nm; teamRows[(long)t] = row;
    }
    // 1. Staff: owner+0x180 directory, team+0xF70 slots, back-pointer +0x18, slot +0x182&0x1F, names +0x50/+0x78.
    OrderedDictionary c1;
    try {
      var staff = Directory(m, owner, 0x180, STAFF, 1, 8192, rb, rl, false);
      long slots = 0, aligned = 0, back = 0, slotOk = 0, namesOk = 0, shared = 0; var detail = new ArrayList(); var mism = new ArrayList();
      foreach (var kv in teamRows) {
        ulong ta = teams.Table + (ulong)kv.Key * TEAM; var td = new ArrayList();
        for (int s = 0; s < 32; s++) {
          ulong p = BitConverter.ToUInt64(kv.Value, 0xF70 + s * 8); if (p == 0) continue; slots++;
          if (!staff.In(p)) { if (td.Count < 32) td.Add(new object[] { s, X(p), "not-in-staff-table" }); continue; }
          aligned++; var sr = R(m, p, STAFF);
          ulong owner18 = BitConverter.ToUInt64(sr, 0x18);
          bool bp = owner18 == ta, so = (sr[0x182] & 0x1F) == s; if (bp) back++; if (so) slotOk++;
          string n1 = Name(sr, 0x50, 40), n2 = Name(sr, 0x78, 40); if (!string.IsNullOrEmpty(n1) || !string.IsNullOrEmpty(n2)) namesOk++;
          if (!bp) {
            long ot = teams.Index(owner18); bool lists = false; int oslot = -1;
            if (ot >= 0) { var orow = R(m, owner18 + 0xF70, 256); for (int k = 0; k < 32; k++) if (BitConverter.ToUInt64(orow, k * 8) == p) { lists = true; oslot = k; } }
            if (lists) shared++;
            if (mism.Count < 64) { var e = new OrderedDictionary(); e["team"] = kv.Key; e["teamName"] = teamNames[kv.Key]; e["slot"] = s; e["staffIndex"] = staff.Index(p);
              e["name50"] = n1; e["name78"] = n2; e["plus18"] = X(owner18); e["plus18Team"] = ot; e["plus18TeamName"] = ot >= 0 && teamNames.ContainsKey(ot) ? teamNames[ot] : null;
              e["plus18TeamListsIt"] = lists; e["plus18TeamSlot"] = oslot; e["slotBits182"] = sr[0x182] & 0x1F; mism.Add(e); }
          }
          if (detail.Count < detailTeams) { var e = new OrderedDictionary(); e["slot"] = s; e["index"] = staff.Index(p); e["name50"] = n1; e["name78"] = n2; e["uniqueId120"] = BitConverter.ToUInt16(sr, 0x120); e["backPointer"] = bp; e["slotMatches"] = so; td.Add(e); }
        }
        if (detail.Count < detailTeams && td.Count > 0) { var e = new OrderedDictionary(); e["team"] = kv.Key; e["name"] = teamNames[kv.Key]; e["slots"] = td; detail.Add(e); }
      }
      bool ok = slots > 0 && aligned == slots && slotOk == slots && namesOk * 10 >= slots * 9 && back + shared == slots;
      c1 = Check(ok ? (back == slots ? "PASS" : "PASS-SHARED") : (slots == 0 ? "INCONCLUSIVE" : "FAIL"),
        "every team+0xF70 pointer is a staff record with slot bits and a name; +0x18 is this team or another team that also lists it (shared)");
      c1["sharedWithOtherTeam"] = shared; c1["backPointerMismatches"] = mism;
      c1["staffCount"] = staff.Count; c1["staffTable"] = X(staff.Table); c1["teamSlots"] = slots; c1["inTable"] = aligned; c1["backPointer"] = back; c1["slotBits"] = slotOk; c1["named"] = namesOk; c1["detail"] = detail;
    } catch (Exception e) { c1 = Check("FAIL", e.Message); }
    // 2. Rotations: team+0x1360 240 u16 vs player UniqueIds (and, when +0x60 maps, the same team's members).
    long ids = 0, known = 0, sameTeam = 0, sameTeamChecked = 0; var rot = new ArrayList();
    foreach (var kv in teamRows) {
      long tid = 0, tk = 0;
      for (int k = 0; k < 240; k++) {
        int v = BitConverter.ToUInt16(kv.Value, 0x1360 + k * 2); if (v == 0 || v == 0xFFFF) continue;
        ids++; tid++; if (uniq.Contains(v)) { known++; tk++; }
        long mt; if (uniqueToTeam.TryGetValue(v, out mt)) { sameTeamChecked++; if (mt == kv.Key) sameTeam++; }
      }
      if (rot.Count < detailTeams && tid > 0) { var e = new OrderedDictionary(); e["team"] = kv.Key; e["name"] = teamNames[kv.Key]; e["nonEmptyIds"] = tid; e["knownPlayers"] = tk;
        var seg = new ArrayList(); for (int k = 0; k < 5; k++) seg.Add(BitConverter.ToUInt16(kv.Value, 0x1360 + k * 2)); e["firstSegment"] = seg; rot.Add(e); }
    }
    // team+0x154C bit 0 = has advanced rotation (getter 0x2F86CF0; set by the Rotations-table commit
    // 0x32D39D8 and gameplan apply 0x22FA6DE, cleared at 0x22FA392). Only flagged teams carry a block.
    long flagged = 0, flaggedValid = 0; var flaggedDetail = new ArrayList();
    foreach (var kv in teamRows) {
      if ((kv.Value[0x154C] & 1) == 0) continue; flagged++;
      HashSet<int> mem; memberU.TryGetValue(kv.Key, out mem); bool valid = mem != null && mem.Count >= 5; int badSeg = -1;
      for (int s = 0; s < 48 && valid; s++) {
        var seen = new HashSet<int>();
        for (int k = 0; k < 5; k++) { int v = BitConverter.ToUInt16(kv.Value, 0x1360 + s * 10 + k * 2); if (!mem.Contains(v) || !seen.Add(v)) { valid = false; badSeg = s; } }
      }
      if (valid) flaggedValid++;
      if (flaggedDetail.Count < 8) { var e = new OrderedDictionary(); e["team"] = kv.Key; e["name"] = teamNames[kv.Key]; e["members"] = mem == null ? 0 : mem.Count; e["valid"] = valid; e["firstBadSegment"] = badSeg;
        var seg = new ArrayList(); for (int k = 0; k < 5; k++) seg.Add(BitConverter.ToUInt16(kv.Value, 0x1360 + k * 2)); e["firstSegment"] = seg; flaggedDetail.Add(e); }
    }
    string rotStatus;
    if (flagged > 0) rotStatus = flaggedValid == flagged ? "PASS" : "FAIL";
    else if (ids == 0) rotStatus = "AUTO";
    else rotStatus = known * 100 >= ids * 95 ? "PASS" : "FAIL";
    var c2 = Check(rotStatus, "flagged teams (+0x154C bit 0): every segment is 5 distinct own-member UniqueIds; else >=95% of non-empty ids are UniqueIds; AUTO = no flag and empty block");
    c2["nonEmptyIds"] = ids; c2["knownPlayerIds"] = known; c2["sameTeamViaPlus60"] = sameTeam; c2["teamCheckedViaPlus60"] = sameTeamChecked; c2["detail"] = rot;
    c2["advancedRotationTeams"] = flagged; c2["advancedRotationValid"] = flaggedValid; c2["advancedRotationDetail"] = flaggedDetail;
    // Diagnostics: what the 0x1360 block holds, and where each team's members (+0x60) appear inside the team record.
    long z = 0, ff = 0, oth = 0; var distinct = new Dictionary<int, int>(); var heads = new ArrayList();
    foreach (var kv in teamRows) {
      for (int k = 0; k < 240; k++) { int v = BitConverter.ToUInt16(kv.Value, 0x1360 + k * 2); if (v == 0) z++; else if (v == 0xFFFF) ff++; else { oth++; int c; distinct.TryGetValue(v, out c); distinct[v] = c + 1; } }
      if (heads.Count < 4 && memberU.ContainsKey(kv.Key)) { var e = new OrderedDictionary(); e["team"] = kv.Key; e["name"] = teamNames[kv.Key]; e["members"] = memberU[kv.Key].Count;
        e["hex1340"] = BitConverter.ToString(kv.Value, 0x1340, 0x60).Replace("-", ""); e["hex1530"] = BitConverter.ToString(kv.Value, 0x1530, 0x20).Replace("-", ""); heads.Add(e); }
    }
    c2["blockZeroU16"] = z; c2["blockFFFFU16"] = ff; c2["blockOtherU16"] = oth; c2["blockDistinct"] = distinct.Count; c2["blockSamples"] = heads;
    var hitU = new Dictionary<int, int>(); var hitI = new Dictionary<int, int>(); var hitA = new Dictionary<int, int>(); int teamsScanned = 0;
    foreach (var kv in memberU) {
      byte[] row; if (!teamRows.TryGetValue(kv.Key, out row) || kv.Value.Count < 5) continue; teamsScanned++;
      for (int o = 0; o + 2 <= TEAM; o += 2) {
        int v = BitConverter.ToUInt16(row, o); int c;
        if (v != 0 && kv.Value.Contains(v)) { hitU.TryGetValue(o, out c); hitU[o] = c + 1; }
        if (v != 0 && memberI[kv.Key].Contains(v)) { hitI.TryGetValue(o, out c); hitI[o] = c + 1; }
        if (o % 8 == 0 && o + 8 <= TEAM && memberA[kv.Key].Contains(BitConverter.ToUInt64(row, o))) { hitA.TryGetValue(o, out c); hitA[o] = c + 1; }
      }
    }
    c2["memberScanTeams"] = teamsScanned; c2["memberUniqueIdOffsets"] = Top(hitU, 24); c2["memberRecordIndexOffsets"] = Top(hitI, 24); c2["memberPointerOffsets"] = Top(hitA, 24);
    // 4. Seasons: owner+0x1C0 kind, +0x1C8 pool, +0x1D0 count (i32), +0x1D8 table (64-byte records); player +0xE0, +0x130 i16[33].
    OrderedDictionary c4;
    try {
      uint kind = BitConverter.ToUInt32(head, 0x1C0); ulong pool = BitConverter.ToUInt64(head, 0x1C8);
      int scount = BitConverter.ToInt32(head, 0x1D0); ulong stable = BitConverter.ToUInt64(head, 0x1D8);
      bool tableOk = scount > 0 && scount <= 0x100000 && Ptr(stable) && stable + (ulong)scount * SEASON <= 0x7fffffffffffUL;
      long refs = 0, inRange = 0, outRange = 0, privateHist = 0, teamPtrOk = 0, teamPtrChecked = 0; var det = new ArrayList();
      foreach (int i in named) {
        var row = rows[i]; if (BitConverter.ToUInt64(row, 0xE0) != 0) privateHist++;
        var dir = new ArrayList();
        for (int k = 0; k < 33; k++) {
          int v = BitConverter.ToInt16(row, 0x130 + k * 2); dir.Add(v); if (v < 0) continue; refs++;
          if (v < scount) { inRange++;
            if (tableOk && teamPtrChecked < 64) { var sr = R(m, stable + (ulong)v * SEASON, SEASON); ulong t0 = BitConverter.ToUInt64(sr, 0); teamPtrChecked++; if (t0 == 0 || teams.In(t0)) teamPtrOk++; }
          } else outRange++;
        }
        if (det.Count < detailPlayers) { var e = new OrderedDictionary(); e["player"] = i; e["name"] = (Name(row, 40, 40) + " " + Name(row, 0, 40)).Trim(); e["e0"] = X(BitConverter.ToUInt64(row, 0xE0)); e["dir130"] = dir; det.Add(e); }
      }
      bool ok = tableOk && kind != 0 && Ptr(pool) && refs > 0 && outRange == 0 && teamPtrChecked > 0 && teamPtrOk * 10 >= teamPtrChecked * 9;
      c4 = Check(ok ? "PASS" : (refs == 0 ? "INCONCLUSIVE" : "FAIL"), "season directory indexes fall inside owner+0x1D0 count; referenced records start with a team pointer");
      c4["poolKind1C0"] = kind; c4["pool1C8"] = X(pool); c4["seasonCount1D0"] = scount; c4["seasonTable1D8"] = X(stable); c4["seasonTableInAllocation"] = stable >= rb && stable + (ulong)Math.Max(scount, 0) * SEASON <= rb + rl;
      c4["directoryRefs"] = refs; c4["inRange"] = inRange; c4["outOfRange"] = outRange; c4["playersWithE0"] = privateHist; c4["recordTeamPtrOk"] = teamPtrOk; c4["recordTeamPtrChecked"] = teamPtrChecked; c4["detail"] = det;
    } catch (Exception e) { c4 = Check("FAIL", e.Message); }
    var c5 = StaffFields(m, owner, rb, rl, teams, teamRows, teamNames, detailTeams);
    var c6 = SeasonPool(m, head, rows);
    var c7 = Menu(m);
    // Stability: the roster must not have changed owner or directories during capture.
    bool stable2 = P(m, m.Base + ROOT) == root && P(m, root + 0xA8) == owner && Same(R(m, owner, 0x540), head);
    res["stable"] = stable2;
    var checks = new OrderedDictionary(); checks["1_staff"] = c1; checks["2_rotations"] = c2; checks["3_player_team_pointer"] = c3; checks["4_seasons"] = c4;
    checks["5_staff_fields"] = c5; checks["6_season_pool"] = c6; checks["7_menu_statics"] = c7;
    res["checks"] = checks;
    return res;
  }
}
"@
$result = [ordered]@{ readOnly = $true; access = 'PROCESS_QUERY_INFORMATION|PROCESS_VM_READ'; capturedUtc = [DateTime]::UtcNow.ToString('o') }
if ($Fixture) {
  $f = Get-Content -LiteralPath $Fixture -Raw | ConvertFrom-Json
  $mem = New-Object FixtureMem([Convert]::ToUInt64($f.base, 16))
  foreach ($r in $f.regions) { $mem.AddRegion([Convert]::ToUInt64($r.base, 16), [Convert]::ToUInt64($r.allocation, 16), [Convert]::ToUInt64($r.len, 16), [uint32]$r.state, [uint32]$r.type, [uint32]$r.protect) }
  foreach ($b in $f.blocks) { $mem.AddBlock([Convert]::ToUInt64($b.at, 16), [Convert]::FromBase64String($b.data)) }
  $result.source = "fixture:$Fixture"
} else {
  $game = @(Get-Process NBA2K27 -ErrorAction Stop)
  if ($game.Count -ne 1) { throw "Expected exactly one NBA2K27 process" }
  $mem = New-Object LiveMem($game[0].Id)
  $result.source = 'live'; $result.pid = $game[0].Id
}
try {
  $probe = [EditorProbe]::Run($mem, $DetailTeams, $DetailPlayers)
  foreach ($k in $probe.Keys) { $result[$k] = $probe[$k] }
} catch {
  $result.error = $_.Exception.InnerException.Message
  if (-not $result.error) { $result.error = $_.Exception.Message }
} finally { if ($mem -is [IDisposable]) { $mem.Dispose() } }
New-Item -ItemType Directory -Force (Split-Path $Out) | Out-Null
$result | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $Out -Encoding utf8
"Wrote $Out"
if ($result.error) { "FAIL setup: $($result.error)"; exit 2 }
"stable during capture: $($result.stable)  players=$($result.players) named=$($result.namedPlayers) teams=$($result.teams)"
foreach ($k in $result.checks.Keys) { $c = $result.checks[$k]; "{0,-6} {1}: {2}" -f $c.status, $k, $c.reason }
if (-not $result.stable) { exit 3 }
