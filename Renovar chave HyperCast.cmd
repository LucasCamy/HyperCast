@echo off
setlocal DisableDelayedExpansion
title Renovar chave do HyperCast
powershell.exe -NoLogo -NoProfile -File "%~dp0tools\renew-access-windows.ps1"
set "renewal_exit=%ERRORLEVEL%"
if not "%renewal_exit%"=="0" echo A renovacao terminou com erro. Veja a mensagem acima.
echo(
pause
exit /b %renewal_exit%
