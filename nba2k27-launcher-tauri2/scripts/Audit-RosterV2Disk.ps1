param([string]$GameExe='C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe',[string]$OutputPath)
$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
if(-not $OutputPath){$OutputPath=Join-Path $project 'artifacts/game-v2-roster-named-fields-discovery-20261016.json'}
if((Get-FileHash -LiteralPath $GameExe -Algorithm SHA256).Hash -ne '29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1'){throw 'Exact Version 2 disk image required'}
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
public sealed class RosterNamedReference {
 public string Key; public uint StringRva, InstructionRva;
}
public static class RosterNamedFieldDiscovery {
 public static List<RosterNamedReference> Find(byte[] code,uint codeRva,byte[] data,uint dataRva,string[] names){
  var strings=new Dictionary<uint,string>();
  var wanted=new HashSet<string>(names,StringComparer.Ordinal);
  var counts=new Dictionary<string,int>(StringComparer.Ordinal);
  for(int i=0;i<data.Length;i++){
   if((i!=0 && data[i-1]!=0)||data[i]<32||data[i]>126)continue;
   int end=i;while(end<data.Length && end-i<=100 && data[end]>=32 && data[end]<=126)end++;
   if(end>=data.Length||data[end]!=0||end-i>100)continue;
   string name=System.Text.Encoding.ASCII.GetString(data,i,end-i);
   if(wanted.Contains(name)){
    int count;counts.TryGetValue(name,out count);if(count>=16)throw new InvalidOperationException("Too many duplicate named fields");
    counts[name]=count+1;strings.Add(dataRva+(uint)i,name);
   }
   i=end;
  }
  var output=new List<RosterNamedReference>();
  for(int i=0;i<=code.Length-7;i++){
   if(code[i]!=0x48||code[i+1]!=0x8d||code[i+2]!=0x15)continue;
   long target=(long)codeRva+i+7+BitConverter.ToInt32(code,i+3);string key;
   if(target<0||target>uint.MaxValue||!strings.TryGetValue((uint)target,out key))continue;
   output.Add(new RosterNamedReference{Key=key,StringRva=(uint)target,InstructionRva=codeRva+(uint)i});
   if(output.Count>8192)throw new InvalidOperationException("Too many named serializer references");
  }
  return output;
 }
}
'@
$taskStream=[IO.File]::OpenRead($GameExe)
try{
 $taskHeader=New-Object byte[] 4096
 if($taskStream.Read($taskHeader,0,4096) -ne 4096){throw 'Truncated PE header'}
 $taskPe=[BitConverter]::ToUInt32($taskHeader,60)
 if($taskPe -lt 64 -or $taskPe -gt 3800 -or [BitConverter]::ToUInt32($taskHeader,$taskPe+8) -ne 0x6AC53765 -or [BitConverter]::ToUInt32($taskHeader,$taskPe+80) -ne 0x35BDE000){throw 'Unexpected Version 2 header'}
 $taskCount=[BitConverter]::ToUInt16($taskHeader,$taskPe+6)
 $taskOptional=[BitConverter]::ToUInt16($taskHeader,$taskPe+20)
 if($taskCount -lt 1 -or $taskCount -gt 32){throw 'Invalid PE section count'}
 $taskSections=@()
 for($taskI=0;$taskI -lt $taskCount;$taskI++){
  $taskAt=$taskPe+24+$taskOptional+$taskI*40
  $taskName=[Text.Encoding]::ASCII.GetString($taskHeader,$taskAt,8).Trim([char]0)
  $taskSections+=,[pscustomobject]@{name=$taskName;rva=[BitConverter]::ToUInt32($taskHeader,$taskAt+12);rawBytes=[BitConverter]::ToUInt32($taskHeader,$taskAt+16);rawOffset=[BitConverter]::ToUInt32($taskHeader,$taskAt+20);flags=[BitConverter]::ToUInt32($taskHeader,$taskAt+36)}
 }
 function Read-RosterAuditSection($Section){
  if($Section.rawBytes -gt 128MB -or $Section.rawOffset+$Section.rawBytes -gt $taskStream.Length){throw 'Unbounded roster audit section'}
  $taskBytes=New-Object byte[] $Section.rawBytes
  [void]$taskStream.Seek($Section.rawOffset,[IO.SeekOrigin]::Begin)
  if($taskStream.Read($taskBytes,0,$taskBytes.Length) -ne $taskBytes.Length){throw 'Truncated roster audit section'}
  return ,$taskBytes
 }
 $taskCodeSection=$taskSections|Where-Object {$_.name -eq '.text' -and $_.flags -eq 0x60000020}|Select-Object -First 1
 $taskDataSection=$taskSections|Where-Object {$_.name -eq '.rdata'}|Select-Object -First 1
 $taskCode=Read-RosterAuditSection $taskCodeSection
 $taskData=Read-RosterAuditSection $taskDataSection
 $taskKeys=(Get-Content -LiteralPath (Join-Path $project 'assets/roster-layout.json') -Raw|ConvertFrom-Json).fields.key
 $taskReferences=[RosterNamedFieldDiscovery]::Find($taskCode,$taskCodeSection.rva,$taskData,$taskDataSection.rva,$taskKeys)
 $taskOutput=[pscustomobject]@{schema='nba2k27-v2-roster-named-field-disk-discovery-v1';imageSha256='29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1';gameProcessAccess=$false;gameFilesChanged=$false;candidateOnly=$true;requestedKeys=$taskKeys.Count;matchedKeys=@($taskReferences.Key|Select-Object -Unique).Count;rows=@($taskReferences|ForEach-Object {[pscustomobject]@{key=$_.Key;instructionRva=('{0:X8}' -f $_.InstructionRva);stringRva=('{0:X8}' -f $_.StringRva) }})}
 [IO.File]::WriteAllText($OutputPath,($taskOutput|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
 [pscustomobject]@{pass=$true;references=$taskReferences.Count;matchedKeys=$taskOutput.matchedKeys;requestedKeys=$taskKeys.Count;output=$OutputPath;gameProcessAccess=$false;candidateOnly=$true}|ConvertTo-Json -Compress
}finally{$taskStream.Dispose()}
