# ------------------------------------------------------------------
#  Creates a "TrayDrop.lnk" desktop shortcut that:
#    * launches the app silently into the notification tray, and
#    * re-opens (foregrounds) the command window when clicked again.
#  Usage (PowerShell):  powershell -ExecutionPolicy Bypass -File make_shortcut.ps1
# ------------------------------------------------------------------
$ErrorActionPreference = "Stop"

$root      = Split-Path -Parent $PSScriptRoot
$vbs       = Join-Path $PSScriptRoot "launch_traydrop_silent.vbs"
$desktop   = [Environment]::GetFolderPath("Desktop")
$lnkPath   = Join-Path $desktop "TrayDrop.lnk"

$shell     = New-Object -ComObject WScript.Shell
$sc        = $shell.CreateShortcut($lnkPath)
$sc.TargetPath       = "wscript.exe"
$sc.Arguments        = "`"$vbs`""
$sc.WorkingDirectory = $root
$sc.Description      = "TrayDrop - background console app with tray icon (click again to open the window)"
$sc.IconLocation     = "$env:SystemRoot\System32\shell32.dll,172"
$sc.WindowStyle      = 7   # minimized
$sc.Save()

Write-Host "Shortcut created: $lnkPath" -ForegroundColor Green

# Optional: also pin an autostart entry (uncomment to enable).
# $startUp = [Environment]::GetFolderPath("Startup")
# Copy-Item $lnkPath (Join-Path $startUp "TrayDrop.lnk") -Force
# Write-Host "Added to Windows Startup folder." -ForegroundColor Green
