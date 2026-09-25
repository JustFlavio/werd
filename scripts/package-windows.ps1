$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
if (-not (Test-Path -LiteralPath $cargo)) { throw 'Rust/Cargo not found' }
$env:PATH = "$(Join-Path $env:USERPROFILE '.cargo\bin');$env:PATH"
$windowsKits = 'C:\Program Files (x86)\Windows Kits\10\bin'
$resourceCompiler = Get-ChildItem -LiteralPath $windowsKits -Filter rc.exe -Recurse -ErrorAction Stop |
    Where-Object { $_.DirectoryName.EndsWith('\x64') } |
    Sort-Object FullName | Select-Object -Last 1
if (-not $resourceCompiler) { throw 'Windows SDK rc.exe (x64) not found' }
$env:RC = $resourceCompiler.FullName
$env:PATH = "$($resourceCompiler.DirectoryName);$env:PATH"
& $cargo build --release -p werd-core -p werd-cli
if ($LASTEXITCODE -ne 0) { throw 'Building the daemon/CLI failed' }
$triple = (& (Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe') --print host-tuple).Trim()
if ($triple -ne 'x86_64-pc-windows-msvc') { throw "Unsupported target: $triple" }
$sidecars = Join-Path $PSScriptRoot '..\src-tauri\binaries'
New-Item -ItemType Directory -Force -Path $sidecars | Out-Null
Copy-Item -LiteralPath 'target\release\werd-daemon.exe' -Destination (Join-Path $sidecars "werd-daemon-$triple.exe") -Force
Copy-Item -LiteralPath 'target\release\werd.exe' -Destination (Join-Path $sidecars "werd-$triple.exe") -Force
& '.\node_modules\.bin\tauri.cmd' build --config src-tauri/tauri.nsis.conf.json
if ($LASTEXITCODE -ne 0) { throw 'Building the NSIS installer failed' }
Write-Output 'Installer NSIS: target/release/bundle/nsis'
