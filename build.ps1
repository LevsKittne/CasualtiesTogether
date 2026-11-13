param(
    [string]$GameDir = "D:\Program Files (x86)\Steam\steamapps\common\Casualties Unknown",
    [switch]$Install
)

$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
$dist = Join-Path $root "dist"
$ct = Join-Path $dist "ModLoader\mods\CasualtiesTogether"
$setup = Join-Path $dist "setup"

if (-not (Test-Path (Join-Path $GameDir "ModLoader\core\ModLoader.dll"))) { throw "ModLoader is not installed in $GameDir" }
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Force $ct, $setup | Out-Null

cargo build --release --manifest-path (Join-Path $root "Cargo.toml") -p ct
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
Copy-Item (Join-Path $root "target\release\ct.dll") $ct

dotnet build (Join-Path $root "mods\CasualtiesTogether\CasualtiesTogether.csproj") -c Release "-p:GameDir=$GameDir" -o (Join-Path $root "target\ct")
if ($LASTEXITCODE -ne 0) { throw "CasualtiesTogether build failed" }
Copy-Item (Join-Path $root "target\ct\CasualtiesTogether.dll") $ct

cargo build --release --manifest-path (Join-Path $root "Cargo.toml") -p installer
if ($LASTEXITCODE -ne 0) { throw "installer build failed" }
Copy-Item (Join-Path $root "target\release\CasualtiesTogether-Setup.exe") $setup
Copy-Item (Join-Path $root "installer\*.txt") $setup

if ($Install) {
    Copy-Item (Join-Path $dist "ModLoader") $GameDir -Recurse -Force
    Write-Host "installed to $GameDir"
}
