@echo off
rem Started by the "Alphahedge RMS" scheduled task. Runs the installed copy in rms\bin\ (so a new
rem rms\rms-server.exe can be pasted in while it runs). Everything it keeps (.env, data\) is in rms\.
rem The log is rms.log; a new one starts when it passes ~10 MB.
cd /d "%~dp0rms"
set "RMS_WEB_DIR=%~dp0rms\web"
set "RMS_BIND=127.0.0.1:8090"
rem The RMS needs no DuckDB add-ons; this makes sure it never tries to download or load one.
set "RMS_NO_EXTENSIONS=1"
set "LOG=%~dp0rms.log"
if exist "%LOG%" for %%F in ("%LOG%") do if %%~zF GTR 10000000 move /y "%LOG%" "%LOG%.old" >nul
"%~dp0rms\bin\rms-server.exe" >> "%LOG%" 2>&1
