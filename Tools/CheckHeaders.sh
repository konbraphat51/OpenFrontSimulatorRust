#!/usr/bin/env bash
# Fails when a tracked .rs, .ts, .mts or .mjs file does not start with the SPDX license line.
set -euo pipefail
cd "$(dirname "$0")/.."
expected='// SPDX-License-Identifier: AGPL-3.0-only'
status=0
while IFS= read -r file; do
  if [ "$(head -n 1 "$file")" != "$expected" ]; then
    echo "missing SPDX header: $file"
    status=1
  fi
done < <(git ls-files '*.rs' '*.ts' '*.mts' '*.mjs')
exit $status
