# Cargo decides "up to date" by file modification times. Files extracted from a zip can get timestamps
# that are OLDER than the previous build (zip stores local time without a time zone), so cargo skips
# the rebuild and you end up with a stale exe.
# This script hashes the sources; if their CONTENT changed since the last run, it refreshes their
# timestamps to "now" so cargo is guaranteed to rebuild.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

$files = @(Get-ChildItem -Path "$root\src", "$root\assets" -Recurse -File -ErrorAction SilentlyContinue)
$files += @(Get-Item "$root\build.rs", "$root\Cargo.toml", "$root\Cargo.lock" -ErrorAction SilentlyContinue)
$files = $files | Sort-Object FullName

$sb = New-Object System.Text.StringBuilder
foreach ($f in $files) {
    $h = (Get-FileHash -Algorithm SHA256 -Path $f.FullName).Hash
    [void]$sb.Append($f.FullName.Substring($root.Length)).Append(':').Append($h).Append("`n")
}
$sha = New-Object System.Security.Cryptography.SHA256Managed
$sig = [System.BitConverter]::ToString($sha.ComputeHash([System.Text.Encoding]::UTF8.GetBytes($sb.ToString()))).Replace('-', '')

New-Item -ItemType Directory -Force -Path "$root\target" | Out-Null
$marker = "$root\target\.retypr-sources.sig"
$old = ''
if (Test-Path $marker) { $old = (Get-Content $marker -Raw).Trim() }

if ($old -ne $sig) {
    $now = Get-Date
    foreach ($f in $files) { $f.LastWriteTime = $now }
    Set-Content -Path $marker -Value $sig
    Write-Host "[INFO] Sources changed since the last build - timestamps refreshed, cargo will rebuild."
} else {
    Write-Host "[INFO] Sources unchanged since the last build."
}
