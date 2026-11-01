# Builds the Qt version of Reelhouse (the Rust core + Qt Quick). Needs MSYS2 and the GitHub CLI (gh auth login) (pacman installs the rest).
# Run it, then start qt\build\reelhouse-qt.exe. If it fails, send build-qt.log.
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
./fetch-assets.ps1
# Qt's qmlimportscanner cannot read a path with accents (like "Area de Trabalho" with the accent): the build then fails at CMake
# with "No such file or directory". In that case the folder is mapped to a free drive letter for the build.
$drive = $null
if ($PSScriptRoot -match '[^\x00-\x7F]') {
    $used = (Get-PSDrive -PSProvider FileSystem).Name
    $letter = "RQPONMLKJIHGFEDZYXWVUTS".ToCharArray() | Where-Object { $used -notcontains "$_" } | Select-Object -First 1
    if (!$letter) { throw "The folder has accents in its path and no drive letter is free. Move it to C:\ReelHouse and run again." }
    $drive = "${letter}:"
    subst $drive "$PSScriptRoot"
    if ($LASTEXITCODE -ne 0) { throw "Could not map the folder to $drive. Move it to a path without accents (C:\ReelHouse) and run again." }
    Write-Host "The folder has accents in its path, so the build runs from $drive\ (the same folder)."
    Set-Location "$drive\"
}
try {
$msys = "C:\msys64"
if (!(Test-Path "$msys\usr\bin\bash.exe")) { throw "MSYS2 was not found at $msys." }
$env:MSYSTEM = "MINGW64"
$env:CHERE_INVOKING = "1"
& "$msys\usr\bin\bash.exe" -lc "bash msys-build-qt.sh"
if ($LASTEXITCODE -ne 0) {
    Write-Host "The build failed. Send me build-qt.log (a copy is in qt\build\debug)" -ForegroundColor Red
    if (Test-Path "build-qt.log") { Get-Content "build-qt.log" -Tail 40 }
    throw "The build failed."
}
Write-Host "Done. Start: qt\build\reelhouse-qt.exe   (every log is in qt\build\debug)"
} finally {
    if ($drive) { Set-Location $PSScriptRoot; subst $drive /d | Out-Null }
}
