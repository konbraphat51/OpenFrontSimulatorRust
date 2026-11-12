#!/usr/bin/env bash
# Runs `ofsim verify` over every record of a directory and prints a summary table.
# Usage: Tools/VerifyAll.sh <records dir> <maps dir> [--until T]   (build first: cargo build --release)
set -uo pipefail
cd "$(dirname "$0")/.."
records="${1:?records dir}"
maps="${2:?maps dir}"
shift 2
bin="${OFSIM_BIN:-target/release/ofsim}"
[ -x "$bin" ] || bin="target/debug/ofsim"
printf '%-12s %-11s %-9s %-8s %s\n' record verdict matched ticks detail
ok=0
total=0
for f in "$records"/*.json.gz "$records"/*.json; do
  [ -e "$f" ] || continue
  total=$((total + 1))
  line="$("$bin" verify --record "$f" --maps "$maps" "$@" 2>&1 | tail -n 1)"
  name="$(echo "$line" | awk '{print $1}')"
  verdict="$(echo "$line" | awk '{print $2}')"
  matched="$(echo "$line" | grep -o 'matched=[0-9/]*' | cut -d= -f2)"
  ticks="$(echo "$line" | grep -o 'ticks_run=[0-9]*' | cut -d= -f2)"
  detail="$(echo "$line" | sed -E 's/^[^ ]+ [A-Z_]+ matched=[0-9/]+ turns=[0-9]+ ticks_run=[0-9]+ ?//')"
  [ "$verdict" = "OK" ] && ok=$((ok + 1))
  printf '%-12s %-11s %-9s %-8s %s\n' "${name:-?}" "${verdict:-ERROR}" "${matched:--}" "${ticks:--}" "${detail:-$line}"
done
echo "OK: $ok / $total"
