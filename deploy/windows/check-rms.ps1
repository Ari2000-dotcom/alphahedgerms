# Looks at everything between the internet and the RMS and says what is wrong. Changes nothing.
# Run it on the VPS in an Administrator PowerShell:   powershell -ExecutionPolicy Bypass -File .\check-rms.ps1
# Then copy everything it prints.

$ErrorActionPreference = 'Continue'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
function Line($t) { Write-Host "`n--- $t" -ForegroundColor Cyan }
function Good($t) { Write-Host "  OK    $t" -ForegroundColor Green }
function Bad($t) { Write-Host "  PROBLEM  $t" -ForegroundColor Red }
function Info($t) { Write-Host "        $t" }

Line 'This computer'
Info ("Windows: " + (Get-CimInstance Win32_OperatingSystem).Caption)
$pub = try { (Invoke-RestMethod 'https://api.ipify.org' -TimeoutSec 8) } catch { $null }
Info ("Public IP as the internet sees it: " + $(if ($pub) { $pub } else { 'could not be read' }))

Line 'The RMS program'
$t = Get-ScheduledTask -TaskName 'Alphahedge RMS' -ErrorAction SilentlyContinue
if ($t) { Info "task 'Alphahedge RMS': $($t.State)" } else { Bad "task 'Alphahedge RMS' does not exist" }
try { $h = Invoke-RestMethod 'http://127.0.0.1:8090/health' -TimeoutSec 4; Good "answers on 127.0.0.1:8090 (schema $($h.schema))" } catch { Bad "does not answer on 127.0.0.1:8090. Last lines of rms.log:"; Get-Content (Join-Path $root 'rms.log') -Tail 8 -ErrorAction SilentlyContinue | ForEach-Object { Info $_ } }

Line 'Caddy (the web server in front of it)'
$mh = Get-ScheduledTask -TaskName 'MarketHub Web' -ErrorAction SilentlyContinue
if ($mh) { Info "task 'MarketHub Web': $($mh.State)" } else { Bad "task 'MarketHub Web' does not exist" }
$caddyExe = if ($mh) { ($mh.Actions | Select-Object -First 1).Execute } else { $null }
$procs = Get-Process caddy -ErrorAction SilentlyContinue
if ($procs) { Good "caddy.exe is running (process $($procs.Id -join ', '))" } else { Bad 'caddy.exe is NOT running: nothing is listening for the websites' }
foreach ($p in 80, 443) {
    $l = Get-NetTCPConnection -LocalPort $p -State Listen -ErrorAction SilentlyContinue
    if ($l) { Good "something listens on port $p ($((Get-Process -Id $l[0].OwningProcess -ErrorAction SilentlyContinue).ProcessName))" } else { Bad "nothing listens on port $p" }
}
if ($caddyExe -and (Test-Path $caddyExe)) {
    $dir = Split-Path -Parent $caddyExe
    $cf = Join-Path $dir 'Caddyfile'
    Info "Caddy folder: $dir"
    Line 'rms.caddy (the RMS website file)'
    $site = Join-Path $dir 'sites\rms.caddy'
    if (Test-Path $site) { Get-Content $site | ForEach-Object { Info $_ } } else { Bad 'sites\rms.caddy is missing' }
    Line 'Does Caddy accept its configuration?'
    Push-Location $dir
    $v = & $caddyExe validate --config $cf --adapter caddyfile 2>&1 | Out-String
    Pop-Location
    if ($LASTEXITCODE -eq 0) { Good 'the configuration is valid' } else { Bad 'Caddy rejects the configuration:'; $v.Trim().Split("`n") | Select-Object -First 12 | ForEach-Object { Info $_ } }
    $logs = Get-ChildItem $dir -Filter *.log -Recurse -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($logs) { Info "newest Caddy log: $($logs.FullName)"; Get-Content $logs.FullName -Tail 8 | ForEach-Object { Info $_ } }
}

Line 'Windows Firewall'
foreach ($p in 80, 443) {
    $rules = Get-NetFirewallPortFilter -Protocol TCP | Where-Object { @($_.LocalPort) -contains "$p" } | Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object { $_.Direction -eq 'Inbound' -and $_.Enabled -eq 'True' -and $_.Action -eq 'Allow' }
    if ($rules) { Good "an inbound rule allows port $p ($(($rules | Select-Object -First 1).DisplayName))" } else { Bad "no enabled inbound rule allows port $p" }
}
Info 'The VPS provider has its own firewall too: ports 80 and 443 must be open there as well.'

Line 'The website name'
$domain = $null
$df = Join-Path $root 'domain.txt'
if (Test-Path $df) { $domain = Get-Content $df | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') } | Select-Object -First 1 }
if ($domain) {
    $domain = ($domain -replace '^[a-zA-Z]+://', '' -replace '[/\s].*$', '')
    $ips = try { (Resolve-DnsName $domain -Type A -ErrorAction Stop | Where-Object { $_.IPAddress }).IPAddress } catch { @() }
    Info "$domain points to: $($ips -join ', ')"
    if ($pub -and $ips -contains $pub) { Good 'it points at this VPS' } elseif ($pub) { Bad "it does NOT point at this VPS ($pub). Fix the record at duckdns.org." }
    Info 'Asking this computer for the site, as a browser would:'
    $r = & curl.exe -s -o NUL -w '%{http_code}' --max-time 8 --resolve "${domain}:443:127.0.0.1" "https://$domain/" 2>&1
    Info "https://$domain/ from this computer: HTTP $r  (200 = the RMS answers; 000 = nothing answered)"
} else { Info 'domain.txt has no address yet' }

Write-Host "`nDone. Copy everything above." -ForegroundColor Cyan
