param(
    [ValidateSet("Public", "PrivateBeta")]
    [string]$DistributionMode = "Public",
    [switch]$AcknowledgeGameDerivedAssets,
    [string]$PythonPath = "",
    [switch]$SkipInstaller
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path
$Version = "1.0.132-beta"
$BuildRoot = Join-Path ([IO.Path]::GetTempPath()) "CharacterModTool-build-$Version-$PID"
$TempDistRoot = Join-Path ([IO.Path]::GetTempPath()) "CharacterModTool-dist-$Version-$PID"
$DistRoot = Join-Path $Root "dist"
$ReleaseRoot = Join-Path $Root "release"
$PackageName = "CharacterModTool-v$Version-$DistributionMode"
$PackageRoot = Join-Path $ReleaseRoot $PackageName
$VenvRoot = Join-Path $Root ".release_venv"

if ($DistributionMode -eq "PrivateBeta" -and -not $AcknowledgeGameDerivedAssets) {
    throw "PrivateBeta includes review-required game-derived assets. Re-run with -AcknowledgeGameDerivedAssets after reading ASSET_DISTRIBUTION_POLICY.txt."
}

function Find-Python {
    if ($PythonPath -and (Test-Path -LiteralPath $PythonPath)) { return (Resolve-Path $PythonPath).Path }
    $venvPython = Join-Path $VenvRoot "Scripts\python.exe"
    if (Test-Path -LiteralPath $venvPython) { return $venvPython }
    $python = Get-Command python.exe -ErrorAction SilentlyContinue
    if ($python) { return $python.Source }
    $py = Get-Command py.exe -ErrorAction SilentlyContinue
    if ($py) { return $py.Source }
    throw "Python 3 was not found. Pass -PythonPath with a Python executable for the one-time build. End users do not need Python."
}

$BasePython = Find-Python
$BuildPython = Join-Path $VenvRoot "Scripts\python.exe"
if (-not (Test-Path -LiteralPath $BuildPython)) {
    if ([IO.Path]::GetFileName($BasePython).ToLowerInvariant() -eq "py.exe") {
        & $BasePython -3 -m venv $VenvRoot
    } else {
        & $BasePython -m venv $VenvRoot
    }
}
if (-not (Test-Path -LiteralPath $BuildPython)) { throw "Could not create the release Python environment." }

& $BuildPython -c "import PyInstaller"
if ($LASTEXITCODE -ne 0) {
    & $BuildPython -m pip install --disable-pip-version-check pyinstaller
    if ($LASTEXITCODE -ne 0) { throw "PyInstaller is unavailable and could not be installed." }
}

New-Item -ItemType Directory -Force -Path $BuildRoot,$TempDistRoot | Out-Null
& $BuildPython -m PyInstaller --noconfirm --clean --distpath $TempDistRoot --workpath $BuildRoot (Join-Path $Root "CharacterModTool.spec")
if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed with exit code $LASTEXITCODE. No release package was created." }

$BuiltAppRoot = Join-Path $TempDistRoot "CharacterModTool"
$BuiltExecutable = Join-Path $BuiltAppRoot "CharacterModTool.exe"
if (-not (Test-Path -LiteralPath $BuiltExecutable)) {
    throw "PyInstaller completed without creating CharacterModTool.exe."
}

$BuiltInternal = Join-Path $BuiltAppRoot "_internal"
$BuiltTkinter = Join-Path $BuiltInternal "_tkinter.pyd"
if (-not (Test-Path -LiteralPath $BuiltTkinter)) {
    $PythonRuntimeRoot = (& $BuildPython -c "import sys; print(sys.base_prefix)").Trim()
    $TkinterSource = Join-Path $PythonRuntimeRoot "Lib\tkinter"
    $PythonDllRoot = Join-Path $PythonRuntimeRoot "DLLs"
    $TclRoot = Join-Path $PythonRuntimeRoot "tcl"
    $RequiredTkSources = @(
        $TkinterSource,
        (Join-Path $PythonDllRoot "_tkinter.pyd"),
        (Join-Path $PythonDllRoot "tcl86t.dll"),
        (Join-Path $PythonDllRoot "tk86t.dll"),
        (Join-Path $TclRoot "tcl8.6"),
        (Join-Path $TclRoot "tk8.6"),
        (Join-Path $TclRoot "tcl8")
    )
    foreach ($source in $RequiredTkSources) {
        if (-not (Test-Path -LiteralPath $source)) {
            throw "PyInstaller excluded Tk and the portable Tk fallback is incomplete: $source"
        }
    }
    Copy-Item -LiteralPath $TkinterSource -Destination $BuiltInternal -Recurse -Force
    Copy-Item -LiteralPath (
        (Join-Path $PythonDllRoot "_tkinter.pyd"),
        (Join-Path $PythonDllRoot "tcl86t.dll"),
        (Join-Path $PythonDllRoot "tk86t.dll")
    ) -Destination $BuiltInternal -Force
    Copy-Item -LiteralPath (Join-Path $TclRoot "tcl8.6") -Destination (Join-Path $BuiltInternal "_tcl_data") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $TclRoot "tk8.6") -Destination (Join-Path $BuiltInternal "_tk_data") -Recurse -Force
    Copy-Item -LiteralPath (Join-Path $TclRoot "tcl8") -Destination (Join-Path $BuiltInternal "tcl8") -Recurse -Force
    Get-ChildItem -LiteralPath (Join-Path $BuiltInternal "tkinter") -Directory -Filter "__pycache__" -Recurse |
        Remove-Item -Recurse -Force
    Write-Host "PyInstaller excluded Tk; installed the portable Tk runtime fallback."
}

