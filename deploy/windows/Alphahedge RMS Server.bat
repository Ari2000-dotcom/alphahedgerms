@echo off
rem The only file to run on the Windows VPS for the Alphahedge RMS. First time: installs it. Every time:
rem starts or updates the RMS and shows its status. Closing this window does not stop the RMS.
rem It never touches MarketHub: its own folder, its own scheduled task, its own port, and one extra
rem file in MarketHub's Caddy "sites" folder.
net session >nul 2>&1 || (powershell -NoProfile -Command "Start-Process -FilePath '%~f0' -Verb RunAs" & exit /b)
title Alphahedge RMS Server
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0rms-server.ps1" %*
pause
