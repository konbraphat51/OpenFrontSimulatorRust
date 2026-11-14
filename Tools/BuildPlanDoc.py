#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Stage 3 of the P6 build plan: render Docs/BuildPlan.md and Docs/BuildPlan.ja.md.

Responsibility: turn the JSON written by BuildPlanGen.py (plus BuildClassify.py output for
the PRNG/DetMath era of each class) into the bilingual class tables; the prose around the
tables lives in Tools/BuildPlanProse.py.

Usage: BuildPlanDoc.py --upstream Cache/OpenFrontIO --builds builds.json --data md.json --out-dir Docs
"""
import argparse
import importlib.util
import json
import os
import subprocess

CAT_JA = {
    "PRNG switch": "PRNG切替",
    "DetMath / math": "DetMath/数学関数",
    "pathfinding change": "経路探索変更",
    "AI behaviour change": "AI挙動変更",
    "constant change (Config)": "定数変更(Config)",
    "formula change (Config)": "式変更(Config)",
    "data table": "データ表",
    "new execution": "新規Execution",
    "removed execution": "Execution削除",
    "constant change": "定数変更",
    "formula / logic change": "式/ロジック変更",
}
SHORT = {"new file": "new", "removed file": "removed", "edit": ""}


def git(up, *a):
    r = subprocess.run(["git", "-C", up, *a], capture_output=True, text=True, errors="replace")
    return r.stdout if r.returncode == 0 else ""


def era(up, b):
    full = b["full"]
    has_det = "DetMath.ts" in b["files"]
    prng = ""
    for path in ("src/core/PseudoRandom.ts", "packages/engine-lib/src/PseudoRandom.ts"):
        prng = git(up, "show", f"{full}:{path}")
        if prng:
            break
    if "seedrandom" in prng:
        return "S"
    return "D" if has_det else "F"


def summary(c, ja):
    ch = sorted(c["changes"], key=lambda x: -(x["added"] + x["deleted"]))
    if not ch:
        return "baseline" if not ja else "基準"
    cats = []
    for x in ch:
        k = CAT_JA.get(x["category"], x["category"]) if ja else x["category"]
        if k not in cats:
            cats.append(k)
    parts = []
    for x in ch[:3]:
        fn = ", ".join(x["functions"][:3])
        tag = SHORT[x["kind"]]
        parts.append(f"`{x['file']}`{' ' + tag if tag else ''} (+{x['added']}/-{x['deleted']}{'; ' + fn if fn else ''})")
    more = len(ch) - 3
    head = f"{len(ch)} files; " if not ja else f"{len(ch)}ファイル; "
    s = head + ", ".join(cats[:4]) + ". " + "; ".join(parts)
    if more > 0:
        s += f"; +{more} more" if not ja else f"; 他{more}"
    if c["prs"]:
        pr = ", ".join(f"#{p}" for p in c["prs"][:6])
        s += f". PRs: {pr}" + (f" (+{len(c['prs']) - 6})" if len(c["prs"]) > 6 else "")
    return s


def table(up, builds, data, ja, status):
    cl = data["classes"]
    tot = sum(c["weight"] for c in cl)
    hdr = (["Class", "Builds (commit12 tag)", "Weight (day-eq / public recs)", "Cum.%", "Parent", "Era", "Change summary", "Diff lines / est. Rust lines", "Risk", "Map files changed", "Status"]
           if not ja else ["クラス", "ビルド (commit12 タグ)", "重み (日換算 / Public件数)", "累積%", "親", "世代", "変更概要", "差分行 / 推定Rust行", "リスク", "変更マップファイル", "状態"])
    rows = ["| " + " | ".join(hdr) + " |", "|" + "---|" * len(hdr)]
    cum = 0.0
    for c in cl:
        cum += c["weight"]
        bl = "<br>".join(f"`{b}` {','.join(t) if t else '-'}" for b, t in c["builds"])
        rep = builds[c["rep"]] if c["rep"] in builds else builds[c["builds"][0][0]]
        mc = c.get("map_changes", {})
        mtxt = ", ".join(f"{m}({'+'.join(x.replace('.json','').replace('.bin','') for x in f)})" for m, f in list(mc.items())[:4])
        if len(mc) > 4:
            mtxt += f" +{len(mc) - 4}"
        rows.append("| " + " | ".join([
            c["id"], bl, f"{c['weight']} / {c['public']}", f"{100 * cum / tot:.0f}", c["parent"] or "root", data["era"][c["id"]],
            summary(c, ja), f"{c['lines']} / {c['size_est']}" + (f" ({c['rewrites']} rewrite)" if c["rewrites"] else ""),
            c["risk"], mtxt or "-", status.get(c["id"], "-")]) + " |")
    return "\n".join(rows)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--upstream", required=True)
    ap.add_argument("--builds", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--out-dir", required=True)
    a = ap.parse_args()
    builds = json.load(open(a.builds))
    data = json.load(open(a.data))
    data["era"] = {c["id"]: era(a.upstream, builds[c["rep"]]) for c in data["classes"]}
    spec = importlib.util.spec_from_file_location("prose", os.path.join(os.path.dirname(__file__), "BuildPlanProse.py"))
    prose = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(prose)
    status_path = os.path.join(a.out_dir, "BuildStatus.json")  # hand-maintained: {"en": {class: text}, "ja": {class: text}}
    status = json.load(open(status_path)) if os.path.exists(status_path) else {}
    for ja, name in ((False, "BuildPlan.md"), (True, "BuildPlan.ja.md")):
        text = prose.render(ja, data, builds, table(a.upstream, builds, data, ja, status.get("ja" if ja else "en", {})))
        open(os.path.join(a.out_dir, name), "w").write(text)


if __name__ == "__main__":
    main()
