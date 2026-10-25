function Get-BuildInputSnapshot {
    param([hashtable]$Trees = @{}, [string[]]$Files = @(), [scriptblock]$Include = { $true })
    $entries = [Collections.Generic.SortedDictionary[string,string]]::new([StringComparer]::Ordinal)
    $paths = [Collections.Generic.List[string]]::new()
    foreach ($label in $Trees.Keys) {
        $directory = [IO.Path]::GetFullPath($Trees[$label])
        if (!(Test-Path -LiteralPath $directory -PathType Container)) { throw "Build input directory missing: $directory" }
        foreach ($file in Get-ChildItem -LiteralPath $directory -File -Recurse -Force) {
            $relative = [IO.Path]::GetRelativePath($directory, $file.FullName).Replace('\', '/')
            if (& $Include $label $relative) { $paths.Add($file.FullName) }
        }
    }
    foreach ($file in $Files) { $paths.Add([IO.Path]::GetFullPath($file)) }
    foreach ($file in $paths) {
        $input = [IO.File]::OpenRead($file)
        try { $entries[$file] = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($input)) }
        finally { $input.Dispose() }
    }
    $bytes = [Text.Encoding]::UTF8.GetBytes((ConvertTo-Json -InputObject $entries -Compress))
    $digest = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
    return [pscustomobject]@{ Digest = $digest; Entries = $entries; Files = $entries.Count }
}

function Assert-BuildInputSnapshot {
    param($Expected, $Actual, [string]$Label = 'Build inputs')
    if ($Expected.Digest -eq $Actual.Digest) { return }
    $changed = @($Expected.Entries.Keys) + @($Actual.Entries.Keys) | Sort-Object -Unique | Where-Object {
        !$Expected.Entries.ContainsKey($_) -or !$Actual.Entries.ContainsKey($_) -or $Expected.Entries[$_] -ne $Actual.Entries[$_]
    } | Select-Object -First 8
    throw "$Label changed during the build; publication aborted. Rebuild from stable inputs.`n$($changed -join "`n")"
}

function Get-CharacterBuildInputs {
    param([string]$Root, [string]$Production, [string]$Roster)
    $project = Split-Path $Root -Parent
    $trees = @{
        rust = (Join-Path $Root 'src'); web = (Join-Path $Root 'web'); worker = (Join-Path $Root 'worker')
        engine = (Join-Path $Root 'engine'); tools = (Join-Path $Root 'build-tools'); tests = (Join-Path $Root 'tests')
        release = (Join-Path $Root 'release')
        cargo = (Join-Path $Root '.cargo'); backend = $Production
        reference = (Join-Path $project 'character-mod-tool-webview2/Reference/src')
        shared = (Join-Path $project 'shared/nba2k-assets')
    }
    $files = @(Get-ChildItem -LiteralPath $Root -File -Force | Where-Object { $_.Extension -ne '.md' -and $_.Name -ne '.gitignore' } | ForEach-Object FullName)
    $files += $Roster
    $files += Join-Path $project 'character-mod-tool-webview2/dist/Character-Mod-Tool-WebView2-0.1.31/App/Reference/src/helpers/Nba2kVczDecoder.exe'
    Get-BuildInputSnapshot -Trees $trees -Files $files -Include {
        param($Label, $Relative)
        if ($Relative -match '(^|/)(__pycache__|\.pytest_cache)(/|$)|\.py[co]$') { return $false }
        if ($Label -eq 'web' -and $Relative -eq 'ui/dom.generated.d.ts') { return $false }
        if ($Label -eq 'shared' -and $Relative -like 'target/*') { return $false }
        if ($Label -eq 'backend') {
            if ($Relative -match '(^|/)node_modules(/|$)|(^|/)node.exe$' -or $Relative -eq 'tools/live_roster/roster.exe') { return $false }
            return Test-PackagedRuntimeFile $Relative
        }
        return $true
    }
}
