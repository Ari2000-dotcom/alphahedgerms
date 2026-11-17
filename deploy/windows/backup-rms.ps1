# Daily backup of the RMS, started by the "Alphahedge RMS Backup" scheduled task at 03:30 (the market is
# closed). It stops the RMS for a few seconds so the database file can be copied cleanly, copies
# rms\.env and rms\data into backups\<date>\, starts the RMS again and keeps the last 14 days.
# .env holds the key that opens the saved broker logins: a database copy without it cannot be read,
# so the two are always copied together. Copy backups\ off this computer now and then as well.

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$rms = Join-Path $root 'rms'
$dest = Join-Path $root ('backups\' + (Get-Date -Format 'yyyy-MM-dd'))
$log = Join-Path $root 'rms-backup.log'
function Note($t) { Add-Content $log ("{0}  {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $t) }

try {
    Stop-ScheduledTask -TaskName 'Alphahedge RMS' -ErrorAction SilentlyContinue
    Get-Process 'rms-server' -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
    New-Item -ItemType Directory -Force $dest | Out-Null
    foreach ($item in '.env', 'data') {
        $src = Join-Path $rms $item
        if (Test-Path $src) { Copy-Item $src $dest -Recurse -Force }
    }
    Note "backup written to $dest"
} catch {
    Note "backup FAILED: $($_.Exception.Message)"
} finally {
    Start-ScheduledTask -TaskName 'Alphahedge RMS'
}

# Keep 14 days.
$keep = (Get-Date).AddDays(-14)
Get-ChildItem (Join-Path $root 'backups') -Directory -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -lt $keep } | Remove-Item -Recurse -Force
