Option Explicit
Dim shell, files, folder, powershell, command, exitCode, errorLog
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
folder = files.GetParentFolderName(WScript.ScriptFullName)
powershell = shell.ExpandEnvironmentStrings("%SystemRoot%") & "\System32\WindowsPowerShell\v1.0\powershell.exe"
command = """" & powershell & """ -NoLogo -NoProfile -ExecutionPolicy Bypass -File """ & folder & "\Start-Dev.ps1"""
If WScript.Arguments.Named.Exists("check") Then
  WScript.Echo command
Else
  exitCode = shell.Run(command, 0, True)
  If exitCode <> 0 Then
    errorLog = folder & "\Dev\start-error.log"
    MsgBox "The installed dev launcher could not be started." & vbCrLf & vbCrLf & _
      "Diagnostic log:" & vbCrLf & errorLog, vbCritical, "2K Launcher Dev"
  End If
End If
