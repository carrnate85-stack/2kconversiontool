$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$node = 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe'
$npmRoot = Join-Path $root 'build-tools\npm'
if (!(Test-Path -LiteralPath (Join-Path $npmRoot 'bin\npm-cli.js'))) {
    $meta = Invoke-RestMethod 'https://registry.npmjs.org/npm/11.11.0'
    $tar = Join-Path $root 'build-tools\npm.tgz'
    Invoke-WebRequest $meta.dist.tarball -OutFile $tar
    $actual = [Convert]::ToBase64String([Security.Cryptography.SHA512]::HashData([IO.File]::ReadAllBytes($tar)))
    if (('sha512-' + $actual) -ne $meta.dist.integrity) { throw 'npm download integrity failed' }
    New-Item -ItemType Directory -Path $npmRoot -Force | Out-Null
    & tar -xzf $tar --strip-components=1 -C $npmRoot
    if ($LASTEXITCODE) { throw 'npm extraction failed' }
    Remove-Item -LiteralPath $tar
}
Push-Location (Join-Path $root 'build-tools')
try {
    & $node (Join-Path $npmRoot 'bin\npm-cli.js') install --no-audit --no-fund
    if ($LASTEXITCODE) { throw 'Development dependencies failed' }
} finally { Pop-Location }
