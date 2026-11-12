# Prepares an upstream worktree for the reference tracer. Idempotent.
# Usage: Reference\Setup.ps1 <commit> [repo-root]   (repo-root defaults to the simulator checkout root)
param(
  [Parameter(Mandatory = $true)][string]$Commit,
  [string]$Root = (Join-Path $PSScriptRoot "..")
)
$ErrorActionPreference = "Stop"
$Root = (Resolve-Path $Root).Path
$clone = Join-Path $Root "Cache/OpenFrontIO"
if (-not (Test-Path (Join-Path $clone ".git"))) { git clone https://github.com/openfrontio/OpenFrontIO.git $clone }
$full = git -C $clone rev-parse "$Commit^{commit}" 2>$null
if ($LASTEXITCODE -ne 0) { git -C $clone fetch --all --tags --quiet; $full = git -C $clone rev-parse "$Commit^{commit}" }
$tree = Join-Path $Root "Cache/Worktrees/$($full.Substring(0, 12))"
if (-not (Test-Path $tree)) { git -C $clone worktree add --detach $tree $full }
if (-not (Test-Path (Join-Path $tree "node_modules"))) {
  Push-Location $tree; npm ci --ignore-scripts --engine-strict=false; Pop-Location
}
$target = Join-Path $tree "ofsim-reference"
New-Item -ItemType Directory -Force $target | Out-Null
Copy-Item (Join-Path $PSScriptRoot "*.ts") $target -Force
Write-Output "ready: $tree"
