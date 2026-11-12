#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stage 2 of the P6 build plan: class graph, edge diffs, TOML and Markdown output.

Responsibility: read the JSON of BuildClassify.py, group builds into code-hash classes,
build a minimum-diff tree rooted at the baseline, summarise every edge (files, enclosing
functions, upstream PRs, classification, size, risk) and write BuildClasses.toml plus
the data used by the Docs/BuildPlan*.md tables.

Usage: BuildPlanGen.py --upstream Cache/OpenFrontIO --in builds.json --toml out.toml --md-data out.json
"""
import argparse
import heapq
import json
import re
import subprocess
from collections import defaultdict
from functools import lru_cache

BASELINE = "4e837520e886"
ROOTS = ["src/core", "packages/engine/src", "packages/engine-lib/src", "packages/engine-api/src", "packages/shared/src"]
STRIP_RE = re.compile(r"^(src/core/|packages/[^/]+/src/)")
METHOD_RE = re.compile(r"^\s{0,4}(?:export\s+)?(?:(?:public|private|protected|static|async|get|set|readonly)\s+)*(?:function\s+)?([A-Za-z_$][\w$]*)\s*(?:<[^>]*>)?\(.*\)\s*(?::\s*[^{]*)?\{\s*$")
CLASS_RE = re.compile(r"^(?:export\s+)?(?:abstract\s+)?(?:class|interface|enum)\s+([A-Za-z_$][\w$]*)")
PR_RE = re.compile(r"\(#(\d+)\)")

UP = ""


def git(*args, check=True):
    r = subprocess.run(["git", "-C", UP, *args], capture_output=True, text=True, errors="replace")
    if check and r.returncode not in (0, 1):
        raise RuntimeError(r.stderr)
    return r.stdout


@lru_cache(maxsize=None)
def blob(sha):
    return git("cat-file", "blob", sha)


@lru_cache(maxsize=None)
def nlines(sha):
    return blob(sha).count("\n")


@lru_cache(maxsize=None)
def numstat(a, b):
    """Added and deleted lines between two blobs (None means absent)."""
    if a is None:
        return nlines(b), 0
    if b is None:
        return 0, nlines(a)
    out = git("diff", "--numstat", a, b).split()
    return (int(out[0]), int(out[1])) if out and out[0].isdigit() else (0, 0)


def class_distance(fa, fb):
    """Total changed lines between two file maps."""
    tot = 0
    for k in set(fa) | set(fb):
        if fa.get(k) != fb.get(k):
            a, d = numstat(fa.get(k), fb.get(k))
            tot += a + d
    return tot


def enclosing(sha, line):
    """Name of the class/method containing a 1-based line of a blob."""
    lines = blob(sha).split("\n")
    cls = meth = None
    for i in range(min(line, len(lines)) - 1, -1, -1):
        s = lines[i]
        if meth is None:
            m = METHOD_RE.match(s)
            if m and m.group(1) not in ("if", "for", "while", "switch", "catch", "return"):
                meth = m.group(1)
        c = CLASS_RE.match(s)
        if c:
            cls = c.group(1)
            break
    if meth and cls:
        return f"{cls}.{meth}"
    return meth or cls or "(top level)"


def numeric_only(a, b):
    """True when old and new differ only by numeric literals."""
    out = git("diff", "-U0", a, b)
    rem = [re.sub(r"\d+(?:\.\d+)?", "N", l[1:].strip()) for l in out.splitlines() if l.startswith("-") and not l.startswith("---")]
    add = [re.sub(r"\d+(?:\.\d+)?", "N", l[1:].strip()) for l in out.splitlines() if l.startswith("+") and not l.startswith("+++")]
    return rem == add and bool(rem)


def functions(a, b):
    """Enclosing functions of the hunks of a blob diff (child side)."""
    out = git("diff", "-U0", a, b)
    names = []
    for m in re.finditer(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@", out, re.M):
        start, cnt = int(m.group(3)), int(m.group(4) or 1)
        n = enclosing(b, max(start, 1) if cnt else max(start, 1))
        if n not in names:
            names.append(n)
    return names


def categorize(key, a, b):
    base = key.split("/")[-1]
    if a is None:
        kind = "new file"
    elif b is None:
        kind = "removed file"
    else:
        kind = "edit"
    if key == "PseudoRandom.ts":
        cat = "PRNG switch"
    elif key == "DetMath.ts" or key == "Util.ts" and a and b and "Math." in git("diff", "-U0", a, b):
        cat = "DetMath / math"
    elif key.startswith("pathfinding/") or "PathFind" in base or "AStar" in base or "Pathfind" in base:
        cat = "pathfinding change"
    elif re.search(r"(Nation|FakeHuman|Bot|Ai|AI)", base) and key.startswith("execution/"):
        cat = "AI behaviour change"
    elif key.startswith("configuration/"):
        if a and b and numeric_only(a, b):
            cat = "constant change (Config)"
        else:
            cat = "formula change (Config)"
    elif key in ("AnonNames.ts", "AnonAnimals.ts", "execution/utils/BotNames.ts", "game/Maps.gen.ts") or "Names" in base:
        cat = "data table"
    elif key.startswith("execution/") and kind == "new file":
        cat = "new execution"
    elif key.startswith("execution/") and kind == "removed file":
        cat = "removed execution"
    elif a and b and numeric_only(a, b):
        cat = "constant change"
    else:
        cat = "formula / logic change"
    return cat, kind


def prs_between(ca, cb):
    out = git("log", "--no-merges", "--format=%h\t%s", f"{ca}...{cb}", "--", *ROOTS)
    prs, n = [], 0
    for line in out.splitlines():
        n += 1
        for p in PR_RE.findall(line):
            if int(p) not in prs:
                prs.append(int(p))
    return sorted(prs), n


def edge_detail(parent, child, fa, fb):
    changes = []
    for k in sorted(set(fa) | set(fb)):
        a, b = fa.get(k), fb.get(k)
        if a == b:
            continue
        add, dele = numstat(a, b)
        cat, kind = categorize(k, a, b)
        fn = functions(a, b) if (a and b) else []
        size = nlines(b) if b else nlines(a)
        rewrite = (a is None or b is None) and size > 300 or (a and b and add + dele > 400 and (add + dele) > 0.5 * max(nlines(a), nlines(b)))
        changes.append(dict(file=k, category=cat, kind=kind, added=add, deleted=dele, functions=fn[:12], n_functions=len(fn), rewrite=bool(rewrite)))
    return changes


def main():
    global UP
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--in", dest="inp", required=True)
    ap.add_argument("--toml", required=True)
    ap.add_argument("--md-data", required=True)
    a = ap.parse_args()
    UP = a.upstream
    builds = json.load(open(a.inp))
    groups = defaultdict(list)
    for c, v in builds.items():
        groups[v["code_hash"]].append(c)
    classes = []
    for h, cs in groups.items():
        cs.sort(key=lambda c: builds[c]["commit_date"])
        classes.append(dict(hash=h, builds=cs, rep=BASELINE if BASELINE in cs else cs[0],
                            weight=sum(builds[c]["day_equiv"] for c in cs), public=sum(builds[c]["public_records"] for c in cs),
                            files=builds[cs[0]]["files"], maps=builds[BASELINE if BASELINE in cs else cs[0]]["map_files"], first=min(builds[c]["first_day"] for c in cs), last=max(builds[c]["last_day"] for c in cs)))
    classes.sort(key=lambda k: k["first"])
    n = len(classes)
    rootidx = next(i for i, k in enumerate(classes) if BASELINE in k["builds"])
    # Candidate edges: by differing file count, plus temporal neighbours.
    cand = defaultdict(set)
    for i in range(n):
        ds = sorted(((sum(1 for k in set(classes[i]["files"]) | set(classes[j]["files"]) if classes[i]["files"].get(k) != classes[j]["files"].get(k)), j) for j in range(n) if j != i))
        for _, j in ds[:7]:
            cand[i].add(j); cand[j].add(i)
        for j in (i - 2, i - 1, i + 1, i + 2):
            if 0 <= j < n:
                cand[i].add(j); cand[j].add(i)
    dist = {}
    for i in cand:
        for j in cand[i]:
            if (j, i) in dist:
                dist[(i, j)] = dist[(j, i)]
            else:
                dist[(i, j)] = class_distance(classes[i]["files"], classes[j]["files"])
    # Greedy volume-aware growth of the tree: pick the frontier edge with best weight per changed line.
    # Prim on pure line distance gives the minimum-total-diff tree; order is then chosen by volume.
    order = [rootidx]
    tree = {}
    inT = {rootidx}
    pq = []
    for j in cand[rootidx]:
        heapq.heappush(pq, (dist[(rootidx, j)], j, rootidx))
    while pq:
        d, j, i = heapq.heappop(pq)
        if j in inT:
            continue
        inT.add(j); tree[j] = i
        for k in cand[j]:
            if k not in inT:
                heapq.heappush(pq, (dist[(j, k)], k, j))
    assert len(inT) == n, (len(inT), n)
    children = defaultdict(list)
    for j, i in tree.items():
        children[i].append(j)
    front = list(children[rootidx])
    while front:
        front.sort(key=lambda j: -(classes[j]["weight"] + 0.001) / (50 + dist[(tree[j], j)]))
        j = front.pop(0)
        order.append(j)
        front.extend(children[j])
    for rank, i in enumerate(order):
        classes[i]["id"] = f"C{rank:03d}"
    out = []
    for i in order:
        k = classes[i]
        rec = dict(id=k["id"], hash=k["hash"], builds=[(c, builds[c]["tags"]) for c in k["builds"]], weight=round(k["weight"], 2),
                   public=k["public"], first=k["first"], last=k["last"], parent=None, changes=[], prs=[], lines=0, nprs_commits=0)
        if i != rootidx:
            p = tree[i]
            pk = classes[p]
            changes = edge_detail(pk["rep"], k["rep"], pk["files"], k["files"])
            prs, ncommit = prs_between(pk["rep"], k["rep"])
            mchg = {}
            for m in sorted(set(pk["maps"]) | set(k["maps"])):
                if pk["maps"].get(m) != k["maps"].get(m):
                    fa, fb = pk["maps"].get(m, {}), k["maps"].get(m, {})
                    mchg[m] = sorted(f for f in set(fa) | set(fb) if fa.get(f) != fb.get(f))
            rec["map_changes"] = mchg
            add = sum(c["added"] for c in changes)
            dele = sum(c["deleted"] for c in changes)
            cats = {c["category"] for c in changes}
            nrew = sum(1 for c in changes if c["rewrite"])
            lines = add + dele
            risk = "low"
            if lines > 300 or cats & {"AI behaviour change", "pathfinding change"}:
                risk = "medium"
            if lines > 2000 or nrew or cats & {"PRNG switch", "DetMath / math"}:
                risk = "high"
            rec.update(size_est=round(add + 0.25 * dele), risk=risk, rewrites=nrew)
            rec.update(parent=pk["id"], changes=changes, prs=prs, nprs_commits=ncommit, parent_rep=pk["rep"], rep=k["rep"],
                       lines=sum(c["added"] + c["deleted"] for c in changes))
        else:
            rec["rep"] = k["rep"]
            rec.update(size_est=0, risk="none", rewrites=0)
        out.append(rec)
    # map-file changes per map across builds in date order
    mapseq = defaultdict(list)
    for c in sorted(builds, key=lambda c: builds[c]["commit_date"]):
        for m, h in builds[c]["map_hash"].items():
            if h:
                if not mapseq[m] or mapseq[m][-1][1] != h:
                    mapseq[m].append((c, h))
    mapchg = {m: v for m, v in mapseq.items() if len(v) > 1}
    cls_of = {c: k["id"] for k in classes for c in k["builds"] if "id" in k}
    variants = defaultdict(set)
    for c, v in builds.items():
        for m, fl in v["map_files"].items():
            for fn, sha in fl.items():
                variants[(m, fn)].add(sha)
    mapstats = {}
    for (m, fn), shas in variants.items():
        mapstats.setdefault(fn, dict(maps=0, variants=0, changed_maps=0))
        mapstats[fn]["maps"] += 1
        mapstats[fn]["variants"] += len(shas)
        mapstats[fn]["changed_maps"] += len(shas) > 1
    json.dump(dict(classes=out, mapchanges=mapchg, mapstats=mapstats, builds={c: dict(cls=cls_of[c], tags=v["tags"], first=v["first_day"], last=v["last_day"], records=v["n_records"], public=v["public_records"], day_equiv=v["day_equiv"]) for c, v in builds.items()}), open(a.md_data, "w"), indent=1)
    with open(a.toml, "w") as f:
        f.write("# SPDX-License-Identifier: AGPL-3.0-only\n# Generated by Tools/BuildPlanGen.py from Tools/BuildClassify.py output; do not edit by hand.\n")
        f.write("# [builds]: commit12 -> class id. [class.<id>]: parent, weight (estimated days of public play), builds, changes (relative to the parent class).\n\n[builds]\n")
        for c in sorted(builds, key=lambda c: builds[c]["commit_date"]):
            f.write(f'{c} = "{cls_of[c]}"\n')
        for r in out:
            f.write(f'\n[class.{r["id"]}]\n')
            f.write(f'hash = "{r["hash"]}"\n')
            f.write(f'parent = "{r["parent"] or ""}"\nweight = {r["weight"]}\nlines_changed = {r["lines"]}\nsize_est_rust_lines = {r["size_est"]}\nrisk = "{r["risk"]}"\n')
            f.write("builds = [" + ", ".join(f'"{c}"' for c, _ in r["builds"]) + "]\n")
            mc = [f"{m}:{x}" for m, fl in sorted(r.get("map_changes", {}).items()) for x in fl]
            f.write("maps_changed = [" + ", ".join(json.dumps(x) for x in mc) + "]\n")
            f.write("prs = [" + ", ".join(str(p) for p in r["prs"]) + "]\n")
            for ch in r["changes"]:
                f.write(f'\n[[class.{r["id"]}.change]]\nname = "{ch["file"]}"\nkind = "{ch["category"]}"\nlines = {ch["added"] + ch["deleted"]}\nrewrite = {str(ch["rewrite"]).lower()}\n')
                f.write("functions = [" + ", ".join(json.dumps(x) for x in ch["functions"]) + "]\n")


if __name__ == "__main__":
    main()
