#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-only
# Fast regression check: the largest records of every build of every ported class (VerifyClass.sh --quick).
# Usage: Tools/QuickRegression.sh [--per-build N (default 1)] [--jobs N] [class ...]   (default: every class in BuildClasses.toml)
# Prints one summary line per class and the non-ok records; classes whose builds are not ported yet are listed once at the end.
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
per=1; jobs=3; classes=()
while [ $# -gt 0 ]; do
  case "$1" in
    --per-build) per="${2:?count}"; shift ;;
    --jobs) jobs="${2:?jobs}"; shift ;;
    -h|--help) sed -n 3,5p "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) classes+=("$1") ;;
  esac
  shift
done
if [ ${#classes[@]} -eq 0 ]; then
  mapfile -t classes < <(grep -o '^\[class\.C[0-9]*\]' "$here/Crates/Engine/src/Builds/BuildClasses.toml" | tr -d '[]' | sed 's/class\.//')
fi
unported=""
for c in "${classes[@]}"; do
  res="$("$here/Tools/VerifyClass.sh" "$c" --quick "$per" --jobs "$jobs" 2>/dev/null)"
  # A class is unported when no record ran and every failure is the "build <commit> ... not ported" error.
  if echo "$res" | grep -q ' ok=0 ' && ! echo "$res" | grep -E '^\s+[A-Za-z0-9]{8} ' | grep -vq 'error record commit [0-9a-f]*: build '; then unported="$unported $c"; continue; fi
  echo "$res" | sed -n 2p | sed "s/^/$c: /"
  echo "$res" | grep -E '^\s+[A-Za-z0-9]{8} ' | grep -v ' ok' || true
done
[ -n "$unported" ] && echo "not ported:$unported"
exit 0
