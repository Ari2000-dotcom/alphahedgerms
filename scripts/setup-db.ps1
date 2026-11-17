# Creates and starts the RMS's own Postgres cluster, separate from any other Postgres on this machine.
#
#   * Data lives in .\pgdata (ignored by git). Port 5433. Only this computer can connect.
#   * A random password is written to .\.env (ignored by git) and used for the "rms" user.
#   * Safe to run again: an existing cluster is started, not recreated.
#
# Usage:  powershell -ExecutionPolicy Bypass -File scripts\setup-db.ps1

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$pgbin = "D:\PostgreSQL\17\bin"
$data = Join-Path $root "pgdata"
$envfile = Join-Path $root ".env"
$port = 5433

if (-not (Test-Path "$pgbin\initdb.exe")) { throw "Postgres programs not found in $pgbin. Edit `$pgbin at the top of this script." }

if (-not (Test-Path (Join-Path $data "PG_VERSION"))) {
    $chars = (48..57) + (65..90) + (97..122)
    $password = -join ($chars | Get-Random -Count 32 | ForEach-Object { [char]$_ })
    $pwfile = Join-Path $env:TEMP "rms-pw.txt"
    Set-Content -Path $pwfile -Value $password -NoNewline -Encoding ascii
    & "$pgbin\initdb.exe" -D $data -U rms --auth=scram-sha-256 --pwfile=$pwfile -E UTF8 --locale=C | Out-Null
    Remove-Item $pwfile -Force
    # Listen on this computer only.
    Add-Content -Path (Join-Path $data "postgresql.conf") -Value "listen_addresses = '127.0.0.1'`nport = $port`nmax_connections = 40"
    Set-Content -Path $envfile -Encoding ascii -Value @"
DATABASE_URL=postgres://rms:$password@127.0.0.1:$port/rms
RMS_BIND=127.0.0.1:8090
"@
    Write-Host "Created a new cluster in $data and wrote $envfile"
    $fresh = $true
}

$status = & "$pgbin\pg_ctl.exe" -D $data status 2>&1
if ($status -notmatch "server is running") {
    & "$pgbin\pg_ctl.exe" -D $data -l (Join-Path $root "pglog.txt") -w start | Out-Null
    Write-Host "Postgres started on port $port"
} else {
    Write-Host "Postgres is already running"
}

# Create the database on first run.
$url = (Get-Content $envfile | Where-Object { $_ -like "DATABASE_URL=*" }) -replace "^DATABASE_URL=", ""
if ($url -match "^postgres://rms:([^@]+)@") { $env:PGPASSWORD = $Matches[1] }
$exists = & "$pgbin\psql.exe" -h 127.0.0.1 -p $port -U rms -d postgres -tAc "SELECT 1 FROM pg_database WHERE datname='rms'"
if (-not $exists) {
    & "$pgbin\psql.exe" -h 127.0.0.1 -p $port -U rms -d postgres -c "CREATE DATABASE rms" | Out-Null
    Write-Host "Created database rms"
}
Write-Host "Ready. Next: cargo run -p rms-server -- migrate"
