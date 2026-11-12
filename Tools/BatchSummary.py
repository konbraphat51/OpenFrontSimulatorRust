#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compact summary of `ofsim batch` NDJSON output.

Prints one totals line (ok, desync, error, skipped counts and hashes checked) and one line
per non-ok record (record, status, first mismatching tick or error, cut to 120 characters).
With --by-build <records root> (the directory holding <commit12>/<record>.json.gz) it also
prints counts per commit and class.

Usage: BatchSummary.py <ndjson...> [--by-build RECORDS_ROOT] [--width N] [--verbose]
Reads stdin when no file is given or the file is "-". --verbose adds ticks and speed to the totals.
"""
import argparse
import functools
import json
import os
import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOML = ROOT / "Crates/Engine/src/Builds/BuildClasses.toml"


def read_records(files):
    """Parsed NDJSON objects; unparsable lines count as error rows."""
    for f in files:
        fh = sys.stdin if f == "-" else open(f, encoding="utf-8")
        with fh:
            for ln in fh:
                ln = ln.strip()
                if not ln:
                    continue
                try:
                    yield json.loads(ln)
                except ValueError:
                    yield {"record": "?", "status": "error", "error": "unparsable line: " + ln[:60]}


def build_index(root):
    """record name -> commit12 for every record file below root."""
    idx = {}
    for p in Path(root).glob("*/*.json*"):
        idx[p.name.split(".")[0]] = p.parent.name[:12]
    return idx


@functools.lru_cache(maxsize=None)
def class_of(commit):
    """Class id of a commit12 from BuildClasses.toml, or '?'."""
    m = re.search(rf'^{commit} = "(\w+)"', TOML.read_text(), re.M)
    return m.group(1) if m else "?"


def main():
    ap = argparse.ArgumentParser(description="One-line summary of ofsim batch NDJSON output.")
    ap.add_argument("files", nargs="*", metavar="ndjson", help="NDJSON files (default: stdin)")
    ap.add_argument("--by-build", metavar="RECORDS_ROOT", help="group counts by commit and class")
    ap.add_argument("--width", type=int, default=120, help="cut non-ok lines to this width (default 120)")
    ap.add_argument("--verbose", action="store_true", help="add total ticks and seconds")
    a = ap.parse_args()
    rows = list(read_records(a.files or ["-"]))
    counts = defaultdict(int)
    hashes = ticks = 0
    secs = 0.0
    bad = []
    for r in rows:
        st = r.get("status", "error")
        counts[st] += 1
        hashes += int(r.get("hashesChecked", 0) or 0)
        ticks += int(r.get("ticks", 0) or 0)
        secs += float(r.get("seconds", 0) or 0)
        if st not in ("ok", "skipped"):
            bad.append(r)
    idx = build_index(a.by_build) if a.by_build else {}
    tail = f" ticks={ticks} cpu_s={secs:.0f}" if a.verbose else ""
    print(f"records={len(rows)} ok={counts['ok']} desync={counts['desync']} error={counts['error']}"
          f"{' skipped=' + str(counts['skipped']) if counts['skipped'] else ''} hashes={hashes}{tail}")
    for r in bad:
        what = f"tick {r['firstMismatchTick']}" if r.get("firstMismatchTick") is not None else ""
        if r.get("error"):
            what = (what + " " if what else "") + str(r["error"]).replace("\n", " ")
        c = idx.get(str(r.get("record")))
        where = f" [{c} {class_of(c)}]" if c else ""
        print(f"  {r.get('record')} {r.get('status')} {what}{where}"[: a.width])
    if a.by_build:
        g = defaultdict(lambda: defaultdict(int))
        for r in rows:
            c = idx.get(str(r.get("record")), "?")
            g[c][r.get("status", "error")] += 1
            g[c]["hashes"] += int(r.get("hashesChecked", 0) or 0)
        for c in sorted(g, key=lambda c: (class_of(c), c)):
            d = g[c]
            print(f"  {class_of(c)} {c} ok={d['ok']} desync={d['desync']} error={d['error']} hashes={d['hashes']}")


if __name__ == "__main__":
    try:
        main()
        sys.stdout.flush()
    except BrokenPipeError:
        # Reader closed the pipe (`| head`): exit quietly, no traceback or flush error at shutdown.
        os.dup2(os.open(os.devnull, os.O_WRONLY), sys.stdout.fileno())
        sys.exit(0)
