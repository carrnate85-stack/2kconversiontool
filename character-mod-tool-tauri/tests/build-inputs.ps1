$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
. (Join-Path $root 'build-tools/build-inputs.ps1')
$artifacts = [IO.Path]::GetFullPath((Join-Path $root 'artifacts'))
$folder = Join-Path $artifacts ('build-inputs-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($folder) | Out-Null
try {
    $file = Join-Path $folder 'app.ts'
    $extra = Join-Path $folder 'new.ts'
    [IO.File]::WriteAllText($file, 'first')
    $snapshot = { Get-BuildInputSnapshot -Trees @{ source = $folder } }
    $first = & $snapshot
    Assert-BuildInputSnapshot $first (& $snapshot)
    $modified = [IO.File]::GetLastWriteTimeUtc($file)
    [IO.File]::WriteAllText($file, 'other')
    [IO.File]::SetLastWriteTimeUtc($file, $modified)
    $rejected = 0
    foreach ($change in @('edited', 'added', 'removed')) {
        if ($change -eq 'added') { [IO.File]::WriteAllText($file, 'first'); [IO.File]::WriteAllText($extra, 'added') }
        if ($change -eq 'removed') { Remove-Item -LiteralPath $extra; Remove-Item -LiteralPath $file }
        try { Assert-BuildInputSnapshot $first (& $snapshot) }
        catch { if ($_.Exception.Message -notlike '*publication aborted*') { throw }; $rejected++ }
    }
    if ($rejected -ne 3) { throw 'A changed build input was accepted.' }
    [IO.File]::WriteAllText($file, 'first')
    Assert-BuildInputSnapshot $first (& $snapshot)
    $filtered = Get-BuildInputSnapshot -Trees @{ source = $folder } -Include { param($label, $relative) $relative -ne 'ignored.tmp' }
    [IO.File]::WriteAllText((Join-Path $folder 'ignored.tmp'), 'generated output')
    $again = Get-BuildInputSnapshot -Trees @{ source = $folder } -Include { param($label, $relative) $relative -ne 'ignored.tmp' }
    Assert-BuildInputSnapshot $filtered $again
    if ($again.Files -ne 1) { throw 'Generated-output exclusion failed.' }
    $release = Join-Path $folder 'sharing.exe'
    [IO.File]::WriteAllText($release, 'prior verified release')
    $published = $false
    try { Assert-BuildInputSnapshot $first (& $snapshot); $published = $true }
    catch { if ($_.Exception.Message -notlike '*publication aborted*') { throw } }
    if ($published -or [IO.File]::ReadAllText($release) -ne 'prior verified release') { throw 'A failed build guard modified the published release.' }
    Write-Output 'Build inputs: unchanged/restored inputs, same-size timestamp-preserving edits, additions, removals, output exclusion and publication protection passed.'
} finally {
    $absolute = [IO.Path]::GetFullPath($folder)
    if ((Split-Path $absolute -Parent) -ne $artifacts) { throw 'Unsafe build-input test cleanup path.' }
    if ((Get-Item -LiteralPath $absolute).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Test cleanup link rejected.' }
    Remove-Item -LiteralPath $absolute -Recurse -Force
}
