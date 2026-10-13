@echo off
setlocal DisableDelayedExpansion
powershell.exe -NoLogo -NoProfile -NoExit -File "%~dp0Test-Streaming.ps1" -Role Viewer
if errorlevel 1 pause
