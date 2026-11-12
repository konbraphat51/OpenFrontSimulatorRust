#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# First-look triage of one record: commit, class, first mismatching tick, optional differential trace and flag bisect.
# Usage: Tools/Triage.sh <record.json.gz> [--trace] [--bisect-flags] [--depth N] [--until T] [--verbose]
#   (no flags)       one verify line plus the class of the record's commit
#   --trace          run Tools/TraceCompare.sh up to the first mismatching tick, print the first 20 lines of the diff
#                    (needs OFSIM_WORKTREE = upstream worktree of the record's commit, with node_modules)
#   --bisect-flags   rerun verify once per Change flag of the class's Edges/<class>.toml with OFSIM_TOGGLE=<flag>, one line each
#   --depth N        with --bisect-flags, also the flags of the N-1 ancestor edges (default 1: own edge only)
#   --until T        stop each verify at tick T (faster bisect)    --verbose  print the raw verify lines
# Builds nothing. Env: OFSIM_BIN (default target/verify/ofsim, else target/release/ofsim), OFSIM_MAPS (default <Cache>/Worktrees/4e837520e886/resources/maps),
# OFSIM_UPSTREAM (default <Cache>/OpenFrontIO), OFSIM_WORKTREE, OFSIM_OUT (trace output dir), OFSIM_CACHE (Cache/ location).
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${OFSIM_CACHE:-}" ]; then cache="$OFSIM_CACHE"
elif [ "$(basename "$(dirname "$here")")" = "Wt" ]; then cache="$(dirname "$(dirname "$here")")"
else cache="$here/Cache"; fi
bin="${OFSIM_BIN:-}"
if [ -z "$bin" ]; then bin="$here/target/verify/ofsim"; [ -x "$bin" ] || bin="$here/target/release/ofsim"; fi
maps="${OFSIM_MAPS:-$cache/Worktrees/4e837520e886/resources/maps}"
upstream="${OFSIM_UPSTREAM:-$cache/OpenFrontIO}"
toml="$here/Crates/Engine/src/Builds/BuildClasses.toml"
edges="$here/Crates/Engine/src/Builds/Edges"

record=""; trace=0; bisect=0; depth=1; until=""; verbose=0
while [ $# -gt 0 ]; do
  case "$1" in
    --trace) trace=1 ;;
    --bisect-flags) bisect=1 ;;
    --depth) depth="${2:?depth}"; shift ;;
    --until) until="${2:?tick}"; shift ;;
    --verbose) verbose=1 ;;
    -h|--help) sed -n 3,13p "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) record="$1" ;;
  esac
  shift
done
[ -f "$record" ] || { echo "usage: Triage.sh <record.json.gz> [--trace] [--bisect-flags] [--depth N] (see --help)" >&2; exit 2; }
[ -x "$bin" ] || { echo "ofsim binary not found at $bin (set OFSIM_BIN; this tool builds nothing)" >&2; exit 2; }

commit="$(zcat -f "$record" | head -c 400 | grep -o '"gitCommit":"[0-9a-f]*"' | head -1 | cut -d'"' -f4)"
c12="${commit:0:12}"
class="$(grep -m1 "^$c12 = " "$toml" | cut -d'"' -f2)"
name="$(basename "$record" .json.gz)"

verify() { # env toggle in $1 (may be empty); prints the verify line
  local extra=(); [ -n "$until" ] && extra=(--until "$until")
  OFSIM_TOGGLE="$1" "$bin" verify --record "$record" --maps "$maps" --upstream "$upstream" "${extra[@]}" 2>&1 | head -n 3
}
# One-line summary of a verify line: verdict, matched, first tick.
brief() {
  local l="$1" v t
  v="$(echo "$l" | awk '{print $2}')"
  t="$(echo "$l" | grep -o 'first_mismatch_tick=[0-9]*\|stop_tick=[0-9]*' | head -1)"
  echo "$v $(echo "$l" | grep -o 'matched=[0-9/]*') ${t:-}" | cut -c1-120
}

base="$(verify "")"
echo "$name commit=$c12 class=${class:-?} $(brief "$base")"
[ "$verbose" = 1 ] && echo "$base"
tick="$(echo "$base" | grep -o 'first_mismatch_tick=[0-9]*\|stop_tick=[0-9]*' | head -1 | cut -d= -f2)"

if [ "$trace" = 1 ]; then
  if [ -z "$tick" ]; then echo "trace: no mismatch tick, nothing to compare"
  elif [ -z "${OFSIM_WORKTREE:-}" ]; then echo "trace: set OFSIM_WORKTREE to the upstream worktree of commit $c12" >&2
  else
    OFSIM_BIN="$bin" OFSIM_UPSTREAM="$upstream" "$here/Tools/TraceCompare.sh" "$record" "$tick" 2>&1 | grep -v '^$' | tail -n 20
  fi
fi

if [ "$bisect" = 1 ]; then
  # Flags of the class edge and of up to depth-1 ancestor edges, with the class they come from.
  flags="$(python3 - "$toml" "$edges" "$class" "$depth" <<'PY'
import re, sys
import os
toml, edges, cls, depth = open(sys.argv[1]).read(), sys.argv[2], sys.argv[3], int(sys.argv[4])
for _ in range(depth):
    path = os.path.join(edges, cls + ".toml")
    if os.path.exists(path):
        for f in re.findall(r'"(\w+)"', open(path).read()):
            print(cls, f)
    p = re.search(r'^\[class\.%s\]\n(?:.*\n)*?parent = "(\w*)"' % cls, toml, re.M)
    if not p or not p.group(1):
        break
    cls = p.group(1)
PY
)"
  if [ -z "$flags" ]; then echo "bisect: no Change flags in Edges/ for ${class:-?} (class not ported or empty edge)"; fi
  while read -r from flag; do
    [ -z "$flag" ] && continue
    r="$(verify "$flag")"
    nt="$(echo "$r" | grep -o 'first_mismatch_tick=[0-9]*\|stop_tick=[0-9]*' | head -1 | cut -d= -f2)"
    nv="$(echo "$r" | awk '{print $2}')"
    bv="$(echo "$base" | awk '{print $2}')"
    if [ "$bv" = "OK" ] || [ "$bv" = "PARTIAL" ]; then
      if [ "$nv" = "OK" ] || [ "$nv" = "PARTIAL" ]; then eff="no effect"; else eff="BREAKS at ${nt:-?}"; fi
    elif [ "$nv" = "OK" ] || [ "$nv" = "PARTIAL" ]; then eff="REMOVES mismatch"
    elif [ "$nt" != "$tick" ]; then eff="MOVES ${tick:-none} -> ${nt:-none}"
    else eff="no effect"; fi
    echo "  toggle $flag ($from): $eff | $(brief "$r")" | cut -c1-160
  done <<<"$flags"
fi
