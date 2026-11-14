#!/usr/bin/env bash
# Differential nuke/SAM/doomsday scenario: runs Reference/NukeScenarioDump.ts and `ofsim nuke-scenario`
# on a record and diffs the traces and the per-tick tile-state digests.
# Usage: Tools/NukeScenarioCompare.sh <name> <record> <warm> <rounds> <spacing> <tail> [doomsdaySpeed|none] [attackers] [grow] [samMod]
# Environment as Tools/TraceCompare.sh (OFSIM_WORKTREE, OFSIM_REFDIR, OFSIM_BIN, OFSIM_OUT).
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
name="${1:?name}"; record="$(realpath "${2:?record}")"
warm="${3:?warm}"; rounds="${4:?rounds}"; spacing="${5:?spacing}"; tail="${6:?tail}"
dd="${7:-none}"; att="${8:-4}"; grow="${9:-25}"; sam="${10:-25}"
tree="${OFSIM_WORKTREE:-/home/user/OpenFrontDs/Simulator/Cache/Worktrees/4e837520e886}"
refdir="${OFSIM_REFDIR:-ofsim-reference}"
bin="${OFSIM_BIN:-$here/target/debug/ofsim}"
out="${OFSIM_OUT:-/tmp/ofsim-compare}"
mkdir -p "$out"
(cd "$tree" && OFSIM_PORTED_EXECS=DoomsdayClock npx tsx "$refdir/NukeScenarioDump.ts" "$record" "$out/$name.ref.ndjson" "$warm" "$rounds" "$spacing" "$tail" "$dd" "$att" "$grow" "$sam" 2>&1 | tail -n 3)
flag=(); [ "$dd" != none ] && flag=(--doomsday "$dd")
OFSIM_INERT=1 "$bin" nuke-scenario --record "$record" --maps "$tree/resources/maps" --out "$out/$name.rs.ndjson" --warm "$warm" --rounds "$rounds" --spacing "$spacing" --tail "$tail" --attackers "$att" --grow "$grow" --sam-mod "$sam" "${flag[@]}" 2>&1 | tail -n 3
(cd "$tree" && npx tsx "$refdir/TraceDiff.ts" "$out/$name.ref.ndjson" "$out/$name.rs.ndjson")
cmp "$out/$name.ref.ndjson.digest" "$out/$name.rs.ndjson.digest" && echo "digest identical"
