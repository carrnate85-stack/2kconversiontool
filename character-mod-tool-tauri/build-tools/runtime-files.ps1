$script:RuntimePolicy = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'runtime-policy.json') -Raw | ConvertFrom-Json
function Test-PackagedRuntimeFile([string]$RelativePath) {
    $path = $RelativePath.Replace('\', '/')
    $prefix = 'python_runtime/Lib/site-packages/'
    if (!$path.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) { return $true }
    $package = $path.Substring($prefix.Length).Split('/')[0]
    foreach ($pattern in $script:RuntimePolicy.excludedSitePackages) {
        if ($package -like $pattern) { return $false }
    }
    return $true
}
