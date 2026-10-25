param(
    [Parameter(Mandatory=$true)][string]$Worker,
    [Parameter(Mandatory=$true)][string]$GameRoot,
    [Parameter(Mandatory=$true)][string]$ModsRoot,
    [Parameter(Mandatory=$true)][string]$RelativePath
)

$ErrorActionPreference = 'Stop'
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $Worker).Path
$start.ArgumentList.Add('--asset-worker')
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$process = [Diagnostics.Process]::new()
$process.StartInfo = $start
if (-not $process.Start()) { throw 'Could not start preview worker' }
$inputStream = $process.StandardInput.BaseStream
$outputStream = $process.StandardOutput.BaseStream
$utf8 = [Text.UTF8Encoding]::new($false)
$sequence = 0

function Read-Exact([IO.Stream]$Stream, [int]$Count) {
    $bytes = [byte[]]::new($Count)
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($bytes, $offset, $Count - $offset)
        if ($read -le 0) { throw 'Preview worker stopped before replying' }
        $offset += $read
    }
    $bytes
}

function Call-Worker([string]$Channel, [object[]]$Arguments) {
    $script:sequence++
    $request = @{ id=$script:sequence; channel=$Channel; args=$Arguments } | ConvertTo-Json -Compress -Depth 12
    $bytes = $utf8.GetBytes($request)
    $header = [BitConverter]::GetBytes([uint32]$bytes.Length)
    $inputStream.Write($header, 0, 4)
    $inputStream.Write($bytes, 0, $bytes.Length)
    $inputStream.Flush()
    while ($true) {
        $length = [BitConverter]::ToUInt32((Read-Exact $outputStream 4), 0)
        if ($length -eq 0 -or $length -gt 268435456) { throw 'Invalid worker frame length' }
        $message = $utf8.GetString((Read-Exact $outputStream ([int]$length))) | ConvertFrom-Json
        if ($message.kind -eq 'event') { continue }
        if ($message.kind -ne 'reply' -or $message.id -ne $script:sequence) { continue }
        if (-not $message.ok) { throw $message.error }
        return $message.value
    }
}

function Matrix-Key([object[]]$Matrix) {
    (@($Matrix | ForEach-Object { '{0:R}' -f [double]$_ }) -join ',')
}

try {
    [void](Call-Worker 'game:load' @($GameRoot))
    $fullPath = Join-Path $ModsRoot $RelativePath
    $preview = Call-Worker 'preview:load-loose' @($fullPath, $ModsRoot, ($RelativePath -replace '\\','/'))
    $rows = foreach ($mesh in @($preview.meshes)) {
        $placements = @($mesh.instances)
        if ($placements.Count -le 1) { continue }
        $groups = $placements | Group-Object { Matrix-Key @($_.matrix) }
        foreach ($group in $groups) {
            if ($group.Count -le 1) { continue }
            $first = $group.Group[0]
            [ordered]@{
                mesh = $mesh.name
                copiesAtSameTransform = $group.Count
                translation = @($first.matrix[12], $first.matrix[13], $first.matrix[14])
                names = @($group.Group | ForEach-Object { $_.name } | Sort-Object -Unique)
                targets = @($group.Group | ForEach-Object { $_.target } | Sort-Object -Unique)
            }
        }
    }
    $basketRows = foreach ($mesh in @($preview.meshes)) {
        $placements = @($mesh.instances)
        if ($mesh.name -notmatch '(?i)basket|rim|backboard|glass|hoop') { continue }
        [ordered]@{
            mesh = $mesh.name
            placements = @($placements | ForEach-Object {
                [ordered]@{
                    name = $_.name
                    target = $_.target
                    translation = @($_.matrix[12], $_.matrix[13], $_.matrix[14])
                }
            })
        }
    }
    [ordered]@{
        file = $RelativePath
        meshes = @($preview.meshes).Count
        reportedInstances = $preview.stats.instances
        exactDuplicateTransformGroups = @($rows).Count
        duplicateTransformDetails = @($rows | Sort-Object -Property copiesAtSameTransform -Descending)
        basketPlacementDetails = @($basketRows)
    } | ConvertTo-Json -Depth 7
} finally {
    try { $process.StandardInput.Close() } catch {}
    if (-not $process.WaitForExit(3000)) { $process.Kill($true) }
    $process.Dispose()
}
