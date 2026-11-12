#!/usr/bin/env bash
# Prepares an upstream worktree for the reference tracer. Idempotent.
# Usage: Reference/Setup.sh <commit> [repo-root]   (repo-root defaults to the simulator checkout root)
set -euo pipefail
commit="${1:?usage: Setup.sh <commit> [repo-root]}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${2:-$here/..}" && pwd)"
clone="$root/Cache/OpenFrontIO"
[ -d "$clone/.git" ] || git clone https://github.com/openfrontio/OpenFrontIO.git "$clone"
full="$(git -C "$clone" rev-parse "$commit^{commit}" 2>/dev/null || { git -C "$clone" fetch --all --tags --quiet; git -C "$clone" rev-parse "$commit^{commit}"; })"
tree="$root/Cache/Worktrees/${full:0:12}"
[ -d "$tree" ] || git -C "$clone" worktree add --detach "$tree" "$full"
[ -d "$tree/node_modules" ] || (cd "$tree" && npm ci --ignore-scripts --engine-strict=false)
mkdir -p "$tree/ofsim-reference"
cp "$here"/*.ts "$tree/ofsim-reference/"
echo "ready: $tree"
