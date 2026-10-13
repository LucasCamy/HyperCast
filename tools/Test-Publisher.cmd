@echo off
setlocal DisableDelayedExpansion
powershell.exe -NoLogo -NoProfile -NoExit -File "%~dp0Test-Streaming.ps1" -Role Publisher
if errorlevel 1 pause
