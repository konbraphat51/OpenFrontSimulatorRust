#!/usr/bin/env bash
# Prepares an upstream worktree for the reference tracer. Idempotent.
# Usage: Reference/Setup.sh <commit> [repo-root]   (repo-root defaults to the simulator checkout root)
# node_modules (about 400 MB) is symlinked from another worktree with an identical package-lock.json, or from
# the worktree named by OFSIM_SHARE_NODE_MODULES=<commit12> (the tracer only needs the engine's few dependencies);
# otherwise `npm ci` runs.
set -euo pipefail
commit="${1:?usage: Setup.sh <commit> [repo-root]}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${2:-$here/..}" && pwd)"
clone="$root/Cache/OpenFrontIO"
[ -d "$clone/.git" ] || git clone https://github.com/openfrontio/OpenFrontIO.git "$clone"
full="$(git -C "$clone" rev-parse "$commit^{commit}" 2>/dev/null || { git -C "$clone" fetch --all --tags --quiet; git -C "$clone" rev-parse "$commit^{commit}"; })"
tree="$root/Cache/Worktrees/${full:0:12}"
[ -d "$tree" ] || git -C "$clone" worktree add --detach "$tree" "$full"
if [ ! -e "$tree/node_modules" ]; then
  donor=""
  if [ -n "${OFSIM_SHARE_NODE_MODULES:-}" ]; then
    donor="$root/Cache/Worktrees/$OFSIM_SHARE_NODE_MODULES"
  else
    for other in "$root"/Cache/Worktrees/*/; do
      other="${other%/}"
      if [ "$other" != "$tree" ] && [ -d "$other/node_modules" ] && [ ! -L "$other/node_modules" ] && cmp -s "$other/package-lock.json" "$tree/package-lock.json"; then
        donor="$other"
        break
      fi
    done
  fi
  if [ -n "$donor" ] && [ -d "$donor/node_modules" ]; then
    ln -s "$(cd "$donor" && pwd)/node_modules" "$tree/node_modules"
  else
    (cd "$tree" && npm ci --ignore-scripts --engine-strict=false)
  fi
fi
mkdir -p "$tree/ofsim-reference"
cp "$here"/*.ts "$tree/ofsim-reference/"
echo "ready: $tree"
