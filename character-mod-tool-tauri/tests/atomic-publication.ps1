$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$artifacts = [IO.Path]::GetFullPath((Join-Path $root 'artifacts'))
$folder = Join-Path $artifacts ('atomic-publication-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($folder) | Out-Null
$sharing = Join-Path $folder 'sharing.exe'
$candidate = Join-Path $folder 'candidate.exe'
try {
    [IO.File]::WriteAllText($sharing, 'prior verified release')
    [IO.File]::WriteAllText($candidate, 'new verified candidate')
    $lock = [IO.File]::Open($sharing, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    $rejected = $false
    try {
        try { [IO.File]::Replace($candidate, $sharing, [NullString]::Value) }
        catch { $rejected = $true }
        if (!$rejected -or [IO.File]::ReadAllText($sharing) -ne 'prior verified release' -or !(Test-Path -LiteralPath $candidate)) {
            throw 'Locked release publication must preserve the old file and candidate.'
        }
    } finally { $lock.Dispose() }
    [IO.File]::Replace($candidate, $sharing, [NullString]::Value)
    if ([IO.File]::ReadAllText($sharing) -ne 'new verified candidate' -or (Test-Path -LiteralPath $candidate)) {
        throw 'Verified candidate publication failed.'
    }
    [IO.File]::WriteAllText($candidate, 'first verified release')
    $new = Join-Path $folder 'new.exe'
    [IO.File]::Move($candidate, $new)
    if ([IO.File]::ReadAllText($new) -ne 'first verified release') { throw 'First release publication failed.' }
    Write-Output 'Atomic publication: locked release preserved, replacement and first publication passed.'
} finally {
    $absolute = [IO.Path]::GetFullPath($folder)
    if (!$absolute.StartsWith($artifacts + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe test cleanup path.' }
    if ((Get-Item -LiteralPath $absolute).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Test cleanup link rejected.' }
    Remove-Item -LiteralPath $absolute -Recurse -Force
}
