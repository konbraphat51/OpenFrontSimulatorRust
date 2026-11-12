#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Verifies every record of every build of one class with `ofsim batch --verify-only`, then prints the BatchSummary.
# Usage: Tools/VerifyClass.sh <class> [--jobs N] [--quick N] [--keep] [--verbose]
#   --quick N: only the N largest records of each build (a fast regression check).
#   Records come from <Cache>/Records/<commit12>/ for each build of the class in BuildClasses.toml; maps and upstream
#   paths are set for you. NDJSON goes to a temp dir (printed with --keep or --verbose, otherwise deleted).
# Builds nothing: rebuild the release binary first (a stale binary gives false desyncs).
# Env: OFSIM_BIN (default target/verify/ofsim, else target/release/ofsim), OFSIM_MAPS (default <Cache>/Worktrees/4e837520e886/resources/maps),
# OFSIM_UPSTREAM (default <Cache>/OpenFrontIO), OFSIM_RECORDS (default <Cache>/Records), OFSIM_CACHE, OFSIM_TMP.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -n "${OFSIM_CACHE:-}" ]; then cache="$OFSIM_CACHE"
elif [ "$(basename "$(dirname "$here")")" = "Wt" ]; then cache="$(dirname "$(dirname "$here")")"
else cache="$here/Cache"; fi
bin="${OFSIM_BIN:-}"
if [ -z "$bin" ]; then bin="$here/target/verify/ofsim"; [ -x "$bin" ] || bin="$here/target/release/ofsim"; fi
maps="${OFSIM_MAPS:-$cache/Worktrees/4e837520e886/resources/maps}"
upstream="${OFSIM_UPSTREAM:-$cache/OpenFrontIO}"
records="${OFSIM_RECORDS:-$cache/Records}"
toml="$here/Crates/Engine/src/Builds/BuildClasses.toml"

class=""; jobs=2; keep=0; verbose=0; quick=0
while [ $# -gt 0 ]; do
  case "$1" in
    --jobs) jobs="${2:?jobs}"; shift ;;
    --quick) quick="${2:?count}"; shift ;;
    --keep) keep=1 ;;
    --verbose) verbose=1 ;;
    -h|--help) sed -n 3,10p "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) class="$1" ;;
  esac
  shift
done
[ -n "$class" ] || { echo "usage: VerifyClass.sh <class> [--jobs N] (see --help)" >&2; exit 2; }
[ -x "$bin" ] || { echo "ofsim binary not found at $bin (set OFSIM_BIN; this tool builds nothing)" >&2; exit 2; }
builds="$(awk -v c="[class.$class]" '$0==c{f=1;next} /^\[/{f=0} f&&/^builds = /{print}' "$toml" | grep -o '[0-9a-f]\{12\}')"
[ -n "$builds" ] || { echo "unknown class $class" >&2; exit 2; }

out="$(mktemp -d "${OFSIM_TMP:-${TMPDIR:-/tmp}}/verifyclass.XXXXXX")"
[ "$keep" = 1 ] || [ "$verbose" = 1 ] || trap 'rm -rf "$out"' EXIT
missing=""
for b in $builds; do
  if ls "$records/$b"/*.json* >/dev/null 2>&1; then
    src="$records/$b"
    if [ "$quick" -gt 0 ]; then
      src="$out/quick/$b"; mkdir -p "$src"
      ls -S "$records/$b"/*.json* | head -n "$quick" | while read -r f; do ln -s "$f" "$src/"; done
    fi
    "$bin" batch --records "$src" --maps "$maps" --upstream "$upstream" --out "$out/work" --verify-only --jobs "$jobs" > "$out/$b.ndjson" 2>"$out/$b.err"
    [ -s "$out/$b.err" ] && [ "$verbose" = 1 ] && sed 's/^/  stderr: /' "$out/$b.err" | head -5
  else missing="$missing $b"; fi
done
[ -n "$missing" ] && echo "no records for:$missing" >&2
echo "$class builds: $(echo $builds | tr ' ' ',')"
cat "$out"/*.ndjson 2>/dev/null | python3 "$here/Tools/BatchSummary.py" --by-build "$records"
{ [ "$keep" = 1 ] || [ "$verbose" = 1 ]; } && echo "ndjson in $out"
exit 0
