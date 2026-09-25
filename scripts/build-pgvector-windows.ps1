param(
    [Parameter(Mandatory = $true)][string]$PgRoot,
    [string]$SourceDir = (Join-Path $PSScriptRoot '..\.cache\pgvector-v0.8.6')
)

$ErrorActionPreference = 'Stop'
$PgRoot = [System.IO.Path]::GetFullPath($PgRoot)
$SourceDir = [System.IO.Path]::GetFullPath($SourceDir)
if (-not (Test-Path -LiteralPath (Join-Path $PgRoot 'bin\postgres.exe'))) {
    throw "PostgreSQL 18 not found in $PgRoot"
}
if (-not (Test-Path -LiteralPath $SourceDir)) {
    git clone --depth 1 --branch v0.8.6 https://github.com/pgvector/pgvector.git $SourceDir
    if ($LASTEXITCODE -ne 0) { throw 'Downloading the pgvector source failed' }
}
$commit = (git -C $SourceDir rev-parse HEAD).Trim()
if ($commit -ne '8ee86c96f0fd72390f890aa8a336fda6d3ab4c6c') {
    throw "Commit pgvector inatteso: $commit"
}
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio Build Tools con MSVC richiesti' }
$vsRoot = (& $vswhere -latest -products '*' -property installationPath).Trim()
$devCmd = Join-Path $vsRoot 'Common7\Tools\VsDevCmd.bat'
if (-not (Test-Path -LiteralPath $devCmd)) { throw "VsDevCmd not found in $vsRoot" }
Push-Location $SourceDir
try {
    cmd.exe /c "call `"$devCmd`" -arch=x64 && set PGROOT=$PgRoot&& nmake /F Makefile.win && nmake /F Makefile.win install"
    if ($LASTEXITCODE -ne 0) { throw 'Building pgvector failed' }
} finally { Pop-Location }
if (-not (Test-Path -LiteralPath (Join-Path $PgRoot 'lib\vector.dll'))) { throw 'vector.dll was not installed' }
if (-not (Test-Path -LiteralPath (Join-Path $PgRoot 'share\extension\vector.control'))) { throw 'vector.control was not installed' }
Write-Output "pgvector v0.8.6 installato in $PgRoot"
