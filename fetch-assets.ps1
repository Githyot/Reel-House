# Downloads what gets built into the exe: libmpv, the rqbit torrent engine and the Anime4K shaders.
# Needs: PowerShell 5+ (unpacks .7z with Windows tar, 7-Zip or WinRAR) and the GitHub CLI (gh) logged in or with GH_TOKEN set.
$ErrorActionPreference = "Stop"
$assets = Join-Path $PSScriptRoot "assets"
$tmp = Join-Path $env:TEMP "reelhouse-assets"
New-Item -ItemType Directory -Force $assets, "$assets\shaders", $tmp | Out-Null

Write-Host "libmpv..."
# The newest upstream release is sometimes published before its files are uploaded; fall back to older ones.
$got = $false
$tags = @("") + @(gh release list -R shinchiro/mpv-winbuild-cmake -L 6 --json tagName -q ".[].tagName")
foreach ($tag in $tags) {
  $args_ = @("release", "download") + $(if ($tag) { @($tag) } else { @() }) + @("-R", "shinchiro/mpv-winbuild-cmake", "-p", "mpv-dev-x86_64-2*.7z", "-D", $tmp, "--clobber")
  & gh @args_ 2>$null
  if ($LASTEXITCODE -eq 0 -and (Get-ChildItem $tmp -Filter "mpv-dev-x86_64-2*.7z")) { $got = $true; break }
  Write-Host "no libmpv package in release '$tag', trying an older one"
}
if (-not $got) { throw "Could not download libmpv from shinchiro/mpv-winbuild-cmake." }
$mpv7z = Get-ChildItem $tmp -Filter "mpv-dev-x86_64-2*.7z" | Sort-Object LastWriteTime | Select-Object -Last 1
$mx = Join-Path $tmp "mpvx"
Remove-Item $mx -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $mx | Out-Null
# Unpack the .7z with whatever is available: Windows' own tar, 7-Zip or WinRAR.
$done = $false
try { tar -xf $mpv7z.FullName -C $mx 2>$null; if ($LASTEXITCODE -eq 0) { $done = $true } } catch {}
if (-not $done) {
  $tools = @("7z", "$env:ProgramFiles\7-Zip\7z.exe", "$env:ProgramFiles\WinRAR\WinRAR.exe", "${env:ProgramFiles(x86)}\WinRAR\WinRAR.exe", "$env:ProgramFiles\WinRAR\UnRAR.exe")
  foreach ($t in $tools) {
    $cmd = Get-Command $t -ErrorAction SilentlyContinue
    if (-not $cmd) { continue }
    if ($t -like "*7z*") { & $cmd.Source x $mpv7z.FullName "-o$mx" -y | Out-Null } else { & $cmd.Source x -y $mpv7z.FullName "$mx\" | Out-Null }
    if (Get-ChildItem $mx -Recurse -Filter "libmpv-2.dll" -ErrorAction SilentlyContinue) { $done = $true; break }
  }
}
$dll = Get-ChildItem $mx -Recurse -Filter "libmpv-2.dll" -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $dll) { throw "Could not unpack $($mpv7z.Name). Extract it by hand and copy libmpv-2.dll into the assets folder." }
Copy-Item $dll.FullName "$assets\libmpv-2.dll" -Force

Write-Host "rqbit..."
try {
  $names = gh release view -R ikatson/rqbit --json assets -q ".assets[].name" | Where-Object { $_ -match "(?i)^rqbit(-win.*)?\.exe$|win.*x86_64|win.*x64|win.*amd64" -and $_ -notmatch "(?i)desktop|\.msi$|arm|sha|sig|\.txt$" }
  Write-Host ("rqbit candidates: " + ($names -join ", "))
  $pick = $names | Where-Object { $_ -match "(?i)x86_64|x64|amd64" } | Select-Object -First 1
  if (-not $pick) { $pick = $names | Select-Object -First 1 }
  if (-not $pick) { throw "no Windows build in the latest release" }
  gh release download -R ikatson/rqbit -p $pick -D $tmp --clobber
  $f = Join-Path $tmp $pick
  if ($pick -match "\.zip$") {
    $rx = Join-Path $tmp "rqx"; Remove-Item $rx -Recurse -Force -ErrorAction SilentlyContinue
    Expand-Archive $f $rx -Force
    $f = (Get-ChildItem $rx -Recurse -Filter "*.exe" | Select-Object -First 1).FullName
  }
  Copy-Item $f "$assets\rqbit.exe" -Force
  Write-Host "rqbit ok"
} catch { Write-Warning "rqbit not downloaded ($_). The exe will use Real-Debrid only." }

Write-Host "Anime4K shaders..."
gh release download -R bloc97/Anime4K -p "*.zip" -D $tmp --clobber
$z = Get-ChildItem $tmp -Filter "Anime4K*.zip" | Select-Object -First 1
if (!$z) { $z = Get-ChildItem $tmp -Filter "*.zip" | Select-Object -First 1 }
$x = Join-Path $tmp "a4k"
Remove-Item $x -Recurse -Force -ErrorAction SilentlyContinue
Expand-Archive $z.FullName $x -Force
Get-ChildItem $x -Recurse -Filter "Anime4K_*.glsl" | ForEach-Object { Copy-Item $_.FullName "$assets\shaders\" -Force }
Write-Host ("Shaders: " + (Get-ChildItem "$assets\shaders" -Filter *.glsl).Count)
