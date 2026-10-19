# Engineering-only primitives. Production entry point has fixed paths and no
# fixture/fault arguments. Tests dot-source this file with synthetic trees only.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not ('ReviewedMainFs' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Text;
using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;
public sealed class ReviewedMainPin : IDisposable {
    public SafeFileHandle Handle;
    public string Path;
    public string Id;
    public bool Directory;
    public FileStream Stream;
    public void Dispose() { if(Stream != null) Stream.Dispose(); else if(Handle != null) Handle.Dispose(); }
}
public static class ReviewedMainFs {
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern SafeFileHandle CreateFileW(string name,uint access,uint share,IntPtr sec,uint creation,uint flags,IntPtr template);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandleEx(SafeFileHandle h,int cls,byte[] data,uint bytes);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool GetFileInformationByHandle(SafeFileHandle h,out Info data);
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool SetFileInformationByHandle(SafeFileHandle h,int cls,byte[] data,uint bytes);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern uint GetFinalPathNameByHandleW(SafeFileHandle h,StringBuilder data,uint bytes,uint flags);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool CreateDirectoryW(string name,IntPtr security);
    [StructLayout(LayoutKind.Sequential)] struct Info {
        public uint Attr,CreationLo,CreationHi,AccessLo,AccessHi,WriteLo,WriteHi,Volume,SizeHi,SizeLo,Links,IndexHi,IndexLo;
    }
    static void Win(bool ok) { if(!ok) throw new Win32Exception(Marshal.GetLastWin32Error()); }
    public static string Full(string p) {
        if(String.IsNullOrEmpty(p) || !System.IO.Path.IsPathRooted(p)) throw new IOException("Absolute path required");
        string s=System.IO.Path.GetFullPath(p).TrimEnd('\\');
        if(s.Length < 4 || s.StartsWith("\\\\") || s.Substring(2).Contains(":")) throw new IOException("Local fixed-volume path required");
        return s;
    }
    public static ReviewedMainPin Open(string path,bool directory,bool rename) {return OpenCore(path,directory,rename,false,false);}
    public static ReviewedMainPin Inventory(string path,bool directory) {return OpenCore(path,directory,false,false,true);}
    public static ReviewedMainPin Parent(string path) {return OpenCore(path,true,false,true,false);}
    public static void NewDirectory(string path) {Win(CreateDirectoryW("\\\\?\\"+Full(path),IntPtr.Zero));}
    static ReviewedMainPin OpenCore(string path,bool directory,bool rename,bool parent,bool inventory) {
        path=Full(path);
        // Deny writes. Windows requires all descendant handles closed for a
        // whole directory move, so inventory descendants are released just
        // before it; membership, hash and stable IDs are checked on both sides.
        uint access=directory ? 0x80u : 0x80000000u;
        if(rename) access |= 0x10000u;
        // Verifier directory handles share DELETE with our retained renamer;
        // immutable ancestor pins and the actual renamer deny it to outsiders.
        uint share=directory ? ((rename||parent)?3u:7u) : (inventory?5u:1u);
        var h=CreateFileW("\\\\?\\"+path,access,share,IntPtr.Zero,3,0x00200000u | (directory?0x02000000u:0),IntPtr.Zero);
        if(h.IsInvalid) {h.Dispose();throw new Win32Exception(Marshal.GetLastWin32Error());}
        try {
            Info info;Win(GetFileInformationByHandle(h,out info));
            if((info.Attr&0x400)!=0 || ((info.Attr&0x10)!=0)!=directory || (!directory && info.Links!=1))
                throw new IOException("Only ordinary, single-link files and ordinary directories are permitted");
            var id=new byte[24];Win(GetFileInformationByHandleEx(h,18,id,24));
            var final=new StringBuilder(32768);uint n=GetFinalPathNameByHandleW(h,final,32768,0);
            if(n==0 || n>=32768) throw new IOException("Cannot resolve retained path");
            string actual=final.ToString();if(actual.StartsWith("\\\\?\\"))actual=actual.Substring(4);
            if(!String.Equals(Full(actual),path,StringComparison.OrdinalIgnoreCase))throw new IOException("Resolved path changed");
            var p=new ReviewedMainPin{Handle=h,Path=path,Id=BitConverter.ToString(id).Replace("-",""),Directory=directory};
            if(!directory)p.Stream=new FileStream(h,FileAccess.Read);
            return p;
        } catch {h.Dispose();throw;}
    }
    public static void Rename(ReviewedMainPin source,string destination) {
        if(!source.Directory || source.Handle.IsClosed)throw new IOException("Retained directory required");
        destination=Full(destination);
        if(File.Exists(destination)||System.IO.Directory.Exists(destination))throw new IOException("Rename destination exists");
        byte[] name=Encoding.Unicode.GetBytes(destination);
        int offset=IntPtr.Size==8?20:12;var bytes=new byte[offset+name.Length+2];
        // ReplaceIfExists stays false. RootDirectory stays null; exact absolute
        // same-volume paths are checked by the caller before every move.
        Array.Copy(BitConverter.GetBytes(name.Length),0,bytes,IntPtr.Size==8?16:8,4);
        Array.Copy(name,0,bytes,offset,name.Length);
        Win(SetFileInformationByHandle(source.Handle,3,bytes,(uint)bytes.Length));
        source.Path=destination;
    }
}
'@
}

