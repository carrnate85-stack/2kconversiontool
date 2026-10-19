$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$source=Join-Path (Split-Path $project -Parent) 'nba2k27-camera-tool/src/Program.cs'
$fixture=Join-Path $project ('artifacts/camera-policy-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$before=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
$csc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$library=Join-Path $fixture 'CameraPolicyReference.dll'
$runner=Join-Path $fixture 'CameraPolicyFixture.exe'
& $csc /nologo /target:library /platform:x64 /optimize+ ("/out:"+$library) /r:System.dll /r:System.Core.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll $source *> (Join-Path $fixture 'reference-compile.log')
if($LASTEXITCODE -ne 0){throw 'Original camera reference compilation failed'}
& $csc /nologo /target:exe /platform:x64 ("/out:"+$runner) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll /r:Microsoft.CSharp.dll (Join-Path $project 'test/CameraPolicyFixture.cs')
if($LASTEXITCODE -ne 0){throw 'Pure camera oracle fixture compilation failed'}
$raw=& $runner $library $source $fixture
if($LASTEXITCODE -ne 0){throw 'Original camera transform fixture failed'}
$data=$raw|ConvertFrom-Json
if($data.schema -ne 'nba2k27-camera-policy-oracle-v1' -or $data.gameAccess -ne $false -or $data.metadata.Count -ne 11 -or $data.press.Count -ne 96 -or $data.beluba.Count -ne 64 -or $data.steps.rows.Count -ne 144){throw 'Unexpected camera oracle contract'}
if((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne $before){throw 'Original camera reference changed during oracle compilation'}
$data|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue $before
[IO.File]::WriteAllText((Join-Path $project 'assets/camera-policy-regressions.json'),($data|ConvertTo-Json -Depth 9),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$fixture;metadata=$data.metadata.Count;press=$data.press.Count;beluba=$data.beluba.Count;steps=$data.steps.rows.Count;sourceSha256=$before;gameAccess=$false;originalHelperChanged=$false}|ConvertTo-Json -Compress
