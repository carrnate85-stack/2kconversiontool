param([Parameter(Mandatory)][string]$FixtureRoot)
$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$fixture=(Resolve-Path -LiteralPath $FixtureRoot).Path
$allowed=(Resolve-Path -LiteralPath (Join-Path $project 'artifacts')).Path+[IO.Path]::DirectorySeparatorChar
if (-not $fixture.StartsWith($allowed,[StringComparison]::OrdinalIgnoreCase)) {throw 'Use an isolated artifacts fixture'}
$row=Get-Content -LiteralPath (Join-Path $fixture 'Logs/browser-workspace-session.json') -Raw|ConvertFrom-Json
$uri=[uri]$row.url
if ($uri.Scheme -ne 'http' -or $uri.Host -ne '127.0.0.1' -or $uri.Port -lt 1024 -or $uri.UserInfo) {throw 'Fixture must be owned loopback HTTP'}
$origin=$uri.GetLeftPart([System.UriPartial]::Authority)
$token=$uri.Fragment.Substring(1)
$tabOwner=[guid]::NewGuid().ToString()
$handler=[System.Net.Http.HttpClientHandler]::new()
$handler.AllowAutoRedirect=$false
$handler.UseProxy=$false
$client=[System.Net.Http.HttpClient]::new($handler)
$client.Timeout=[TimeSpan]::FromSeconds(3)
function Rpc([string]$Method,$Argument,[string]$RequestOrigin=$origin,[string]$RequestToken=$token) {
    $request=[System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Post,($origin+'/rpc'))
    $null=$request.Headers.TryAddWithoutValidation('Origin',$RequestOrigin)
    $null=$request.Headers.TryAddWithoutValidation('X-Roster-Token',$RequestToken)
    $null=$request.Headers.TryAddWithoutValidation('X-Animation-Owner',$tabOwner)
    $body=@{method=$Method;args=@($Argument)}|ConvertTo-Json -Depth 20 -Compress
    if($null -eq $Argument){$body=@{method=$Method;args=@()}|ConvertTo-Json -Compress}
    $request.Content=[System.Net.Http.StringContent]::new($body,[Text.Encoding]::UTF8,'application/json')
    $request.Content.Headers.ContentType=[System.Net.Http.Headers.MediaTypeHeaderValue]::new('application/json')
    $response=$client.SendAsync($request).GetAwaiter().GetResult()
    try {
        $text=$response.Content.ReadAsStringAsync().GetAwaiter().GetResult()
        [pscustomobject]@{status=[int]$response.StatusCode;data=$(if($text.StartsWith('{')){$text|ConvertFrom-Json}else{$text})}
    } finally {$response.Dispose();$request.Dispose()}
}
try {
    $bootstrap=Rpc 'rosterBootstrap' $null
    if($bootstrap.status -ne 200 -or $bootstrap.data.result.playerResearch.status -ne 'idle'){throw 'Unexpected live state'}
    $status=Rpc 'player23Catalog' @{action='status'}
    $timer=[Diagnostics.Stopwatch]::StartNew()
    $search=Rpc 'player23Catalog' @{action='search';catalogId=$status.data.result.catalogId;query='Curry Stephen';limit=40;offset=0;category='current'}
    $timer.Stop()
    if($search.status -ne 200 -or $search.data.result.players.Count -ne 2){throw 'Search mismatch'}
    $review=Rpc 'player23Catalog' @{action='player';catalogId=$status.data.result.catalogId;playerId=512}
    if($review.status -ne 200 -or $review.data.result.seasons.Count -ne 13){throw 'Review mismatch'}
    $selected=Rpc 'player23Catalog' @{action='selection';catalogId=$status.data.result.catalogId;playerId=512;sections=@('info','attributes');seasonIds=@();includeAwards=$false}
    if($selected.status -ne 200 -or $selected.data.result.data.info.firstName -ne 'Stephen' -or @($selected.data.result.data.fields.PSObject.Properties).Count -lt 30 -or $selected.data.result.data.seasonHistory){throw 'Category selection mismatch'}
    $season=Rpc 'player23Catalog' @{action='selection';catalogId=$status.data.result.catalogId;playerId=512;sections=@('seasons');seasonIds=@($review.data.result.seasons[0].id);includeAwards=$false}
    if($season.status -ne 200 -or $season.data.result.data.version -ne 2 -or @($season.data.result.data.seasonHistory).Count -ne 1 -or $season.data.result.data.seasonHistory[0].awards){throw 'Portable source-season selection mismatch'}
    $blocked=Rpc 'savePlayer' @{recordIndex=0}
    if($blocked.status -ne 400 -or $blocked.data.error -notlike '*not ported*'){throw 'Game save capability was not absent'}
    $teamBlocked=Rpc 'saveTeam' @{recordIndex=0}
    if($teamBlocked.status -ne 400 -or $teamBlocked.data.error -notlike '*not ported*'){throw 'Team game save capability was not absent in the no-game fixture'}
    $uniformBlocked=Rpc 'saveTeamUniform' @{recordIndex=0}
    if($uniformBlocked.status -ne 400 -or $uniformBlocked.data.error -notlike '*not ported*'){throw 'Uniform game save capability was not absent in the no-game fixture'}
    $foreign=Rpc 'ping' $null 'https://example.invalid'
    if($foreign.status -ne 403){throw 'Foreign origin was not blocked'}
    $wrongToken=Rpc 'ping' $null $origin ('f'*64)
    if($wrongToken.status -ne 403){throw 'Wrong token was not blocked'}
    $page=$client.GetAsync($row.url.Split('#')[0]).GetAwaiter().GetResult()
    $html=$page.Content.ReadAsStringAsync().GetAwaiter().GetResult()
    if([int]$page.StatusCode -ne 200 -or -not $html.Contains('roster-browser-bridge.js') -or -not $html.Contains('rust-browser-adapter.js')){throw 'Missing original bridge/native adapter'}
    if(-not ($page.Headers.GetValues('Content-Security-Policy') -join '').Contains("frame-ancestors 'none'")){throw 'Missing frame protection'}
    $page.Dispose()
    $manifest=Get-Content -LiteralPath (Join-Path $project 'assets/wpf-browser-manifest.json') -Raw|ConvertFrom-Json
    $verified=0
    foreach($asset in $manifest.files) {
        if($asset.path -eq '/renderer/index.html'){continue} # Only bridge/sample bootstrap inserted in served index.
        $response=$client.GetAsync($origin+$asset.path).GetAwaiter().GetResult()
        try {
            if([int]$response.StatusCode -ne 200){throw ('Missing asset: '+$asset.path)}
            $bytes=$response.Content.ReadAsByteArrayAsync().GetAwaiter().GetResult()
            $hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($bytes)).ToLowerInvariant()
            if($hash -ne $asset.sha256){throw ('Changed asset: '+$asset.path)}
            $verified++
        } finally {$response.Dispose()}
    }
    $result=[ordered]@{pass=$true;gameAccess=$false;originalAssetsVerified=$verified;sourceAssets=$manifest.files.Count;searchPlayers=$search.data.result.players.Count;searchMs=$timer.Elapsed.TotalMilliseconds;reviewSeasons=$review.data.result.seasons.Count;sourceCategorySelection=$selected.status;portableSeasonSelection=$season.status;saveBlocked=$blocked.status;teamSaveBlocked=$teamBlocked.status;uniformSaveBlocked=$uniformBlocked.status;foreignOriginBlocked=$foreign.status;wrongTokenBlocked=$wrongToken.status;browserVisualVerified=$false;utc=[DateTime]::UtcNow.ToString('o')}
    $json=$result|ConvertTo-Json
    [IO.File]::WriteAllText((Join-Path $fixture 'Logs/browser-http-check.json'),$json,[Text.Encoding]::UTF8)
    $json
} finally {$client.Dispose();$handler.Dispose()}
