#!/usr/bin/env bash
# Prepares an upstream worktree for the reference tracer. Idempotent.
# Usage: Reference/Setup.sh <commit> [repo-root]   (repo-root defaults to the simulator checkout root)
# node_modules (about 400 MB) is symlinked from the pool Cache/NodeModules/<package-lock hash>/ (filled by `npm ci`
# once per lock file), or from the worktree named by OFSIM_SHARE_NODE_MODULES=<commit12> when the pool lacks it.
# Remove a worktree with `git -C Cache/OpenFrontIO worktree remove --force Cache/Worktrees/<commit12>`; the pool stays.
set -euo pipefail
commit="${1:?usage: Setup.sh <commit> [repo-root]}"
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "${2:-$here/..}" && pwd)"
# A worktree under Cache/Wt/<name> shares the main checkout's Cache/.
if [ -z "${2:-}" ] && [ "$(basename "$(dirname "$root")")" = "Wt" ]; then root="$(cd "$root/../../.." && pwd)"; fi
clone="$root/Cache/OpenFrontIO"
[ -d "$clone/.git" ] || git clone https://github.com/openfrontio/OpenFrontIO.git "$clone"
full="$(git -C "$clone" rev-parse "$commit^{commit}" 2>/dev/null || { git -C "$clone" fetch --all --tags --quiet; git -C "$clone" rev-parse "$commit^{commit}"; })"
tree="$root/Cache/Worktrees/${full:0:12}"
[ -d "$tree" ] || git -C "$clone" worktree add --detach "$tree" "$full"
# Drop a link left dangling by a removed donor worktree.
[ -L "$tree/node_modules" ] && [ ! -e "$tree/node_modules" ] && rm "$tree/node_modules"
if [ ! -e "$tree/node_modules" ]; then
  # Shared pool keyed by package-lock.json, so removing any worktree never breaks another one.
  lockhash="$(sha256sum "$tree/package-lock.json" | cut -c1-16)"
  pool="$root/Cache/NodeModules/$lockhash/node_modules"
  if [ -n "${OFSIM_SHARE_NODE_MODULES:-}" ] && [ ! -d "$pool" ]; then
    pool="$root/Cache/Worktrees/$OFSIM_SHARE_NODE_MODULES/node_modules"
  fi
  if [ ! -d "$pool" ]; then
    (cd "$tree" && npm ci --ignore-scripts --engine-strict=false)
    mkdir -p "$(dirname "$pool")"
    mv "$tree/node_modules" "$pool"
  fi
  if [ -d "$tree/packages" ]; then
    # Workspace build: share the packages but link @openfront/* to this worktree's own sources.
    mkdir "$tree/node_modules"
    for entry in "$pool"/* "$pool"/.bin; do
      [ "$(basename "$entry")" = "@openfront" ] || ln -s "$entry" "$tree/node_modules/$(basename "$entry")"
    done
    mkdir "$tree/node_modules/@openfront"
    for pkg in "$tree"/packages/*/; do ln -s "../../packages/$(basename "$pkg")" "$tree/node_modules/@openfront/$(basename "$pkg")"; done
  else
    ln -s "$pool" "$tree/node_modules"
  fi
fi
mkdir -p "$tree/ofsim-reference"
cp "$here"/*.ts "$here"/*.mjs "$tree/ofsim-reference/"
node "$tree/ofsim-reference/MakeCoreShim.mjs" "$tree" "$here"
echo "ready: $tree"
