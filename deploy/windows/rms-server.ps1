# Everything for the Alphahedge RMS on the Windows VPS, started by "Alphahedge RMS Server.bat":
#   1. installs or updates the RMS and keeps it running with Windows (restarts on a crash)
#   2. on the very first run creates its settings (.env) and database, and shows the admin password once
#   3. adds the RMS's website to the Caddy that already serves MarketHub, WITHOUT changing MarketHub's site
#   4. schedules a daily backup, and shows the status
#
# What it never does: touch MarketHub's files, task, port or website; open new firewall ports (80 and 443
# are already open for MarketHub); install anything (no Python, no Chrome, no second Caddy).
#
# Switches:  -NoReload   write the website file but do not reload Caddy now (do it later, out of hours)

param([switch]$NoReload)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root
[Console]::OutputEncoding = [Text.Encoding]::UTF8

function Step($t) { Write-Host "`n==> $t" -ForegroundColor Cyan }
function Ok($t) { Write-Host "    OK   $t" -ForegroundColor Green }
function Warn($t) { Write-Host "    WARN $t" -ForegroundColor Yellow }
function Fail($t) { Write-Host "    FAIL $t" -ForegroundColor Red; exit 1 }

$rms = Join-Path $root 'rms'
$newExe = Join-Path $rms 'rms-server.exe'
$binDir = Join-Path $rms 'bin'
$runExe = Join-Path $binDir 'rms-server.exe'
$port = 8090
$api = "http://127.0.0.1:$port"
$log = Join-Path $root 'rms.log'

foreach ($f in 'rms\rms-server.exe', 'rms\web\index.html', 'run-rms.cmd', 'backup-rms.ps1') {
    if (-not (Test-Path (Join-Path $root $f))) { Fail "$f is missing next to this script: copy the whole package folder" }
}

# ---------- 1. install / update / keep running ----------
$task = Get-ScheduledTask -TaskName 'Alphahedge RMS' -ErrorAction SilentlyContinue
$needCopy = -not (Test-Path $runExe) -or ((Get-FileHash $newExe).Hash -ne (Get-FileHash $runExe).Hash)

# The port must be free, or be ours already.
$taken = Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue
if ($taken -and -not $task) {
    $who = (Get-Process -Id $taken[0].OwningProcess -ErrorAction SilentlyContinue).ProcessName
    Fail "port $port is already used by '$who'. The RMS needs it (it listens on 127.0.0.1 only). Nothing was changed."
}

