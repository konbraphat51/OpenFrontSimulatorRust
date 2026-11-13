#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Opt-in profile-guided build of `ofsim` (about 5 to 8 percent faster; state hashes are unaffected).
# Usage: Tools/BuildPgo.sh [--until TICKS (default 4000)]
#   1. builds an instrumented binary into target/pgo-gen, 2. replays the first TICKS ticks of the benchmark
#   records (Tools/Bench.py set) to collect a profile, 3. builds the optimised binary into target/pgo.
# Result: target/pgo/release/ofsim (use it with OFSIM_BIN, for example `OFSIM_BIN=target/pgo/release/ofsim Tools/Bench.py`).
# Needs `rustup component add llvm-tools` (llvm-profdata) and the records under Cache/Records.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${OFSIM_CACHE:-}" ]; then cache="$OFSIM_CACHE"
elif [ "$(basename "$(dirname "$here")")" = "Wt" ]; then cache="$(dirname "$(dirname "$here")")"
else cache="$here/Cache"; fi
until=4000
while [ $# -gt 0 ]; do
  case "$1" in
    --until) until="${2:?ticks}"; shift ;;
    -h|--help) sed -n 3,9p "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
  shift
done
profdata="$(find "$(rustc --print sysroot)" -name llvm-profdata -type f | head -n 1)"
[ -n "$profdata" ] || { echo "llvm-profdata not found: run 'rustup component add llvm-tools'" >&2; exit 2; }
maps="${OFSIM_MAPS:-$cache/Worktrees/4e837520e886/resources/maps}"
upstream="${OFSIM_UPSTREAM:-$cache/OpenFrontIO}"
records="${OFSIM_RECORDS:-$cache/Records}"
raw="$here/target/pgo-data"
rm -rf "$raw"; mkdir -p "$raw"
RUSTFLAGS="-Cprofile-generate=$raw" cargo build --release -q --target-dir "$here/target/pgo-gen"
for r in $(python3 - "$here" <<'PY'
import re, sys
text = open(sys.argv[1] + "/Tools/Bench.py").read()
print(" ".join(re.findall(r'\("C\d+", "([0-9a-f]+/[A-Za-z0-9]+)"\)', text)))
PY
); do
  [ -f "$records/$r.json.gz" ] || { echo "skip missing $r" >&2; continue; }
  LLVM_PROFILE_FILE="$raw/%p.profraw" "$here/target/pgo-gen/release/ofsim" verify --record "$records/$r.json.gz" \
    --maps "$maps" --upstream "$upstream" --until "$until" >/dev/null
done
"$profdata" merge -o "$here/target/pgo.profdata" "$raw"/*.profraw
RUSTFLAGS="-Cprofile-use=$here/target/pgo.profdata" cargo build --release -q --target-dir "$here/target/pgo"
echo "built $here/target/pgo/release/ofsim"
