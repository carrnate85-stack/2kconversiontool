param(
    [Parameter(Mandatory=$true)][string]$Worker,
    [Parameter(Mandatory=$true)][string]$GameRoot,
    [Parameter(Mandatory=$true)][string]$ModsRoot,
    [Parameter(Mandatory=$true)][string]$RelativePath,
    [switch]$FirstBatchOnly
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

try {
    $catalogTimer = [Diagnostics.Stopwatch]::StartNew()
    $catalog = Call-Worker 'game:load' @($GameRoot)
    $catalogTimer.Stop()

    $fullPath = Join-Path $ModsRoot $RelativePath
    $previewTimer = [Diagnostics.Stopwatch]::StartNew()
    $preview = Call-Worker 'preview:load-loose' @($fullPath, $ModsRoot, ($RelativePath -replace '\\','/'))
    $previewTimer.Stop()

    $cursor = 0
    $total = $null
    $loadedTextures = 0
    [uint64]$loadedBytes = 0
    $batches = 0
    $textureTimer = [Diagnostics.Stopwatch]::StartNew()
    do {
        $batch = Call-Worker 'preview:material-textures' @([UInt64]::MaxValue, $cursor)
        $batches++
        $loadedTextures += @($batch.textures).Count
        $loadedBytes += [uint64]$batch.bytes
        $next = [int]$batch.end
        $total = [int]$batch.total
        if ($next -le $cursor -and $cursor -lt $total) {
            throw "Texture cursor did not advance from $cursor of $total"
        }
        $cursor = $next
    } while (-not $FirstBatchOnly -and $cursor -lt $total)
    $textureTimer.Stop()

    [ordered]@{
        pass = ($FirstBatchOnly -or $cursor -eq $total)
        mode = if ($FirstBatchOnly) { 'first-batch-timing' } else { 'all-batches' }
        file = $RelativePath
        catalogResources = $catalog.resources
        catalogMs = $catalogTimer.ElapsedMilliseconds
        previewMs = $previewTimer.ElapsedMilliseconds
        meshes = @($preview.meshes).Count
        reportedInstances = $preview.stats.instances
        batches = $batches
        loadedTextures = $loadedTextures
        finalCursor = $cursor
        total = $total
        loadedBytes = $loadedBytes
        textureLoadMs = $textureTimer.ElapsedMilliseconds
    } | ConvertTo-Json -Depth 4
} finally {
    try { $process.StandardInput.Close() } catch {}
    if (-not $process.WaitForExit(3000)) { $process.Kill($true) }
    $process.Dispose()
}