# The marker is written only once the database is ready, so a setup that stopped half way is redone, not skipped.
$marker = Join-Path $rms '.setup-ok'
if ($needCopy -or -not $task -or -not (Test-Path $marker)) {
    Step $(if ($task) { 'Updating the RMS' } else { 'Installing the RMS' })
    if ($task) { Stop-ScheduledTask -TaskName 'Alphahedge RMS' -ErrorAction SilentlyContinue }
    Get-Process 'rms-server' -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    New-Item -ItemType Directory -Force $binDir | Out-Null
    Copy-Item $newExe $runExe -Force

    # ---------- 1b. Microsoft's C++ runtime ----------
    # The RMS's database engine (DuckDB) is C++. It needs MSVCP140.dll, and a RECENT one: programs built with
    # Visual Studio 2022 (17.10 and later) fail to start their DLLs ("A dynamic link library (DLL)
    # initialization routine failed") on a runtime older than 14.40. A fresh or old VPS often has none or an
    # old one (MarketHub does not need it). So it is installed or upgraded here, from Microsoft.
    $cppRuntime = Join-Path $env:windir 'System32\msvcp140.dll'
    function Get-RuntimeVersion {
        if (-not (Test-Path $cppRuntime)) { return $null }
        $v = (Get-Item $cppRuntime).VersionInfo
        [version]('{0}.{1}.{2}' -f $v.FileMajorPart, $v.FileMinorPart, $v.FileBuildPart)
    }
    $have = Get-RuntimeVersion
    if (-not $have -or $have -lt [version]'14.40.0') {
        Step ('Installing Microsoft''s Visual C++ runtime (now: {0}; the RMS needs 14.40 or newer)' -f $(if ($have) { $have } else { 'none' }))
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $vc = Join-Path $env:TEMP 'vc_redist.x64.exe'
        Invoke-WebRequest 'https://aka.ms/vs/17/release/vc_redist.x64.exe' -OutFile $vc -UseBasicParsing
        $p = Start-Process $vc -ArgumentList '/install', '/quiet', '/norestart' -Wait -PassThru
        $have = Get-RuntimeVersion
        # 0 = installed; 3010 = installed but Windows must restart to swap a file that is in use
        if (-not $have -or $have -lt [version]'14.40.0') {
            if ($p.ExitCode -eq 3010) { Fail 'The Visual C++ runtime was installed but Windows has to restart to finish it. Restart this VPS, then run this again.' }
            Fail "the Visual C++ runtime did not install (exit code $($p.ExitCode)). Install the newest vc_redist.x64.exe from microsoft.com by hand (restart afterwards), then run this again."
        }
        Ok "Visual C++ runtime $have installed"
    } else {
        Ok "Visual C++ runtime $have"
    }

    # ---------- 2. first run: settings, database, admin password ----------
    # The program is run with its output captured; a native program's messages must not be read as PowerShell errors.
    function Run-Rms($exe, $arg) {
        $prev = $ErrorActionPreference
        $ErrorActionPreference = 'Continue'
        Push-Location $rms
        $env:RMS_NO_EXTENSIONS = '1'     # the RMS needs no DuckDB add-ons and must never try to download or load one
        try { $text = (& $exe $arg 2>&1 | Out-String); $code = $LASTEXITCODE } finally { Pop-Location; $ErrorActionPreference = $prev }
        [pscustomobject]@{ Text = $text; Code = $code }
    }
    function Explain-Failure($r) {
        $hex = if ($null -ne $r.Code) { '0x{0:X}' -f ([int]$r.Code) } else { 'none' }
        "the program ended with code $($r.Code) ($hex); the Visual C++ runtime here is $(Get-RuntimeVersion). Code 0xC0000135 or 0xC000007B means a Windows runtime file is missing; 'DLL initialization routine failed' means the runtime is too old: restart this VPS and run this again."
    }
    $env0 = Join-Path $rms '.env'
    $firstRun = -not (Test-Path $env0)
    if ($firstRun) { Step 'First run: creating the settings and the database' }
    $r = Run-Rms $runExe 'migrate'      # makes .env on the first run, and brings the database schema up to date
    Write-Host $r.Text
    if ($r.Code -ne 0 -or -not (Test-Path $env0)) {
        # Do not install the service on a broken setup. The password (if one was just made) is in rms\.env and stays.
        Fail "the RMS could not set up its database. $(Explain-Failure $r)"
    }
    if ($firstRun) {
        Write-Host '    The admin password is shown above, ONCE. Write it down now. It is also in rms\.env' -ForegroundColor Yellow
        Write-Host '    (RMS_ADMIN_PASSWORD=...), and you can change it from the Admin block in the RMS.' -ForegroundColor Yellow
    }

    Set-Content $marker (Get-Date -Format s)

    $action = New-ScheduledTaskAction -Execute (Join-Path $root 'run-rms.cmd') -WorkingDirectory $root
    $trigger = New-ScheduledTaskTrigger -AtStartup
    $settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) `
        -StartWhenAvailable -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    Register-ScheduledTask -TaskName 'Alphahedge RMS' -Action $action -Trigger $trigger -Settings $settings -Principal $principal -Force | Out-Null
    Ok 'the RMS starts with Windows and restarts if it ever crashes'
}
if ((Get-ScheduledTask -TaskName 'Alphahedge RMS').State -ne 'Running') { Start-ScheduledTask -TaskName 'Alphahedge RMS' }

$up = $false
foreach ($i in 1..40) {
    try { Invoke-RestMethod "$api/health" -TimeoutSec 2 | Out-Null; $up = $true; break } catch { Start-Sleep -Milliseconds 500 }
}
if (-not $up) { Fail "the RMS did not start; see $log" }
Ok "the RMS is running ($api, this computer only)"

# ---------- 3. the website, through the Caddy that MarketHub already uses ----------
# MarketHub's Caddyfile ends with: import ".../caddy/sites/*.caddy". Every file in that folder is another
# website. This script adds one, rms.caddy, and leaves MarketHub's own Caddyfile alone. Before Caddy is told
# to load it, the whole configuration is checked: if the check fails, rms.caddy is removed again and Caddy
# is not touched.
$domainFile = Join-Path $root 'domain.txt'
$domain = $null
if (Test-Path $domainFile) {
    $domain = Get-Content $domainFile | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') } | Select-Object -First 1
    # Only the name is wanted: "https://rms.duckdns.org/" pasted from a browser becomes "rms.duckdns.org".
    if ($domain) { $domain = ($domain -replace '^[a-zA-Z]+://', '' -replace '[/\s].*$', '').ToLower() }
}
if (-not $domain) {
    Warn 'No website yet: put the RMS address (a second DuckDNS name pointing at this VPS, e.g. rms-alphahedge.duckdns.org) in domain.txt, then run this again.'
} else {
    # Find MarketHub's Caddy: the program its "MarketHub Web" task runs.
    $caddyExe = $null
    $mh = Get-ScheduledTask -TaskName 'MarketHub Web' -ErrorAction SilentlyContinue
    if ($mh) { $caddyExe = ($mh.Actions | Select-Object -First 1).Execute }
    if (-not $caddyExe -or -not (Test-Path $caddyExe)) {
        $guess = Get-ChildItem (Split-Path -Parent $root) -Recurse -Filter caddy.exe -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($guess) { $caddyExe = $guess.FullName }
    }
    if (-not $caddyExe -or -not (Test-Path $caddyExe)) {
        Fail 'MarketHub''s Caddy was not found. Run MarketHub Server.bat once first (it installs Caddy and its website), then run this again.'
    }
    $caddyDir = Split-Path -Parent $caddyExe
    $caddyfile = Join-Path $caddyDir 'Caddyfile'
    if (-not (Test-Path $caddyfile)) { Fail "Caddy's Caddyfile was not found in $caddyDir" }
    if ((Get-Content $caddyfile -Raw) -notmatch 'sites/\*\.caddy') {
        Fail "Caddy's Caddyfile does not import its sites folder, so the RMS website cannot be added safely. Run the newest MarketHub Server.bat first. Nothing was changed."
    }
    $sites = Join-Path $caddyDir 'sites'
    New-Item -ItemType Directory -Force $sites | Out-Null
    $site = Join-Path $sites 'rms.caddy'

    # Optional: only these addresses may open the RMS website (allow.txt, one IP or range per line).
    $allowFile = Join-Path $root 'allow.txt'
    $allow = @()
    if (Test-Path $allowFile) { $allow = @(Get-Content $allowFile | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') }) }
    $guard = ''
    if ($allow.Count -gt 0) { $guard = "`t@blocked not remote_ip $($allow -join ' ')`n`trespond @blocked 403`n" }

    $conf = @"
# The Alphahedge RMS website. Written by Alphahedge RMS Server.bat: edit domain.txt / allow.txt, not this file.
$domain {
	encode zstd gzip
$guard	header {
		Strict-Transport-Security "max-age=31536000"
		X-Content-Type-Options nosniff
		X-Frame-Options DENY
		Referrer-Policy no-referrer
	}
	reverse_proxy 127.0.0.1:$port {
		header_up X-Forwarded-For {remote_host}
		header_up X-Forwarded-Proto {scheme}
	}
}
"@
    $old = if (Test-Path $site) { Get-Content $site -Raw } else { $null }
    if ($old -and $old.Trim() -eq $conf.Trim()) {
        Ok "the RMS website ($domain) is already set up"
    } else {
        [IO.File]::WriteAllText($site, $conf)
        Push-Location $caddyDir
        $check = & $caddyExe validate --config $caddyfile --adapter caddyfile 2>&1 | Out-String
        $valid = $LASTEXITCODE -eq 0
        Pop-Location
        if (-not $valid) {
            if ($old) { [IO.File]::WriteAllText($site, $old) } else { Remove-Item $site -Force }
            Write-Host $check
            Fail 'Caddy rejected the new configuration, so it was taken back out. Caddy and MarketHub were not touched.'
        }
        Ok "rms.caddy written and checked ($domain)"
        if ($NoReload) {
            Warn 'Not reloaded (-NoReload). The RMS website starts working the next time Caddy reloads or restarts.'
        } else {
            # A reload is graceful (no restart, no downtime for MarketHub's pages), but an open live connection
            # can be closed once and its page reconnects by itself. Do it out of market hours if that matters.
            Push-Location $caddyDir
            & $caddyExe reload --config $caddyfile --adapter caddyfile 2>&1 | Out-Null
            $reloaded = $LASTEXITCODE -eq 0
            Pop-Location
            if ($reloaded) { Ok 'Caddy reloaded; MarketHub''s website was not changed' }
            else { Warn 'Caddy did not reload (is it running?). Start MarketHub''s "MarketHub Web" task; it will pick up the RMS website.' }
        }
    }
    # Caddy must be running for any website (MarketHub's or the RMS's) to answer. If it is not, start it the
    # way MarketHub's own installer does: through its scheduled task.
    if (-not (Get-Process caddy -ErrorAction SilentlyContinue)) {
        Warn 'Caddy is not running, so no website answers (MarketHub''s either). Starting it now.'
        Start-ScheduledTask -TaskName 'MarketHub Web' -ErrorAction SilentlyContinue
        foreach ($i in 1..20) { if (Get-Process caddy -ErrorAction SilentlyContinue) { break }; Start-Sleep -Milliseconds 500 }
        if (Get-Process caddy -ErrorAction SilentlyContinue) { Ok 'Caddy is running' }
        else { Warn 'Caddy did not start. Run check-rms.ps1, or start it by hand to see why: in the Caddy folder run  caddy.exe run --config Caddyfile --adapter caddyfile' }
    }
    Write-Host "    The website:  https://$domain   (the certificate is issued by Caddy on the first visit; the DuckDNS name must already point here)"
    if ($allow.Count -gt 0) { Ok "only these addresses may open it: $($allow -join ', ')" }
    else { Warn 'Anyone on the internet can reach the sign-in page. To limit it to your own IP, put it in allow.txt (one per line) and run this again.' }
}

# ---------- 4. daily backup ----------
if (-not (Get-ScheduledTask -TaskName 'Alphahedge RMS Backup' -ErrorAction SilentlyContinue)) {
    $action = New-ScheduledTaskAction -Execute 'powershell.exe' -Argument "-NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $root 'backup-rms.ps1')`"" -WorkingDirectory $root
    $trigger = New-ScheduledTaskTrigger -Daily -At '03:30'
    $principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
    Register-ScheduledTask -TaskName 'Alphahedge RMS Backup' -Action $action -Trigger $trigger -Principal $principal -Force | Out-Null
    Ok 'daily backup at 03:30 into backups\ (kept 14 days). Copy backups\ off this computer now and then too.'
}

# ---------- status ----------
Step 'Status'
$h = Invoke-RestMethod "$api/health"
Write-Host ("    database schema: {0}   orphan rows: {1}" -f $h.schema, @($h.orphans).Count)
Write-Host "    log: $log"