$selfTestProcess = Start-Process -FilePath $BuiltExecutable `
    -ArgumentList "--package-self-test" `
    -WorkingDirectory $BuiltAppRoot `
    -WindowStyle Hidden `
    -Wait `
    -PassThru
$selfTestProcess.Refresh()
$selfTestExitCode = $selfTestProcess.ExitCode
if ($null -eq $selfTestExitCode -or $selfTestExitCode -ne 0) {
    $selfTestReport = Join-Path ([IO.Path]::GetTempPath()) "CharacterModTool-package-self-test.txt"
    $details = if (Test-Path -LiteralPath $selfTestReport) { Get-Content -LiteralPath $selfTestReport -Raw } else { "No self-test report was created." }
    throw "Packaged backend self-test failed. No release package was created.`n$details"
}

$resolvedRelease = [IO.Path]::GetFullPath($ReleaseRoot)
$resolvedPackage = [IO.Path]::GetFullPath($PackageRoot)
if (-not $resolvedPackage.StartsWith($resolvedRelease, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to replace a package outside the release directory."
}
if (Test-Path -LiteralPath $PackageRoot) { Remove-Item -LiteralPath $PackageRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $PackageRoot | Out-Null
Copy-Item -Path (Join-Path $BuiltAppRoot "*") -Destination $PackageRoot -Recurse -Force
Copy-Item -LiteralPath (Join-Path $Root "Launch Character Mod Tool.bat") -Destination $PackageRoot -Force
Copy-Item -LiteralPath (Join-Path $Root "README_Character_Mod_Tool.txt") -Destination (Join-Path $PackageRoot "README.txt") -Force
Copy-Item -LiteralPath (Join-Path $Root "ASSET_DISTRIBUTION_POLICY.txt") -Destination $PackageRoot -Force
Copy-Item -LiteralPath (Join-Path $Root "THIRD_PARTY_NOTICES.txt") -Destination $PackageRoot -Force
New-Item -ItemType Directory -Force -Path (Join-Path $PackageRoot "outputs") | Out-Null

if ($DistributionMode -eq "Public") {
    $internal = Join-Path $PackageRoot "_internal"
    foreach ($relative in @("built_in_glasses", "built_in_headbands", "accessory_templates", "dynamic_body_package", "tools\live_roster")) {
        $candidate = Join-Path $internal $relative
        $resolvedCandidate = [IO.Path]::GetFullPath($candidate)
        if ($resolvedCandidate.StartsWith([IO.Path]::GetFullPath($internal), [StringComparison]::OrdinalIgnoreCase) -and (Test-Path -LiteralPath $candidate)) {
            Remove-Item -LiteralPath $candidate -Recurse -Force
        }
    }
    @"
This public package intentionally excludes review-required game-derived glasses,
headbands, accessory templates, and dynamic-body assets. See ASSET_DISTRIBUTION_POLICY.txt.
The affected controls remain unavailable until locally supplied assets are installed.
"@ | Set-Content -LiteralPath (Join-Path $PackageRoot "ASSETS_NOT_INCLUDED.txt") -Encoding ASCII
}

$ZipPath = Join-Path $ReleaseRoot "$PackageName.zip"
if (Test-Path -LiteralPath $ZipPath) { Remove-Item -LiteralPath $ZipPath -Force }
Compress-Archive -LiteralPath $PackageRoot -DestinationPath $ZipPath -CompressionLevel Optimal

$DistAppRoot = Join-Path $DistRoot "CharacterModTool"
if (Test-Path -LiteralPath $DistAppRoot) { Remove-Item -LiteralPath $DistAppRoot -Recurse -Force }
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null
Copy-Item -LiteralPath $BuiltAppRoot -Destination $DistRoot -Recurse -Force

if (-not $SkipInstaller) {
    $isccCandidates = @(
        "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
        "$env:ProgramFiles\Inno Setup 6\ISCC.exe"
    )
    $iscc = $isccCandidates | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -First 1
    if ($iscc) {
        & $iscc "/DSourceDir=$PackageRoot" "/DAppVersion=$Version" "/DOutputDir=$ReleaseRoot" (Join-Path $Root "CharacterModTool.iss")
    } else {
        Write-Host "Inno Setup was not found; portable EXE and ZIP were created."
    }
}

Write-Host "Release folder: $PackageRoot"
Write-Host "Portable ZIP: $ZipPath"

Remove-Item -LiteralPath $BuildRoot,$TempDistRoot -Recurse -Force -ErrorAction SilentlyContinue
