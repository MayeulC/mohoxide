# Fresh dist packaging time and runtime parity for two commits, on Windows.
# Each commit gets its own worktree and empty target dir, is packaged with
# dev\package\package-windows.ps1, then the two binaries run the 3000-tick
# fight in alternating rounds. Results: target\build-speed\results.txt.
#
#   powershell -ExecutionPolicy Bypass -File dev\build-speed\measure-windows.ps1 [-Base 2ad97cb] [-Head HEAD] [-Rounds 5]
param([string]$Base = "2ad97cb", [string]$Head = "HEAD", [int]$Rounds = 5)
$ErrorActionPreference = "Stop"
$repo = (Resolve-Path "$PSScriptRoot\..\..").Path
$work = "$repo\target\build-speed"
New-Item -ItemType Directory -Force $work | Out-Null
$results = "$work\results.txt"

$shas = @()
foreach ($ref in @($Base, $Head)) {
    $sha = (git -C $repo rev-parse --short $ref).Trim()
    $shas += $sha
    $tree = "$work\tree-$sha"
    try { git -C $repo worktree remove --force $tree 2>&1 | Out-Null } catch {}
    if (Test-Path $tree) { Remove-Item -Recurse -Force $tree }
    git -C $repo worktree prune
    git -C $repo worktree add --detach $tree $sha | Out-Null
    # A child PowerShell: package-windows.ps1 sets RUSTFLAGS for its session.
    $t = Measure-Command {
        powershell -NoProfile -ExecutionPolicy Bypass -File "$tree\dev\package\package-windows.ps1" | Out-Host
    }
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    Copy-Item "$tree\target\package\dist\rts-core-engine.exe" "$work\rts-$sha.exe"
    "build ${sha}: $([math]::Round($t.TotalSeconds, 2)) s" | Tee-Object -Append $results
}

Set-Location $repo
for ($i = 1; $i -le $Rounds; $i++) {
    foreach ($sha in $shas) {
        $log = "$work\run-$sha-$i.log"
        $t = Measure-Command {
            & "$work\rts-$sha.exe" --headless --ticks 3000 --script dev/scenes/fight.lua --log $log | Out-Null
        }
        $sums = (Select-String -Path $log -Pattern 'progress' | ForEach-Object {
            if ($_.Line -match 'checksum=([0-9a-f]+)') { "checksum=$($Matches[1])" } }) -join ' '
        "run $i ${sha}: $([math]::Round($t.TotalSeconds, 2)) s $sums" | Tee-Object -Append $results
    }
}