function Get-PromotionPath([string]$Root,[string]$Relative) {
    $base=[ReviewedMainFs]::Full($Root)
    $path=[ReviewedMainFs]::Full([IO.Path]::Combine($base,$Relative))
    if (-not $path.StartsWith($base+'\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Path escaped exact promotion root' }
    return $path
}
function Assert-PromotionAbsent([string]$Path) {
    # Get-Item sees dangling reparse points that Test-Path may hide.
    $item=Get-Item -LiteralPath $Path -Force -ErrorAction SilentlyContinue
    if ($null -ne $item) { throw "Existing destination is never replaced: $Path" }
    if ([IO.File]::Exists($Path) -or [IO.Directory]::Exists($Path)) { throw 'Existing destination is never replaced' }
}
function Get-PromotionHash([ReviewedMainPin]$Pin) {
    $Pin.Stream.Position=0
    $sha=[Security.Cryptography.SHA256]::Create()
    try { $digest=[Convert]::ToHexString($sha.ComputeHash($Pin.Stream)) } finally { $sha.Dispose() }
    $Pin.Stream.Position=0
    return $digest
}
function Get-PromotionText([ReviewedMainPin]$Pin) {
    if ($Pin.Stream.Length -gt 4194304) { throw 'Bounded release metadata exceeded 4 MiB' }
    $Pin.Stream.Position=0
    $reader=[IO.StreamReader]::new($Pin.Stream,[Text.UTF8Encoding]::new($false,$true),$false,4096,$true)
    try { $result=$reader.ReadToEnd() } finally { $reader.Dispose() }
    $Pin.Stream.Position=0
    return $result
}
function Open-PromotionAncestors([string]$Path,[Collections.Generic.List[IDisposable]]$Pins) {
    $current=[ReviewedMainFs]::Full($Path)
    $paths=[Collections.Generic.List[string]]::new()
    while ($current.Length -gt 3) { $paths.Add($current);$current=[IO.Path]::GetDirectoryName($current) }
    for ($i=$paths.Count-1;$i -ge 0;$i--) { $Pins.Add([ReviewedMainFs]::Parent($paths[$i])) }
}
function Open-PromotionTree([string]$Root,[bool]$Rename=$false) {
    $rootPath=[ReviewedMainFs]::Full($Root)
    $pins=[Collections.Generic.List[IDisposable]]::new()
    try {
        $top=[ReviewedMainFs]::Open($rootPath,$true,$Rename);$pins.Add($top)
        $rows=[Collections.Generic.List[object]]::new()
        $pending=[Collections.Generic.Queue[string]]::new();$pending.Enqueue('')
        while ($pending.Count -gt 0) {
            $rel=$pending.Dequeue();$dir=if($rel){Get-PromotionPath $rootPath $rel}else{$rootPath}
            foreach ($item in Get-ChildItem -LiteralPath $dir -Force -ErrorAction Stop) {
                if ($pins.Count -ge 100000) { throw 'App inventory exceeds bounded count' }
                $relative=if($rel){$rel+'/'+$item.Name}else{$item.Name}
                $path=Get-PromotionPath $rootPath $relative
                $pin=[ReviewedMainFs]::Inventory($path,$item.PSIsContainer);$pins.Add($pin)
                if ($item.PSIsContainer) {
                    $rows.Add([pscustomobject]@{path=$relative;kind='directory';bytes=0;sha256=$null;id=$pin.Id})
                    $pending.Enqueue($relative)
                } else {
                    $rows.Add([pscustomobject]@{path=$relative;kind='file';bytes=$pin.Stream.Length;sha256=(Get-PromotionHash $pin);id=$pin.Id})
                }
            }
        }
        $sort=[Collections.Generic.SortedDictionary[string,object]]::new([StringComparer]::Ordinal)
        foreach($row in $rows){$sort.Add($row.path,$row)}
        $ordered=@($sort.Values)
        $canonical=($ordered|ForEach-Object {$_.kind+'|'+$_.path+'|'+$_.bytes+'|'+$_.sha256+'|'+$_.id}) -join "`n"
        $sha=[Security.Cryptography.SHA256]::Create()
        try{$digest=[Convert]::ToHexString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($canonical)))}finally{$sha.Dispose()}
        return [pscustomobject]@{root=$top;rows=$ordered;digest=$digest;pins=$pins}
    } catch {foreach($p in $pins){$p.Dispose()};throw}
}
function Close-PromotionTree($Tree) { if($null -ne $Tree){foreach($p in $Tree.pins){$p.Dispose()}} }
function Release-PromotionChildren($Tree) {
    foreach($p in $Tree.pins){if(-not [Object]::ReferenceEquals($p,$Tree.root)){$p.Dispose()}}
    $Tree.pins.Clear();$Tree.pins.Add($Tree.root)
}
function Assert-PromotionTree($Tree,[string]$CurrentPath) {
    # Directory is retained, and every file denies writers. Recheck
    # membership to catch new children, plus stable IDs for every old node.
    $current=Open-PromotionTree $CurrentPath
    try { if($current.root.Id -ne $Tree.root.Id -or $current.digest -ne $Tree.digest){throw 'Pinned App inventory changed'} }
    finally {Close-PromotionTree $current}
}
function Assert-PromotionProcesses($Rows) {
    foreach($p in $Rows) {
        $name=([string]$p.Name).ToLowerInvariant()
        $path=([string]$p.ExecutablePath).Replace('/','\').ToLowerInvariant()
        $cmd=([string]$p.CommandLine).Replace('/','\').ToLowerInvariant()
        if ($name -eq 'nba2k27.exe' -or $name.StartsWith('easyanticheat') -or
            $name -in @('eaclaunch.exe','start_protected_game.exe','2k launcher.exe','2k launcher wpf.exe','2k launcher rust.exe','2k launcher rust prototype.exe','nba2k27-launcher-tauri2.exe') -or
            ($name.StartsWith('2k-launcher-') -and $name.EndsWith('.exe'))) { throw "Close game/protected process/launcher first (PID $($p.ProcessId))" }
        if ($name -in @('node.exe','electron.exe')) {
            if(-not $path -or $path.EndsWith('\app\backend\node.exe') -or
                $path.Contains('\nba2k27-mod-launcher\') -or $cmd.Contains('\nba2k27-mod-launcher\') -or
                $path.Contains('\2k launcher\app\') -or $cmd.Contains('\2k launcher\app\')) {
                throw "Old/unknown launcher runtime must be closed first (PID $($p.ProcessId))"
            }
        }
    }
}
function Assert-PromotionClosed { Assert-PromotionProcesses @(Get-CimInstance -ClassName Win32_Process -ErrorAction Stop | Select-Object Name,ExecutablePath,CommandLine,ProcessId) }
function Write-PromotionEvent([IO.FileStream]$Receipt,[string]$Event,$Payload) {
    $line=[ordered]@{event=$Event;utc=[DateTime]::UtcNow.ToString('o');data=$Payload}|ConvertTo-Json -Depth 14 -Compress
    $bytes=[Text.UTF8Encoding]::new($false).GetBytes($line+"`n")
    $Receipt.Write($bytes,0,$bytes.Length);$Receipt.Flush($true)
}
function Copy-PromotionFile([ReviewedMainPin]$Source,[string]$Destination) {
    Assert-PromotionAbsent $Destination
    $output=[IO.File]::Open($Destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
    try{$Source.Stream.Position=0;$Source.Stream.CopyTo($output);$output.Flush($true)}finally{$output.Dispose();$Source.Stream.Position=0}
    $target=[ReviewedMainFs]::Open($Destination,$false,$false)
    try{if($target.Stream.Length -ne $Source.Stream.Length -or (Get-PromotionHash $target) -ne (Get-PromotionHash $Source)){throw 'Staged release copy differs'}}finally{$target.Dispose()}
}

function Open-PromotionRelease([string]$Build,[Collections.Generic.List[IDisposable]]$Pins) {
    $buildPath=[ReviewedMainFs]::Full($Build)
    Open-PromotionAncestors $buildPath $Pins
    $files=[Collections.Generic.SortedDictionary[string,object]]::new([StringComparer]::Ordinal)
    $inputs=@{
        '2k Launcher Rust.exe'='Portable/App/2k Launcher Rust Prototype.exe'
        'BUILD-INFO.json'='Portable/BUILD-INFO.json'
        'FEATURE-PARITY.json'='Portable/FEATURE-PARITY.json'
        'QA-REPORT.md'='Portable/QA-REPORT.md'
        'README.md'='Portable/README.md'
        'THIRD-PARTY-NOTICES.txt'='Portable/THIRD-PARTY-NOTICES.txt'
    }
    foreach($dir in @('Portable','Portable/App')){$Pins.Add([ReviewedMainFs]::Open((Get-PromotionPath $buildPath $dir),$true,$false))}
    foreach($destination in $inputs.Keys){
        $pin=[ReviewedMainFs]::Open((Get-PromotionPath $buildPath $inputs[$destination]),$false,$false);$Pins.Add($pin);$files.Add($destination,$pin)
    }
    $appItems=@(Get-ChildItem -LiteralPath (Get-PromotionPath $buildPath 'Portable/App') -Force)
    if($appItems.Count -ne 1 -or $appItems[0].Name -cne '2k Launcher Rust Prototype.exe'){throw 'QA App must contain only the verified Rust executable'}
    $qaPin=[ReviewedMainFs]::Open((Get-PromotionPath $buildPath 'QA-VERIFICATION.json'),$false,$false);$Pins.Add($qaPin)
    $mp4Pin=[ReviewedMainFs]::Open((Get-PromotionPath $buildPath 'SAVED-MP4-VERIFICATION.json'),$false,$false);$Pins.Add($mp4Pin)
    $info=Get-PromotionText $files['BUILD-INFO.json']|ConvertFrom-Json -ErrorAction Stop
    $qa=Get-PromotionText $qaPin|ConvertFrom-Json -ErrorAction Stop
    $mp4=Get-PromotionText $mp4Pin|ConvertFrom-Json -ErrorAction Stop
    $exe=$files['2k Launcher Rust.exe'];$hash=Get-PromotionHash $exe
    if($info.version -cne '0.36.0' -or $info.sha256 -ne $hash -or $info.bytes -ne $exe.Stream.Length -or $info.isolatedUiVerified -ne $true -or $info.uiChecks -lt 105){throw 'Build information does not prove this exact verified 0.36.0 executable'}
    if($qa.version -cne '0.36.0' -or $qa.exeSha256 -ne $hash -or $qa.exeBytes -ne $exe.Stream.Length -or $qa.pipelineBuildVerified -ne $true -or $qa.pipelineUiChecks -ne $info.uiChecks -or
        $qa.http.pass -ne $true -or $qa.httpFixtureExited -ne $true -or $qa.providerWorkerFixtureExited -ne $true -or $qa.providerWorker.passed -ne $true -or
        $qa.testOnlyMarkersAbsentFromRelease -ne $true -or $qa.dataAndLibraryPreserved -ne $true -or $qa.automatedGameAccess -ne $false -or
        $qa.gameVersion2LegacyMemoryWriteGateEnabled -ne $false -or @($qa.repeats).Count -lt 3){throw 'QA evidence failed or belongs to a different executable'}
    foreach($row in $qa.repeats){if($row.exited -ne $true -or $row.wpfValidatorsPassed -ne $true -or $row.checks -ne $info.uiChecks){throw 'QA repeat evidence is incomplete'}}
    if($mp4.version -cne '0.36.0' -or $mp4.exeSha256 -ne $hash -or $mp4.pass -ne $true -or $mp4.decodedVideos -ne 4 -or $mp4.decodedFramePositions -ne 12 -or
        $mp4.audioOutput -ne $false -or $mp4.gameAccess -ne $false -or $mp4.mainChanged -ne $false -or @($mp4.rows).Count -ne 4){throw 'Saved MP4 verification failed or belongs to a different executable'}
    foreach($row in $mp4.rows){if($row.passed -ne $true -or @($row.points).Count -ne 3 -or $row.width -le 0 -or $row.height -le 0){throw 'Saved MP4 frame evidence is incomplete'}}
    return [pscustomobject]@{files=$files;exeHash=$hash;evidence=@{buildInfo=(Get-PromotionHash $files['BUILD-INFO.json']);qa=(Get-PromotionHash $qaPin);savedMp4=(Get-PromotionHash $mp4Pin)}}
}

function Invoke-ReviewedPromotionCore {
    param([Parameter(Mandatory)][string]$Root,[Parameter(Mandatory)][string]$Build,
        [Parameter(Mandatory)][string]$ExpectedOldExeSha,[Parameter(Mandatory)][scriptblock]$MediaCheck,
        [scriptblock]$ClosedCheck={Assert-PromotionClosed},[switch]$Commit,
        # Internal fixture-only fault hook; the production wrapper never exposes it.
        [scriptblock]$Fault={param($Point)})
    $pins=[Collections.Generic.List[IDisposable]]::new();$old=$null;$staged=$null;$receipt=$null;$receiptPin=$null
    $movedOld=$false;$movedNew=$false;$rollback=$false;$failure=$null
    $rootPath=[ReviewedMainFs]::Full($Root)
    $app=Get-PromotionPath $rootPath 'App'
    $backup=Get-PromotionPath $rootPath 'Recovery/App-before-Rust-0.36.0-20261016'
    $receiptPath=Get-PromotionPath $rootPath 'Recovery/rust-main-app-promotion-0.36.0-20261016.jsonl'
    $stage=Get-PromotionPath $rootPath ('Recovery/App-next-Rust-0.36.0-20261016-'+[guid]::NewGuid().ToString('N'))
    try {
        Open-PromotionAncestors $rootPath $pins
        $namespace=@{}
        $rootPin=[ReviewedMainFs]::Parent($rootPath);$pins.Add($rootPin);$namespace['root']=$rootPin.Id
        foreach($rel in @('Recovery','Data','Library')){$parent=[ReviewedMainFs]::Parent((Get-PromotionPath $rootPath $rel));$pins.Add($parent);$namespace[$rel]=$parent.Id}
        if([IO.Directory]::Exists((Get-PromotionPath $rootPath 'Runtime'))){$parent=[ReviewedMainFs]::Parent((Get-PromotionPath $rootPath 'Runtime'));$pins.Add($parent);$namespace['Runtime']=$parent.Id}
        Assert-PromotionAbsent $backup;Assert-PromotionAbsent $receiptPath;Assert-PromotionAbsent $stage
        & $ClosedCheck
        $release=Open-PromotionRelease $Build $pins
        $old=Open-PromotionTree $app $true
        $oldExe=@($old.rows|Where-Object {$_.path -ceq '2k Launcher WPF.exe' -and $_.kind -ceq 'file'})
        if($oldExe.Count -ne 1 -or $oldExe[0].sha256 -ne $ExpectedOldExeSha){throw 'Main App is not the exact reviewed WPF build'}
        $settingsPath=Get-PromotionPath $rootPath 'Data/settings.json'
        $settings=[ReviewedMainFs]::Open($settingsPath,$false,$false);$pins.Add($settings)
        $settingsHash=Get-PromotionHash $settings
        # The publisher performs full retained-identity/hash verification of all
        # new media units and original MP4s. A completion JSON alone is NOT proof.
        # Pin the receipt BEFORE the independent inspector reads it, closing
        # the check-then-replace gap between its verification and our transaction.
        $mediaPin=[ReviewedMainFs]::Open((Get-PromotionPath $rootPath 'Recovery/rust-saved-media-publication.jsonl'),$false,$false);$pins.Add($mediaPin)
        $mediaReceiptSha=Get-PromotionHash $mediaPin
        $media=& $MediaCheck
        if($media.phase -cne 'complete-verified' -or $media.units -ne 5 -or $media.reusedVideos -ne 4 -or $media.stageInventorySha256 -ne '76b3d222e19e7c6f577452a3aee3f3e8964ab52fb5199b0b69e1778832ca8ef0' -or
            $media.originalsUnchanged -ne $true -or $media.settingsPreserved -ne $true -or $media.gameAccess -ne $false -or $media.appChanged -ne $false){throw 'Saved media publisher is not complete-verified'}
        Assert-PromotionTree $old $app
        if(-not $Commit){return [pscustomobject]@{phase='dry-run-verified';version='0.36.0';writes=$false;oldAppFiles=@($old.rows|Where-Object kind -eq 'file').Count;oldAppManifestSha256=$old.digest;newExeSha256=$release.exeHash;mediaPhase=$media.phase;backup=$backup}}
        & $ClosedCheck
        # CREATE_NEW and directory creation without clobber; every future effect
        # has a flushed intent first. Partial staging is retained, not deleted.
        $receipt=[IO.File]::Open($receiptPath,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
        Write-PromotionEvent $receipt 'reviewed' @{version='0.36.0';root=$rootPath;namespaceIds=$namespace;app=$app;backup=$backup;stage=$stage;oldRootId=$old.root.Id;oldManifestSha256=$old.digest;oldManifest=$old.rows;newExeSha256=$release.exeHash;evidence=$release.evidence;mediaReceiptId=$mediaPin.Id;mediaReceiptSha256=$mediaReceiptSha;settingsId=$settings.Id;settingsSha256=$settingsHash}
        Write-PromotionEvent $receipt 'stage-intent' @{path=$stage}
        Assert-PromotionAbsent $stage
        # Native CreateDirectory refuses an existing name (no merge/reuse).
        [ReviewedMainFs]::NewDirectory($stage)
        $stagePin=[ReviewedMainFs]::Open($stage,$true,$true);$pins.Add($stagePin)
        if(@(Get-ChildItem -LiteralPath $stage -Force).Count -ne 0){throw 'New staging directory is not empty'}
        Write-PromotionEvent $receipt 'stage-owned' @{id=$stagePin.Id;path=$stage}
        foreach($name in $release.files.Keys){
            Write-PromotionEvent $receipt 'copy-intent' @{name=$name;bytes=$release.files[$name].Stream.Length;sha256=(Get-PromotionHash $release.files[$name])}
            & $Fault ('before-copy:'+$name)
            Copy-PromotionFile $release.files[$name] (Get-PromotionPath $stage $name)
            Write-PromotionEvent $receipt 'copy-verified' @{name=$name}
            & $Fault ('after-copy:'+$name)
        }
        $staged=Open-PromotionTree $stage
        if($staged.rows.Count -ne $release.files.Count){throw 'Staged App has extra or missing files'}
        Write-PromotionEvent $receipt 'stage-verified' @{id=$stagePin.Id;manifestSha256=$staged.digest;manifest=$staged.rows}
        & $Fault 'staged'
        & $ClosedCheck
        Assert-PromotionTree $old $app;Assert-PromotionTree $staged $stage
        if((Get-PromotionHash $settings) -ne $settingsHash -or (Get-PromotionHash $mediaPin) -ne $mediaReceiptSha){throw 'Settings/media evidence changed'}
        Assert-PromotionAbsent $backup
        Write-PromotionEvent $receipt 'old-app-move-intent' @{from=$app;to=$backup;id=$old.root.Id;manifestSha256=$old.digest}
        & $Fault 'before-old-rename'
        Assert-PromotionAbsent $backup;Assert-PromotionTree $old $app
        Release-PromotionChildren $old
        [ReviewedMainFs]::Rename($old.root,$backup);$movedOld=$true
        Write-PromotionEvent $receipt 'old-app-moved' @{id=$old.root.Id}
        & $Fault 'after-old-rename'
        & $ClosedCheck
        Assert-PromotionAbsent $app;Assert-PromotionTree $old $backup;Assert-PromotionTree $staged $stage
        Write-PromotionEvent $receipt 'new-app-move-intent' @{from=$stage;to=$app;id=$stagePin.Id;manifestSha256=$staged.digest}
        & $Fault 'before-new-rename'
        Assert-PromotionAbsent $app;Assert-PromotionTree $staged $stage
        Release-PromotionChildren $staged
        [ReviewedMainFs]::Rename($stagePin,$app);$movedNew=$true
        Write-PromotionEvent $receipt 'new-app-moved' @{id=$stagePin.Id}
        & $Fault 'after-new-rename'
        Assert-PromotionTree $old $backup;Assert-PromotionTree $staged $app
        if((Get-PromotionHash $settings) -ne $settingsHash){throw 'Original WPF settings changed'}
        Write-PromotionEvent $receipt 'complete' @{oldBackup=$backup;newApp=$app;newExeSha256=$release.exeHash;dataLibraryRuntimeUntouched=$true}
        return [pscustomobject]@{phase='complete';version='0.36.0';app=$app;executable=(Get-PromotionPath $app '2k Launcher Rust.exe');oldBackup=$backup;receipt=$receiptPath;newExeSha256=$release.exeHash;dataLibraryRuntimeUntouched=$true}
    } catch {
        $failure=$_
        if($null -ne $receipt){try{Write-PromotionEvent $receipt 'held-error' @{message=$_.Exception.Message;oldMoved=$movedOld;newMoved=$movedNew}}catch{}}
        if($movedOld -and -not $movedNew){
            try {
                # Roll back only our retained exact old directory, only while
                # App is absent. Never move/remove a foreign App to make room.
                & $ClosedCheck
                Assert-PromotionAbsent $app;Assert-PromotionTree $old $backup
                Write-PromotionEvent $receipt 'rollback-intent' @{from=$backup;to=$app;id=$old.root.Id;manifestSha256=$old.digest}
                [ReviewedMainFs]::Rename($old.root,$app);$rollback=$true
                Assert-PromotionTree $old $app
                Write-PromotionEvent $receipt 'rolled-back' @{oldRestored=$true;stageRetained=$stage}
            } catch {try{Write-PromotionEvent $receipt 'rollback-held' @{message=$_.Exception.Message}}catch{}}
        }
        throw [IO.IOException]::new(('Promotion held. Old App restored: '+$rollback+'. Receipt/stage retained for review. '+$failure.Exception.Message),$failure.Exception)
    } finally {
        if($null -ne $receipt){$receipt.Dispose()}
        Close-PromotionTree $staged;Close-PromotionTree $old
        foreach($pin in $pins){$pin.Dispose()}
    }
}
