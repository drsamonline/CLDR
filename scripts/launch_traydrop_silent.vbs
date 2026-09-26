' ------------------------------------------------------------------
'  TrayDrop silent launcher (Windows, VBScript)
'  Starts the app with NO console window flashing on screen.
'  Re-running it while the app is up simply raises the existing
'  command window (handled inside traydrop.app via the mutex).
'  Point your desktop / Start-menu shortcut at this file:
'      wscript.exe "...\scripts\launch_traydrop_silent.vbs"
' ------------------------------------------------------------------
Option Explicit
Dim sh, fso, root
Set sh  = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
root = fso.GetParentFolderName(fso.GetParentFolderName(WScript.ScriptFullName))
sh.CurrentDirectory = root
' Prefer pythonw (no console), fall back to python minimised.
If sh.Run("where pyw", 0, True) = 0 Then
    sh.Exec "pyw -3 -m traydrop"
Else
    sh.Run "python -m traydrop", 7, False   ' 7 = min-no-focus
End If
Set sh = Nothing
Set fso = Nothing
