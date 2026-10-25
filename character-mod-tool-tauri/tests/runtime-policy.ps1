$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\build-tools\runtime-files.ps1')
foreach ($path in @(
    'python_runtime/Lib/site-packages/pip/__init__.py',
    'python_runtime\Lib\site-packages\PyInstaller\bootloader\run.exe',
    'python_runtime/Lib/site-packages/pip-26.1.2.dist-info/METADATA',
    'python_runtime/Lib/site-packages/distutils-precedence.pth',
    'python_runtime/Lib/site-packages/capstone/lib/capstone.dll'
)) {
    if (Test-PackagedRuntimeFile $path) { throw "Development payload was retained: $path" }
}
foreach ($path in @(
    'python_runtime/Lib/site-packages/PIL/Image.py',
    'python_runtime/Lib/site-packages/rarfile.py',
    'python_runtime/Lib/site-packages/zstandard/backend_c.pyd',
    'python_runtime/Lib/site-packages/pipper/__init__.py',
    'python_runtime/Lib/tkinter/__init__.py',
    'blender_tools/pip/custom.py',
    'tools/live_roster/roster.exe',
    'archive_reader/7z.exe',
    'built_in_headbands/template.iff'
)) {
    if (!(Test-PackagedRuntimeFile $path)) { throw "Functional payload was excluded: $path" }
}
Write-Output 'Runtime policy: scoped development-package exclusions preserve runtime codecs, Blender tools and assets.'
