#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Throughput benchmark: times fixed long records per era in verify and replay mode (ticks/s, peak RSS).

Usage: Tools/Bench.py [--mode verify|replay|both] [--runs N] [--set default|<record path>...] [--every N] [--json out.json]
Each run is one `ofsim batch --jobs 1` process on one record (release binary; env OFSIM_BIN), so peak RSS is per record.
With --runs N the best (fastest) run is reported. `--scale` additionally times `batch --jobs 1,2,4` over the whole default set.
Env: OFSIM_BIN (default target/release/ofsim), OFSIM_MAPS, OFSIM_UPSTREAM, OFSIM_RECORDS, OFSIM_CACHE.
"""
import argparse
import json
import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

here = Path(__file__).resolve().parent.parent
if os.environ.get("OFSIM_CACHE"):
    cache = Path(os.environ["OFSIM_CACHE"])
elif here.parent.name == "Wt":
    cache = here.parent.parent
else:
    cache = here / "Cache"
bin_path = Path(os.environ.get("OFSIM_BIN", here / "target/release/ofsim"))
maps = Path(os.environ.get("OFSIM_MAPS", cache / "Worktrees/4e837520e886/resources/maps"))
upstream = Path(os.environ.get("OFSIM_UPSTREAM", cache / "OpenFrontIO"))
records = Path(os.environ.get("OFSIM_RECORDS", cache / "Records"))

# (class, commit12/record) fixed set: the longest local record of one build per era.
DEFAULT_SET = [
    ("C000", "a188868bc16c/dqD3tRRz64"),
    ("C023", "dd1277e245b5/tEJEgWSc"),
    ("C040", "de54f7c1a161/HQ4dbm2d"),
    ("C049", "106938c395bb/FMHzJ8XF"),
    ("C059", "793efe4dd1ba/67986TJw"),
    ("C081", "dfd420fd7836/cjPSE6enhb"),
    ("C088", "0211f17b71ef/cxzGcV5tJm"),
]


def run_batch(src_dir: Path, mode: str, jobs: int, every: int, out: Path):
    """Runs one batch in a helper process; returns (result lines, wall seconds, peak RSS in MB of that batch)."""
    cmd = [str(bin_path), "batch", "--records", str(src_dir), "--maps", str(maps), "--upstream", str(upstream),
           "--out", str(out), "--jobs", str(jobs)]
    cmd += ["--verify-only"] if mode == "verify" else ["--every", str(every), "--force"]
    code = ("import resource,subprocess,sys;p=subprocess.run(sys.argv[1:],capture_output=True,text=True);"
            "sys.stdout.write(p.stdout);print('RSS',resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)")
    t0 = time.perf_counter()
    p = subprocess.run([sys.executable, "-c", code] + cmd, capture_output=True, text=True)
    wall = time.perf_counter() - t0
    lines = [json.loads(l) for l in p.stdout.splitlines() if l.startswith("{")]
    rss = [int(l.split()[1]) for l in p.stdout.splitlines() if l.startswith("RSS ")]
    return lines, wall, (rss[0] / 1024 if rss else 0)


def bench_one(label: str, rec: Path, mode: str, runs: int, every: int):
    tmp = Path(tempfile.mkdtemp(prefix="ofbench."))
    try:
        src = tmp / "src"
        src.mkdir()
        (src / rec.name).symlink_to(rec)
        best = None
        for _ in range(runs):
            lines, wall, rss = run_batch(src, mode, 1, every, tmp / "out")
            if not lines or lines[0]["status"] != "ok":
                return {"label": label, "mode": mode, "status": lines[0]["status"] if lines else "none", "record": rec.stem}
            if best is None or lines[0]["seconds"] < best["seconds"]:
                best = dict(lines[0], rssMB=round(rss))
        best["label"] = label
        best["mode"] = mode
        return best
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def scale(runs: int):
    tmp = Path(tempfile.mkdtemp(prefix="ofbench."))
    try:
        src = tmp / "src"
        src.mkdir()
        for _, r in DEFAULT_SET:
            (src / (r.replace("/", "_") + ".json.gz")).symlink_to(records / (r + ".json.gz"))
        res = []
        for jobs in (1, 2, 4):
            lines, wall, rss = run_batch(src, "verify", jobs, 100, tmp / "out")
            ticks = sum(l["ticks"] for l in lines)
            res.append({"jobs": jobs, "wall": round(wall, 1), "ticks": ticks, "ticksPerSecond": round(ticks / wall), "peakRssMB": round(rss)})
        return res
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--mode", default="both", choices=["verify", "replay", "both"])
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--every", type=int, default=100)
    ap.add_argument("--json")
    ap.add_argument("--scale", action="store_true")
    ap.add_argument("--only", help="comma separated classes")
    ap.add_argument("records", nargs="*", help="explicit record paths instead of the default set")
    a = ap.parse_args()
    items = [(Path(r).stem, Path(r)) for r in a.records] if a.records else [
        (c, records / (r + ".json.gz")) for c, r in DEFAULT_SET if not a.only or c in a.only.split(",")]
    modes = ["verify", "replay"] if a.mode == "both" else [a.mode]
    out = []
    print(f"{'class':6} {'record':12} {'mode':6} {'ticks':>7} {'sec':>7} {'ticks/s':>8} {'RSS MB':>7} status")
    for label, rec in items:
        for m in modes:
            r = bench_one(label, rec, m, a.runs, a.every)
            out.append(r)
            if "ticks" in r:
                print(f"{label:6} {r['record']:12} {m:6} {r['ticks']:7} {r['seconds']:7.2f} {r['ticksPerSecond']:8} {r['rssMB']:7} {r['status']}", flush=True)
            else:
                print(f"{label:6} {r['record']:12} {m:6} FAILED {r['status']}", flush=True)
    tot = {m: [sum(x["ticks"] for x in out if x["mode"] == m and "ticks" in x), sum(x["seconds"] for x in out if x["mode"] == m and "ticks" in x)] for m in modes}
    for m, (t, s) in tot.items():
        if s:
            print(f"total {m}: {t} ticks in {s:.1f} s = {t / s:.0f} ticks/s")
    if a.scale:
        out.append({"scale": scale(a.runs)})
        for s in out[-1]["scale"]:
            print(f"batch --jobs {s['jobs']}: {s['wall']} s, {s['ticksPerSecond']} ticks/s, peak RSS (largest process child) {s['peakRssMB']} MB")
    if a.json:
        Path(a.json).write_text(json.dumps(out, indent=1))


main()
