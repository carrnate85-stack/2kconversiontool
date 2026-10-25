param(
    [string]$GameExe = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe'
)
$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$taskTools = Split-Path $taskRoot -Parent
$taskPython = Join-Path $taskTools '.toolchains\llvm-mingw-20260826-ucrt-x86_64\python\bin\python.exe'
$taskMapper = Join-Path $PSScriptRoot 'map-wnba-priority-profile.py'
if (-not (Test-Path -LiteralPath $GameExe -PathType Leaf)) {
    throw "NBA 2K27 executable not found: $GameExe"
}
if (-not (Test-Path -LiteralPath $taskPython -PathType Leaf)) {
    throw "Pinned Python runtime not found: $taskPython"
}
if (-not (Test-Path -LiteralPath $taskMapper -PathType Leaf)) {
    throw "WNBA priority mapper not found: $taskMapper"
}
$taskPrevious = [Environment]::GetEnvironmentVariable('C27_GAME_EXE', 'Process')
try {
    $env:C27_GAME_EXE = (Resolve-Path -LiteralPath $GameExe).Path
    & $taskPython $taskMapper
    if ($LASTEXITCODE -ne 0) {
        throw "WNBA priority mapping failed with exit code $LASTEXITCODE"
    }
} finally {
    [Environment]::SetEnvironmentVariable('C27_GAME_EXE', $taskPrevious, 'Process')
}
