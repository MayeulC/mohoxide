# Builds the Windows download: dist/rts-core-engine-<version>-windows-x64.zip,
# holding rts-core-engine.exe (the C runtime linked in, so no VC++ redistributable
# is needed), README.txt, LICENSE.txt and THIRD-PARTY-LICENSES.txt. See docs/guides/packaging.md.
#
#   powershell -ExecutionPolicy Bypass -File dev\package\package-windows.ps1 [-Profile dist|release]
param([string]$Profile = "dist")
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
Set-Location $repo

$version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$commit = (git rev-parse --short HEAD).Trim()
$name = "rts-core-engine-$version-$commit-windows-x64"

# A target folder of its own: the static C runtime changes every crate's
# build, which would otherwise throw away the everyday build's cache.
$env:CARGO_TARGET_DIR = "$repo\target\package"
$env:RUSTFLAGS = "-C target-feature=+crt-static"
cargo build --profile $Profile --bin rts-core-engine
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$dir = if ($Profile -eq "dev") { "debug" } else { $Profile }
$exe = "$env:CARGO_TARGET_DIR\$dir\rts-core-engine.exe"

$out = "$repo\dist\$name"
Remove-Item -Recurse -Force $out -ErrorAction SilentlyContinue
New-Item -ItemType Directory $out | Out-Null
Copy-Item $exe "$out\rts-core-engine.exe"
Copy-Item "$repo\dev\package\README.txt" "$out\README.txt"
Copy-Item "$repo\LICENSE" "$out\LICENSE.txt"
Copy-Item "$repo\dev\package\THIRD-PARTY-LICENSES.txt" "$out\THIRD-PARTY-LICENSES.txt"
# tar.exe (Windows 10+) writes a standard zip; PowerShell 5's Compress-Archive
# stores `\` in paths, which unzips as odd file names on macOS and Linux.
Remove-Item "$out.zip" -ErrorAction SilentlyContinue
tar.exe -a -c -f "$out.zip" -C "$repo\dist" $name
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
Write-Output "$out.zip"
