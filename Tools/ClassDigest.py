#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compact upstream digest of one build class: what changed versus its parent class.

Resolves the representative (first) commit of the class and of its parent from
BuildClasses.toml, diffs upstream between them (engine roots only, whitespace ignored,
files that cannot change the simulation dropped), groups hunks by file and enclosing
function, and maps every file to the Rust file that ports it (`//! Port of src/core/..`).
Also lists PR numbers between the commits and the Change flags already in the class's
EDGES entry of Rules.rs.

Usage: ClassDigest.py <class> [--upstream DIR] [--files-only] [--max-lines N] [--verbose]
Env: OFSIM_UPSTREAM overrides the upstream clone (default <Cache>/OpenFrontIO); OFSIM_CACHE overrides Cache/.
"""
import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
from BuildPlanGen import CLASS_RE, METHOD_RE, ROOTS, STRIP_RE  # noqa: E402

# Cache/ of the checkout; a worktree under Cache/Wt/<name> shares the main checkout's Cache/.
CACHE = Path(os.environ.get("OFSIM_CACHE") or (ROOT.parent.parent if ROOT.parent.name == "Wt" else ROOT / "Cache"))
TOML = ROOT / "Crates/Engine/src/Builds/BuildClasses.toml"
RULES = ROOT / "Crates/Engine/src/Builds/Rules.rs"
# Paths that cannot change simulation state: tests, wire schemas, stats, display, i18n, map loaders, docs.
EXCLUDE_RE = re.compile(
    r"(^|/)(tests?|__tests__|lang|i18n|wire)/|\.(test|spec)\.ts$|\.(json|md|txt|d\.ts)$"
    r"|Schemas?\.ts$|Stats(Impl|Schemas)?\.ts$|GameView\.ts$|GameUpdates\.ts$|UserSettings\.ts$"
    r"|Colors\.ts$|(Pastel|Color)Theme\w*\.ts$|Env\.ts$|GameMapLoader\.ts$|MapLoader\.ts$|Translat"
)


def git(upstream, *args):
    r = subprocess.run(["git", "-C", str(upstream), *args], capture_output=True, text=True, errors="replace")
    if r.returncode not in (0, 1):
        sys.exit(f"git {' '.join(args[:2])} failed: {r.stderr.strip()}")
    return r.stdout


def class_info(toml, cls):
    """(parent, builds) of a class from the TOML text."""
    m = re.search(rf'^\[class\.{cls}\]\n(.*?)(?=^\[|\Z)', toml, re.S | re.M)
    if not m:
        sys.exit(f"unknown class {cls}")
    body = m.group(1)
    parent = re.search(r'^parent = "(\w*)"', body, re.M).group(1)
    builds = re.findall(r'"([0-9a-f]{12})"', re.search(r"^builds = \[(.*?)\]", body, re.M).group(1))
    return parent, builds


def edge_flags(cls):
    """Change flags already in the EDGES entry of the class: (add, remove) or None."""
    m = re.search(r'class:\s*"%s",\s*add:\s*&\[(.*?)\],\s*remove:\s*&\[(.*?)\]' % cls, RULES.read_text(), re.S)
    if not m:
        return None
    pick = lambda s: re.findall(r"Change::(\w+)", s)
    return pick(m.group(1)), pick(m.group(2))


def rust_index():
    """(port index, stem index): upstream path -> Rust files citing it in a `//! Port of` header; stem -> Rust files named like it."""
    idx, stems = {}, {}
    for p in (ROOT / "Crates").rglob("*.rs"):
        rel = str(p.relative_to(ROOT))
        stems.setdefault(p.stem, []).append(rel)
        try:
            with open(p, encoding="utf-8") as f:
                head = [f.readline() for _ in range(5)]
        except OSError:
            continue
        for ln in head:
            if ln.startswith("//! Port of"):
                for m in re.finditer(r"(?:src/core|packages/[\w-]+/src)/(\S+?\.ts)", ln):
                    if rel not in idx.setdefault(m.group(1), []):
                        idx[m.group(1)].append(rel)
    return idx, stems


def enclosing(blob_lines, line):
    """Class.method (or best available) containing a 1-based line."""
    cls = meth = None
    for i in range(min(line, len(blob_lines)) - 1, -1, -1):
        s = blob_lines[i]
        if meth is None:
            m = METHOD_RE.match(s)
            if m and m.group(1) not in ("if", "for", "while", "switch", "catch", "return"):
                meth = m.group(1)
        c = CLASS_RE.match(s)
        if c:
            cls = c.group(1)
            break
    return f"{cls}.{meth}" if meth and cls else meth or cls or "(top level)"


def main():
    ap = argparse.ArgumentParser(description="Upstream digest of a build class versus its parent class.")
    ap.add_argument("cls", metavar="class", help="class id, for example C064")
    ap.add_argument("--upstream", default=os.environ.get("OFSIM_UPSTREAM", str(CACHE / "OpenFrontIO")))
    ap.add_argument("--files-only", action="store_true", help="file list with +/- counts and Rust mapping, no hunks")
    ap.add_argument("--max-lines", type=int, default=400, help="hunk-line budget (default 400): a file gets a share proportional to its size, at least 4, at most N")
    ap.add_argument("--verbose", action="store_true", help="full noise (imports, comments, braces), full paths of excluded files, PR titles, per-file budget N")
    a = ap.parse_args()
    toml = TOML.read_text()
    parent, builds = class_info(toml, a.cls)
    if not parent:
        sys.exit(f"{a.cls} is the root class, nothing to diff")
    new = builds[0]
    old = class_info(toml, parent)[1][0]
    up = a.upstream
    roots = ["--", *ROOTS]

    log = git(up, "log", "--no-merges", "--format=%s", f"{old}..{new}", *roots[:0]).splitlines()
    if not log:
        log = git(up, "log", "--no-merges", "--format=%s", f"{new}..{old}").splitlines()
    prs, seen = [], set()
    for s in log:
        for n in re.findall(r"\(#(\d+)\)", s):
            if n not in seen:
                seen.add(n)
                prs.append((n, s))
    edge = edge_flags(a.cls)
    flags = "none (class not in EDGES)" if edge is None else f"+{','.join(edge[0]) or '-'} -{','.join(edge[1]) or '-'}"
    print(f"{a.cls} {new} <- {parent} {old} | commits {len(log)} | PRs {' '.join('#' + n for n, _ in prs[:40]) or '-'}{f' +{len(prs) - 40} more (--verbose)' if len(prs) > 40 and not a.verbose else ''}")
    print(f"flags in EDGES: {flags}")
    if a.verbose:
        for n, s in prs:
            print(f"  #{n} {s[:100]}")

    stat = git(up, "diff", "-w", "-M", "--numstat", "-z", old, new, *roots).split("\0")
    files = []  # (old_path, new_path, add, del)
    i = 0
    while i < len(stat) and stat[i]:
        add, dele, rest = stat[i].split("\t", 2)
        if rest == "":
            files.append((stat[i + 1], stat[i + 2], add, dele))
            i += 3
        else:
            files.append((rest, rest, add, dele))
            i += 1
    idx, stems = rust_index()
    kept, excluded = [], []
    for o, n, ad, de in files:
        key = STRIP_RE.sub("", n if n != "/dev/null" else o)
        (excluded if EXCLUDE_RE.search(key) else kept).append((o, n, ad, de, key))
    kept.sort(key=lambda t: -(int(t[2]) + int(t[3]) if t[2].isdigit() else 0))
    print(f"files: {len(kept)} kept, {len(excluded)} excluded")

    total = sum(int(t[2]) + int(t[3]) for t in kept if t[2].isdigit()) or 1
    for o, n, ad, de, key in kept:
        size = int(ad) + int(de) if ad.isdigit() else 0
        budget = a.max_lines if a.verbose else max(4, min(a.max_lines, a.max_lines * size // total))
        okey = STRIP_RE.sub("", o)
        rs = idx.get(key) or idx.get(okey)
        if rs:
            port = ", ".join(r.replace("Crates/Engine/src/", "E:") for r in rs)
        else:
            stem = Path(key).stem
            near = [f for k, v in stems.items() if k == stem or re.fullmatch(re.escape(stem) + r"V\d+\w*", k) for f in v]
            port = ("~ " + ", ".join(r.replace("Crates/Engine/src/", "E:") for r in near)) if near else "(no Rust port found)"
        tag = f" (renamed from {okey})" if o != n else ""
        print(f"== {key} +{ad}/-{de}{tag} -> {port}")
        if a.files_only:
            continue
        diff = git(up, "diff", "-w", "-U0", "-M", old, new, "--", o, n).splitlines()
        blob_new = git(up, "show", f"{new}:{n}").splitlines() if ad != "0" or de == "0" else []
        blob_old = git(up, "show", f"{old}:{o}").splitlines() if not blob_new else []
        shown, cur, truncated = 0, None, False
        for ln in diff:
            m = re.match(r"@@ -(\d+)(?:,\d+)? \+(\d+)(?:,(\d+))? @@", ln)
            if m:
                where = enclosing(blob_new, int(m.group(2))) if blob_new else enclosing(blob_old, int(m.group(1)))
                if where != cur:
                    cur = where
                    if shown < budget:
                        print(f"  [{where}]")
                        shown += 1
                continue
            if ln[:1] in "+-" and not ln.startswith(("+++", "---")):
                body = ln[1:].strip()
                if not a.verbose and (not body or body.startswith(("//", "/*", "*", "import ", "} from", "}", ")", "{"))):
                    continue
                if shown < budget:
                    print(f"   {ln[:160]}")
                    shown += 1
                elif not truncated:
                    truncated = True
        if truncated:
            print(f"  ... truncated at {budget} lines (--max-lines is the total budget, shared by size; --verbose gives each file all of it)")
    if excluded:
        names = [STRIP_RE.sub("", n if n != "/dev/null" else o) for o, n, *_ in excluded]
        if a.verbose:
            print(f"excluded ({len(excluded)}): {', '.join(names)}")
        else:
            print(f"excluded ({len(excluded)}): {', '.join(x.rsplit('/', 1)[-1] for x in names)}")


if __name__ == "__main__":
    main()
