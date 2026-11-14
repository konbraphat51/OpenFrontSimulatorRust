#!/usr/bin/env bash
# Differential trace: runs the upstream engine (TraceDumper) and ofsim on a record and diffs the traces.
# Usage: Tools/TraceCompare.sh <record> <until> [--inert]   (worktree and maps from OFSIM_WORKTREE,
# default Cache/Worktrees/4e837520e886 of the main checkout; --inert compares with all not-ported
# executions made inert on both sides, see Reference/InertPatch.ts)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
record="$(realpath "${1:?record}")"
until="${2:?until tick}"
inert="${3:-}"
tree="${OFSIM_WORKTREE:-/home/user/OpenFrontDs/Simulator/Cache/Worktrees/4e837520e886}"
refdir="${OFSIM_REFDIR:-ofsim-reference}"
bin="${OFSIM_BIN:-$here/target/debug/ofsim}"
out="${OFSIM_OUT:-/tmp/ofsim-compare}"
mkdir -p "$out"
name="$(basename "$record" .json.gz)"
flag=()
if [ "$inert" = "--inert" ]; then flag=(--inert 1); export OFSIM_INERT=1; fi
(cd "$tree" && npx tsx "$refdir/TraceDumper.ts" "$record" "$out/$name.ref.ndjson" --until "$until" "${flag[@]}" 2>&1 | tail -n 2) || true
"$bin" trace --upstream "${OFSIM_UPSTREAM:-/home/user/OpenFrontDs/Simulator/Cache/OpenFrontIO}" --record "$record" --maps "$tree/resources/maps" --out "$out/$name.rs.ndjson" --until "$until" 2>&1 | tail -n 2
(cd "$tree" && npx tsx "$refdir/TraceDiff.ts" "$out/$name.ref.ndjson" "$out/$name.rs.ndjson")
