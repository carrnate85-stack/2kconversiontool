param(
    [Parameter(Mandatory=$true)][string]$Worker,
    [Parameter(Mandatory=$true)][string]$GameRoot,
    [Parameter(Mandatory=$true)][string]$ModsRoot,
    [Parameter(Mandatory=$true)][string]$RelativePath,
    [string]$Config
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
function Read-Exact([IO.Stream]$Stream,[int]$Count) {
    $bytes = [byte[]]::new($Count)
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($bytes,$offset,$Count-$offset)
        if ($read -le 0) { throw 'Preview worker stopped before replying' }
        $offset += $read
    }
    $bytes
}
function Call-Worker([string]$Channel,[object[]]$Arguments) {
    $script:sequence++
    $callTimer = [Diagnostics.Stopwatch]::StartNew()
    $request = @{ id=$script:sequence; channel=$Channel; args=$Arguments } | ConvertTo-Json -Compress -Depth 12
    $bytes = $utf8.GetBytes($request)
    $header = [BitConverter]::GetBytes([uint32]$bytes.Length)
    $inputStream.Write($header,0,4)
    $inputStream.Write($bytes,0,$bytes.Length)
    $inputStream.Flush()
    while ($true) {
        $length = [BitConverter]::ToUInt32((Read-Exact $outputStream 4),0)
        if ($length -eq 0 -or $length -gt 268435456) { throw 'Invalid worker frame length' }
        $message = $utf8.GetString((Read-Exact $outputStream ([int]$length))) | ConvertFrom-Json
        if ($message.kind -eq 'event') {
            Write-Host ("  {0,7} ms  {1} ({2}%)" -f $callTimer.ElapsedMilliseconds,$message.payload.phase,$message.payload.percent)
            continue
        }
        if ($message.kind -ne 'reply' -or $message.id -ne $script:sequence) { continue }
        if (-not $message.ok) { throw $message.error }
        return $message.value
    }
}
function Material-Target([string]$Name) {
    $name = $Name.ToLowerInvariant()
    if ($name -match '(?:^|_)(?:arms?|chest)(?:_|$)') { return 'chest' }
    if ($name -match '(?:^|_)(?:torso|body)(?:_|$)') { return 'torso' }
    if ($name -match '(?:^|_)(?:legs?|thigh|calf)(?:_|$)') { return 'legs' }
    if ($name -match '(?:^|_)face(?:_|$)') { return 'face' }
    return ''
}
function Boundary-PairCount([object]$Preview,[string]$Left,[string]$Right) {
    $count = 0
    foreach ($mesh in @($Preview.meshes)) {
        $vertices = @{ $Left = [Collections.Generic.HashSet[int]]::new(); $Right = [Collections.Generic.HashSet[int]]::new() }
        foreach ($group in @($mesh.groups)) {
            $target = Material-Target ([string]$group.material)
            if ($target -ne $Left -and $target -ne $Right) { continue }
            $end = [int]$group.start + [int]$group.count
            for ($cursor = [int]$group.start; $cursor -lt $end; $cursor++) {
                [void]$vertices[$target].Add([int]$mesh.indices[$cursor])
            }
        }
        $rightPositions = @{}
        foreach ($vertex in $vertices[$Right]) {
            $offset = $vertex * 3
            $key = '{0:F3},{1:F3},{2:F3}' -f [double]$mesh.positions[$offset],[double]$mesh.positions[$offset+1],[double]$mesh.positions[$offset+2]
            $rightPositions[$key] = $true
        }
        foreach ($vertex in $vertices[$Left]) {
            $offset = $vertex * 3
            $key = '{0:F3},{1:F3},{2:F3}' -f [double]$mesh.positions[$offset],[double]$mesh.positions[$offset+1],[double]$mesh.positions[$offset+2]
            if ($rightPositions.ContainsKey($key)) { $count++ }
        }
    }
    return $count
}
try {
    Write-Host 'Probe: bootstrap'
    $bootstrap = Call-Worker 'app:bootstrap' @()
    Write-Host 'Probe: game catalog'
    $indexed = Call-Worker 'game:load' @($GameRoot)
    $fullPath = Join-Path $ModsRoot $RelativePath
    Write-Host "Probe: loose preview $RelativePath"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $previewArgs = @($fullPath,$ModsRoot,($RelativePath -replace '\\','/'))
    if ($PSBoundParameters.ContainsKey('Config')) { $previewArgs += $Config }
    $preview = Call-Worker 'preview:load-loose' $previewArgs
    $arenaBatch = $null
    if ([bool]$preview.arena) {
        Write-Host 'Probe: first arena material batch'
        $arenaBatch = Call-Worker 'preview:material-textures' @([UInt64]::MaxValue,0)
    }
    $timer.Stop()
    [ordered]@{
        pass = $true
        workerVersion = $bootstrap.version
        resources = $indexed.resources
        relativePath = $RelativePath
        elapsedMs = $timer.ElapsedMilliseconds
        meshes = @($preview.meshes).Count
        textures = @($preview.textures).Count
        arena = [bool]$preview.arena
        arenaTextureBatch = if ($null -eq $arenaBatch) { $null } else { @($arenaBatch.textures).Count }
        arenaTextureBytes = if ($null -eq $arenaBatch) { $null } else { $arenaBatch.bytes }
        arenaTextureCursor = if ($null -eq $arenaBatch) { $null } else { "$($arenaBatch.end)/$($arenaBatch.total)" }
        player = $null -ne $preview.player
        playerDetails = $preview.player
        looseSource = $preview.source.kind -eq 'loose-mod'
        dependencyContext = $preview.source.dependencyContext
        skinBoundaryPairs = [ordered]@{
            faceChest = Boundary-PairCount $preview 'face' 'chest'
            chestTorso = Boundary-PairCount $preview 'chest' 'torso'
            torsoLegs = Boundary-PairCount $preview 'torso' 'legs'
        }
        textureDetails = @($preview.textures | ForEach-Object {
            [ordered]@{ name=$_.name; source=$_.source; format=$_.format; binding=$_.binding; range=$_.range }
        })
        materialDetails = @($preview.materials | ForEach-Object {
            [ordered]@{ name=$_.name; shader=$_.shader; texture=$_.texture; textures=$_.textures }
        })
        meshDetails = @($preview.meshes | ForEach-Object {
            $xs = for ($i=0; $i -lt $_.positions.Count; $i+=3) { [double]$_.positions[$i] }
            $ys = for ($i=1; $i -lt $_.positions.Count; $i+=3) { [double]$_.positions[$i] }
            $zs = for ($i=2; $i -lt $_.positions.Count; $i+=3) { [double]$_.positions[$i] }
            [ordered]@{ name=$_.name; bounds=[ordered]@{ x=@(($xs | Measure-Object -Minimum -Maximum).Minimum,($xs | Measure-Object -Minimum -Maximum).Maximum); y=@(($ys | Measure-Object -Minimum -Maximum).Minimum,($ys | Measure-Object -Minimum -Maximum).Maximum); z=@(($zs | Measure-Object -Minimum -Maximum).Minimum,($zs | Measure-Object -Minimum -Maximum).Maximum) }; groups=@($_.groups | ForEach-Object { [ordered]@{ material=$_.material; start=$_.start; count=$_.count } }); configurationItem=$_.configurationItem; configurationSource=$_.configurationSource }
        })
        skipped = @($preview.skipped).Count
        skippedItems = @($preview.skipped)
    } | ConvertTo-Json -Depth 5
} finally {
    try { $process.StandardInput.Close() } catch {}
    if (-not $process.WaitForExit(3000)) { $process.Kill($true) }
    $process.Dispose()
}
