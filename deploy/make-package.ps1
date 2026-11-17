# Builds the copy-to-VPS folder ..\Alphahedge-RMS-VPS (and a .zip of it) from this project.
# Run on the PC, after:
#     cargo r                      (cargo build --workspace --release: makes target\release\rms-server.exe)
#     cd web ; npm run build       (makes web\dist)
# No secrets go in it: no .env (the admin password, the vault key) and no data\ (the database), so
# the folder and zip are safe to upload. The VPS makes its own .env on the first run; a new package
# unpacked over the VPS folder leaves .env, data\, backups\ and the logs where they are.

$ErrorActionPreference = 'Stop'
$proj = Split-Path -Parent $PSScriptRoot
$out = Join-Path (Split-Path -Parent $proj) 'Alphahedge-RMS-VPS'

$exe = Join-Path $proj 'target\release\rms-server.exe'
$dist = Join-Path $proj 'web\dist'
if (-not (Test-Path $exe)) { throw "target\release\rms-server.exe is missing: run 'cargo r' in $proj first" }
if (-not (Test-Path "$dist\index.html")) { throw "web\dist is missing: run 'npm run build' in $proj\web first" }
if ((Get-Item $exe).LastWriteTime -lt (Get-ChildItem "$proj\crates" -Recurse -Include *.rs, *.sql | Sort-Object LastWriteTime -Descending | Select-Object -First 1).LastWriteTime) {
    throw "target\release\rms-server.exe is older than the source: run 'cargo r' again"
}

# The settings from the last package, kept: unpacked over the VPS folder, a blank domain.txt would
# leave the website on its old address.
$domain = $null
$allow = @()
if (Test-Path "$out\domain.txt") {
    $domain = Get-Content "$out\domain.txt" | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') } | Select-Object -First 1
}
if (Test-Path "$out\allow.txt") {
    $allow = @(Get-Content "$out\allow.txt" | ForEach-Object { $_.Trim() } | Where-Object { $_ -and -not $_.StartsWith('#') })
}
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Force "$out\rms" | Out-Null

Copy-Item "$PSScriptRoot\windows\*" $out
Copy-Item "$PSScriptRoot\README-VPS.txt" "$out\README.txt"
Copy-Item $exe "$out\rms\rms-server.exe"
Copy-Item $dist "$out\rms\web" -Recurse


Set-Content -Path "$out\domain.txt" -Encoding ASCII -Value (@(
    '# The RMS website address, on the line below without the #. A SECOND DuckDNS name pointing at this',
    '# VPS (not MarketHub''s own name), e.g.:',
    '# rms-alphahedge.duckdns.org',
    '# Then run "Alphahedge RMS Server.bat" again.'
) + @($domain | Where-Object { $_ }))
Set-Content -Path "$out\allow.txt" -Encoding ASCII -Value (@(
    '# Optional. Only these addresses may open the RMS website: one IP or range per line, e.g.',
    '# 203.0.113.7',
    '# 198.51.100.0/24',
    '# Leave it empty and the sign-in page is open to the internet (the password and lockout still apply).'
) + $allow)

# Batch files want CRLF.
foreach ($b in Get-ChildItem $out -File | Where-Object { $_.Extension -in '.bat', '.cmd', '.ps1', '.txt' }) {
    [IO.File]::WriteAllText($b.FullName, ([IO.File]::ReadAllText($b.FullName) -replace "`r?`n", "`r`n"))
}

# Last check before zipping: nothing secret in what may be uploaded or shared.
$secrets = '.env', 'rms.duckdb', 'rms.duckdb.wal', 'reset-code.txt'
$found = Get-ChildItem $out -Recurse -File -Force | Where-Object { $_.Name -in $secrets }
if ($found) { throw "secrets in the package, not zipping: $($found.FullName -join ', ')" }

$zip = "$out.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Compress-Archive -Path "$out\*" -DestinationPath $zip
Write-Host "Package ready: $out"
Write-Host "Zip:           $zip"
