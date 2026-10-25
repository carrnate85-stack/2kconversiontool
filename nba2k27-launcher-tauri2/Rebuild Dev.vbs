Option Explicit
Dim shell, files, folder, powershell, command, result
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
folder = files.GetParentFolderName(WScript.ScriptFullName)
powershell = shell.ExpandEnvironmentStrings("%SystemRoot%") & "\System32\WindowsPowerShell\v1.0\powershell.exe"
command = """" & powershell & """ -NoLogo -NoProfile -ExecutionPolicy Bypass -File """ & folder & "\Dev.ps1"" -NoRun"
If WScript.Arguments.Named.Exists("check") Then
  WScript.Echo command
Else
  result = shell.Run(command, 0, True)
  If result = 0 Then
    MsgBox "Dev build updated. Open it with Start Dev.", vbInformation, "2K Launcher Dev"
  Else
    MsgBox "The dev rebuild failed. Diagnostic log:" & vbCrLf & folder & "\Dev\start-error.log", vbCritical, "2K Launcher Dev"
  End If
End If
