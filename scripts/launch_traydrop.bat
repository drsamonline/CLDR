@echo off
REM ------------------------------------------------------------------
REM  TrayDrop launcher (Windows)
REM  * First run       -> starts the app hidden in the notification tray.
REM  * Already running -> brings the existing command window forward.
REM  Double-click this file, or point a .lnk shortcut at it
REM  (see scripts\make_shortcut.ps1 for an automatic version).
REM ------------------------------------------------------------------
setlocal
cd /d "%~dp0.."

where pyw >nul 2>nul
if %errorlevel%==0 (
    start "" /min pyw -3 -m traydrop
) else (
    start "" /min python -m traydrop
)
endlocal
