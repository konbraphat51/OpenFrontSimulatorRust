#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stage 1 of the P6 build plan: classify deployed builds by simulation-code content hash.

Responsibility: for each build in Cache/Records/Index.csv, hash the simulation-relevant
files of the upstream core (pre-split src/core, post-split packages/{engine,engine-lib,
engine-api,shared}/src) after dropping the exclusion list below, hash the map files used
by the build's records, collect Node/Docker facts and the sighting weights, and write
one JSON file for BuildPlanGen.py.

Usage: BuildClassify.py --upstream Cache/OpenFrontIO --records Cache/Records --out out.json
"""
import argparse
import csv
import hashlib
import json
import re
import subprocess
from collections import defaultdict

CORE_RE = re.compile(r"^(src/core/|packages/(engine|engine-lib|engine-api|shared)/src/)")
STRIP_RE = re.compile(r"^(src/core/|packages/[^/]+/src/)")
MAP_RE = re.compile(r"^resources/maps/([^/]+)/(map\.bin|map4x\.bin|map16x\.bin|mini_map\.bin|manifest\.json)$")

# Files dropped from the hash: wire/API schemas, client-only views, themes, transport, test-only.
# When unsure a file stays included (Schemas.ts, configuration/*Config.ts, Maps.gen.ts, names).
EXCLUDED = {
    "ApiSchemas.ts", "ClanApiSchemas.ts", "CosmeticSchemas.ts", "StatsSchemas.ts",
    "WireSchemas.ts", "WorkerSchemas.ts", "ZbinWire.ts", "LevelBadgeWire.ts",
    "AssetPaths.ts", "AssetUrls.ts", "Base64.ts", "CloseCodes.ts", "ClusterConfig.ts",
    "CustomFlag.ts", "EventBus.ts", "Format.ts", "PatternDecoder.ts", "ServerList.ts",
    "MapPlayerCounts.ts", "FetchGameMapLoader.ts", "GameMapLoader.ts",
    "game/FetchGameMapLoader.ts", "game/GameMapLoader.ts",
    "game/GameView.ts", "game/ReadViews.ts", "game/NameBoxCalculator.ts",
    "game/UserSettings.ts", "game/SnapshotHeader.ts",
    "configuration/PastelTheme.ts", "configuration/PastelThemeDark.ts",
    "configuration/Colors.ts", "configuration/ColorAllocator.ts",
    "utilities/DebugSpan.ts", "snapshot/README.md", "worker/tsconfig.json",
}
EXCLUDED_PREFIX = ("worker/", "snapshot/")


def git(up, *args):
    return subprocess.run(["git", "-C", up, *args], capture_output=True, text=True, check=True).stdout


def norm_map_dir(name):
    return re.sub(r"[^a-z0-9]", "", name.lower())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--records", required=True)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    rows = list(csv.DictReader(open(f"{a.records}/Index.csv")))
    builds = defaultdict(list)
    for r in rows:
        builds[r["gitCommit"][:12]].append(r)
    out = {}
    for c12, recs in builds.items():
        full = git(a.upstream, "rev-parse", c12).strip()
        files, maps = {}, defaultdict(dict)
        for line in git(a.upstream, "ls-tree", "-r", full).splitlines():
            meta, path = line.split("\t", 1)
            sha = meta.split()[2]
            if CORE_RE.match(path):
                key = STRIP_RE.sub("", path)
                if key in EXCLUDED or key.startswith(EXCLUDED_PREFIX):
                    continue
                assert key not in files, (c12, key)
                files[key] = sha
            else:
                m = MAP_RE.match(path)
                if m:
                    maps[m.group(1)][m.group(2)] = sha
        h = hashlib.sha256()
        for k in sorted(files):
            h.update(f"{k}\0{files[k]}\n".encode())
        used = sorted({norm_map_dir(r["map"]) for r in recs})
        map_hash = {}
        for m in used:
            if m in maps:
                map_hash[m] = hashlib.sha256(json.dumps(maps[m], sort_keys=True).encode()).hexdigest()[:12]
            else:
                map_hash[m] = None
        docker = git(a.upstream, "show", f"{full}:Dockerfile")
        froms = re.findall(r"^FROM\s+(\S+)", docker, re.M)
        try:
            pkg = json.loads(git(a.upstream, "show", f"{full}:package.json"))
            engines = pkg.get("engines", {})
        except Exception:
            engines = {}
        try:
            nvm = git(a.upstream, "show", f"{full}:.nvmrc").strip()
        except Exception:
            nvm = None
        tags = git(a.upstream, "tag", "--points-at", full).split()
        date = git(a.upstream, "log", "-1", "--format=%cI", full).strip()
        out[c12] = dict(full=full, code_hash=h.hexdigest()[:16], files=files, map_hash=map_hash, map_files=maps,
                        docker_from=froms, engines=engines, nvmrc=nvm, tags=tags, commit_date=date,
                        n_records=len(recs))
    # Sighting weights: one day split among builds by share of that day's Public samples.
    perday = defaultdict(lambda: defaultdict(int))
    perday_all = defaultdict(lambda: defaultdict(int))
    pub = defaultdict(int)
    for r in rows:
        c = r["gitCommit"][:12]
        perday_all[r["day"]][c] += 1
        if r["type"] == "Public":
            perday[r["day"]][c] += 1
            pub[c] += 1
    dayeq = defaultdict(float)
    days = defaultdict(set)
    for day, d in perday_all.items():
        src = perday[day] or d
        tot = sum(src.values())
        for c, n in src.items():
            dayeq[c] += n / tot
        for c in d:
            days[c].add(day)
    for c, o in out.items():
        o["public_records"] = pub[c]
        o["day_equiv"] = round(dayeq[c], 3)
        o["days_seen"] = len(days[c])
        o["first_day"] = min(days[c])
        o["last_day"] = max(days[c])
    json.dump(out, open(a.out, "w"), indent=1)


if __name__ == "__main__":
    main()
