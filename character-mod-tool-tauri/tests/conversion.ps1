$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$project=Split-Path $root -Parent
$production=Join-Path $project 'character-mod-tool-webview2\dist\Character-Mod-Tool-0.1.31.exe'
$before=(Get-FileHash -LiteralPath $production).Hash
$build = Get-Content -LiteralPath (Join-Path $root 'dist\BUILD.json') -Raw | ConvertFrom-Json
$exe=Join-Path $root ('dist\Character-Mod-Tool-Tauri-Preview-' + $build.version + '.exe')
$job=Start-Process -FilePath $exe -ArgumentList '--script',('"'+(Join-Path $PSScriptRoot 'conversion.json')+'"') -WindowStyle Hidden -PassThru -Wait -RedirectStandardOutput (Join-Path $root 'artifacts\conversion.stdout.json') -RedirectStandardError (Join-Path $root 'artifacts\conversion.stderr.log')
if($job.ExitCode){Get-Content -LiteralPath (Join-Path $root 'artifacts\conversion.stderr.log') -Tail 15; throw 'Tauri conversion test failed'}
$result=Get-Content -LiteralPath (Join-Path $root 'artifacts\conversion.stdout.json') -Raw | ConvertFrom-Json
if($result.results.swap.validation.errors -ne 0){throw 'Conversion validation errors'}
if((Get-FileHash -LiteralPath $production).Hash -ne $before){throw 'Production executable changed'}
Write-Output ('Converted: '+$result.results.swap.output)
Write-Output ('Validation: '+$result.results.swap.validation.errors+' errors, '+$result.results.swap.validation.warnings+' warnings')
Write-Output ('Facial hair: '+$result.results.swap.facialHair.vertices+' vertices, '+$result.results.swap.facialHair.textures.Count+' textures')
Write-Output 'Production executable unchanged.'
