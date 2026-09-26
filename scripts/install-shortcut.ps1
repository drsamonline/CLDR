# # =============================================================================
# # CLDR — Command Line Dispatch & Route
# # © 2026 Dr. Sohil Momin (drsamonline). All rights reserved.
# # MIT-licensed: THE ABOVE COPYRIGHT / ATTRIBUTION NOTICE MUST BE RETAINED IN
# # ALL COPIES OR SUBSTANTIAL PORTIONS OF THIS SOFTWARE. Removing this
# # watermark is a violation of the license terms.
# # =============================================================================
#
<#
.SYNOPSIS
  CLDR launch-shortcut installer (Windows).  © 2026 Dr. Sohil Momin (drsamonline)
.DESCRIPTION
  Creates, for the CURRENT USER only (no admin needed):
    1. Start Menu shortcut  : "CLDR.lnk"        -> cldr.exe            (opens command window)
    2. Desktop shortcut     : "CLDR Summon.lnk" -> cldr.exe --summon   (brings window forward)
    3. Startup (tray) entry : "CLDR Tray.lnk"   -> cldr.exe --tray     (hidden, runs in background)
    4. summon.vbs           : hidden-window launcher used by hotkey tools
.NOTES
  Watermark: keep the header above intact in any copy (MIT attribution clause).
#>

$ErrorActionPreference = 'Stop'
$root    = Split-Path -Parent $PSScriptRoot
$exe     = Join-Path $root 'target\release\cldr.exe'
if (-not (Test-Path $exe)) { $exe = Join-Path $root 'target\debug\cldr.exe' }
if (-not (Test-Path $exe)) { throw "cldr.exe not found — run 'cargo build --release' first." }

$shell    = New-Object -ComObject WScript.Shell
$startup  = [Environment]::GetFolderPath('Startup')
$programs = [Environment]::GetFolderPath('Programs')
$desktop  = [Environment]::GetFolderPath('Desktop')

function New-Lnk($path, $target, $args, $desc) {
    $lnk = $shell.CreateShortcut($path)
    $lnk.TargetPath       = $target
    $lnk.Arguments        = $args
    $lnk.WorkingDirectory = Split-Path $target
    $lnk.Description      = "$desc — by Dr. Sohil Momin"
    if ($args -eq '--tray') { $lnk.WindowStyle = 7 } else { $lnk.WindowStyle = 1 }
    $lnk.Save()
    Write-Host "[OK] $path"
}

New-Lnk (Join-Path $programs 'CLDR.lnk')                $exe ''          'CLDR command window'
New-Lnk (Join-Path $desktop  'CLDR Summon.lnk')         $exe '--summon'  'Summon CLDR command window'
New-Lnk (Join-Path $startup  'CLDR Tray.lnk')           $exe '--tray'    'CLDR background tray resident'

$vbs = Join-Path $env:LOCALAPPDATA 'cldr\summon.vbs'
New-Item -ItemType Directory -Force -Path (Split-Path $vbs) | Out-Null
@"
' CLDR hidden summon launcher — © 2026 Dr. Sohil Momin (drsamonline)
CreateObject("Wscript.Shell").Run """$exe --summon""", 0, False
"@ | Set-Content -Encoding ASCII $vbs
Write-Host "[OK] $vbs  (bind Win+Alt+C to this via AutoHotKey/PowerToys — see docs/SHORTCUTS.md)"
Write-Host ''
Write-Host 'Done. Sign out/in once for the Startup tray entry to take effect,' -ForegroundColor Green
Write-Host 'or start it now:  & "' + (Join-Path $startup 'CLDR Tray.lnk') + '"'
