param([string]$GameExe = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe')
$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class SerializerReadOnly {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern IntPtr OpenProcess(uint rights, bool inherit, int pid);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool ReadProcessMemory(IntPtr handle, IntPtr address, byte[] bytes, UIntPtr count, out UIntPtr read);
 [DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr handle);
}
'@
$game = @(Get-Process -Name NBA2K27 | Where-Object { $_.Path -eq $GameExe })
if ($game.Count -ne 1) { throw 'Expected one running game at the selected path.' }
$base = $game[0].MainModule.BaseAddress.ToInt64()
$proof = Get-Content (Join-Path (Split-Path $PSScriptRoot -Parent) 'assets/roster-v2-field-proof.json') -Raw | ConvertFrom-Json
$start = [int]$proof.serializerRva
$length = [int]$proof.serializerBytes
if ($start -ne 0x6ae330 -or $length -lt 1 -or $length -gt 65536) { throw 'Unbounded serializer proof span' }
$handle = [SerializerReadOnly]::OpenProcess(0x410, $false, $game[0].Id)
if ($handle -eq [IntPtr]::Zero) { throw 'Cannot open game for read-only comparison.' }
$stream = [IO.File]::OpenRead($GameExe)
try {
    $header = New-Object byte[] 4096
    if ($stream.Read($header,0,4096) -ne 4096) { throw 'Truncated image header' }
    $pe = [BitConverter]::ToInt32($header,60)
    $sections = [BitConverter]::ToUInt16($header,$pe+6)
    $optional = [BitConverter]::ToUInt16($header,$pe+20)
    $diskOffset = -1L
    for ($i=0; $i -lt $sections; $i++) {
        $at = $pe+24+$optional+$i*40
        $rva = [BitConverter]::ToUInt32($header,$at+12)
        $size = [BitConverter]::ToUInt32($header,$at+16)
        if ($start -ge $rva -and $start+$length -le $rva+$size) {
            $diskOffset = [long][BitConverter]::ToUInt32($header,$at+20)+$start-$rva
        }
    }
    if ($diskOffset -lt 0) { throw 'Serializer is outside file-backed sections' }
    $disk = New-Object byte[] $length
    [void]$stream.Seek($diskOffset,[IO.SeekOrigin]::Begin)
    if ($stream.Read($disk,0,$length) -ne $length) { throw 'Truncated serializer' }
    $live = New-Object byte[] $length
    $read = [UIntPtr]::Zero
    if (-not [SerializerReadOnly]::ReadProcessMemory($handle,[IntPtr]($base+$start),$live,[UIntPtr]$length,[ref]$read) -or $read.ToUInt64() -ne $length) { throw 'Incomplete game read' }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $diff = @(for ($i=0; $i -lt $length; $i++) { if ($disk[$i] -ne $live[$i]) { [pscustomobject]@{ rva=('{0:X8}' -f ($start+$i)); disk=('{0:X2}' -f $disk[$i]); live=('{0:X2}' -f $live[$i]) } } })
        [ordered]@{ pid=$game[0].Id; imageBase=('{0:X}' -f $base); timestamp=('{0:X8}' -f [BitConverter]::ToUInt32($header,$pe+8)); imageSize=('{0:X8}' -f [BitConverter]::ToUInt32($header,$pe+80)); diskSha256=[BitConverter]::ToString($sha.ComputeHash($disk)).Replace('-','').ToLower(); liveSha256=[BitConverter]::ToString($sha.ComputeHash($live)).Replace('-','').ToLower(); expectedSha256=$proof.serializerSha256; changedBytes=$diff.Count; differences=@($diff | Select-Object -First 32); gameWrites=$false } | ConvertTo-Json -Depth 4
    } finally { $sha.Dispose() }
} finally { $stream.Dispose(); [void][SerializerReadOnly]::CloseHandle($handle) }
