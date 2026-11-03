# Windows version of dev/show_scenarios.sh: plays the scenario tests
# (tests/scenarios/*.lua) one after another in a window, for recording, the
# camera following their units. Each runs until it calls done() (plus a short
# hold), then EndGame() closes the window and the next one starts.
#
#   powershell -ExecutionPolicy Bypass -File dev\show_scenarios.ps1                  # all, in tests/scenarios.rs order
#   powershell -ExecutionPolicy Bypass -File dev\show_scenarios.ps1 transport fuel   # just these
#   ... show_scenarios.ps1 -Speed 2 -Zoom 60 -Keep
# The Lua appended to each is dev/show_scenarios.lua.
param(
    [Parameter(ValueFromRemainingArguments)] [string[]]$Names = @(),
    [int]$Speed = 1,
    [int]$Zoom = 80,
    [int]$Hold = 30,     # ticks to keep showing after done()
    [switch]$Keep
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

cargo build --release
if ($LASTEXITCODE -ne 0) { exit 1 }
$exe = Join-Path $root 'target\release\rts-core-engine.exe'
# ReadAllText: Windows PowerShell's Get-Content reads UTF-8 without a BOM as ANSI.
$show = [IO.File]::ReadAllText((Join-Path $root 'dev\show_scenarios.lua'))
$tests = [IO.File]::ReadAllText((Join-Path $root 'tests\scenarios.rs'))

$list = @()
if ($Names.Count -gt 0) {
    foreach ($n in $Names) {
        $m = [regex]::Match($tests, "scenario\(`"$([regex]::Escape($n))`", (\d+)\)")
        $list += , @($n, $(if ($m.Success) { [int]$m.Groups[1].Value } else { 3000 }))
    }
} else {
    foreach ($m in [regex]::Matches($tests, 'scenario\("([a-z_0-9]*)", (\d+)\)')) {
        $list += , @($m.Groups[1].Value, [int]$m.Groups[2].Value)
    }
}

$work = Join-Path ([IO.Path]::GetTempPath()) ("show_scenarios_" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force $work | Out-Null
if ($Keep) { "keeping $work" }

try {
    foreach ($entry in $list) {
        $name, $max = $entry
        $src = Join-Path $root "tests\scenarios\$name.lua"
        if (-not (Test-Path $src)) { "skip ${name}: no $src"; continue }
        $text = [IO.File]::ReadAllText($src)
        # Camera on the first unit the scenario places with literal coordinates
        # (CreateUnitHPR(bp, army, x, y, z, ...)), else the scenario land.
        $cx, $cz = 420, 600
        $m = [regex]::Match($text, "CreateUnitHPR\('[a-z0-9]+', *'ARMY_[0-9]', *([0-9.]+), *[^,]+, *([0-9.]+)")
        if ($m.Success) { $cx, $cz = $m.Groups[1].Value, $m.Groups[2].Value }
        $wrapped = Join-Path $work "$name.lua"
        $header = "`nSHOW = { name = `"$name`", zoom = $Zoom, hold = $Hold }`n"
        [IO.File]::WriteAllText($wrapped, $text + $header + $show)
        "== $name (camera $cx,$cz, at most $max ticks)"
        & $exe --session --no-ai --no-record --no-audio --speed $Speed `
            --ticks ($max + $Hold + 10) --camera "$cx,$cz,$Zoom" `
            --script $wrapped --log (Join-Path $work 'game.log')
        if ($LASTEXITCODE -ne 0) { "   exit $LASTEXITCODE" }
    }
} finally {
    if (-not $Keep) { Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue }
}
