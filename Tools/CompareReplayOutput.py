#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
"""Compares two replay output directories (upstream ReplayDumper.ts vs `ofsim replay`).

Usage: python3 Tools/CompareReplayOutput.py <reference dir> <candidate dir>

Checks Result.json (byte-equal), Entities.ndjson (byte-equal, first differing line and field
reported) and Tiles/*.u16.zst (decompressed bytes equal; the zstd frames themselves may differ
between zstd versions). Exit code 0 when everything matches. Needs Python 3.14+, the `zstd` CLI or Node 22.15+ to decompress.
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path


def decompress(path: Path) -> bytes:
    """Returns the content of a zstd file (Python 3.14 zstd, the `zstd` CLI, or Node 22.15+)."""
    try:
        from compression import zstd  # type: ignore[import-not-found]  # Python 3.14+

        return zstd.decompress(path.read_bytes())
    except ImportError:
        pass
    if shutil.which("zstd"):
        return subprocess.run(["zstd", "-dc", str(path)], check=True, capture_output=True).stdout
    script = "process.stdout.write(require('zlib').zstdDecompressSync(require('fs').readFileSync(process.argv[1])))"
    return subprocess.run(["node", "-e", script, str(path)], check=True, capture_output=True).stdout


def first_diff(a: object, b: object, where: str = "") -> str | None:
    """Path of the first difference between two JSON values."""
    if type(a) is not type(b):
        return f"{where}: {a!r} != {b!r}"
    if isinstance(a, dict) and isinstance(b, dict):
        if list(a) != list(b):
            return f"{where}: keys {list(a)} != {list(b)}"
        for k in a:
            d = first_diff(a[k], b[k], f"{where}.{k}")
            if d:
                return d
    elif isinstance(a, list) and isinstance(b, list):
        for i, (x, y) in enumerate(zip(a, b)):
            d = first_diff(x, y, f"{where}[{i}]")
            if d:
                return d
        if len(a) != len(b):
            return f"{where}: length {len(a)} != {len(b)}"
    elif a != b:
        return f"{where}: {a!r} != {b!r}"
    return None


def compare(ref: Path, cand: Path) -> list[str]:
    """Returns the list of problems (empty when the outputs match)."""
    problems: list[str] = []
    if (ref / "Result.json").read_bytes() != (cand / "Result.json").read_bytes():
        r = json.loads((ref / "Result.json").read_text())
        c = json.loads((cand / "Result.json").read_text())
        problems.append(f"Result.json differs: {first_diff(r, c)}")
    ref_lines = (ref / "Entities.ndjson").read_bytes().split(b"\n")
    cand_lines = (cand / "Entities.ndjson").read_bytes().split(b"\n")
    if ref_lines != cand_lines:
        for i, (x, y) in enumerate(zip(ref_lines, cand_lines)):
            if x != y:
                tick = json.loads(x)["tick"] if x else None
                problems.append(
                    f"Entities.ndjson line {i} (tick {tick}): {first_diff(json.loads(x), json.loads(y))}"
                )
                break
        else:
            problems.append(f"Entities.ndjson line count {len(ref_lines)} != {len(cand_lines)}")
    ref_tiles = {p.name: p for p in (ref / "Tiles").glob("T*.u16.zst")}
    cand_tiles = {p.name: p for p in (cand / "Tiles").glob("T*.u16.zst")}
    if ref_tiles.keys() != cand_tiles.keys():
        problems.append(f"tile file sets differ: {sorted(ref_tiles.keys() ^ cand_tiles.keys())[:5]}")
    frames_equal = 0
    for name in sorted(ref_tiles.keys() & cand_tiles.keys()):
        if ref_tiles[name].read_bytes() == cand_tiles[name].read_bytes():
            frames_equal += 1
            continue
        a, b = decompress(ref_tiles[name]), decompress(cand_tiles[name])
        if a != b:
            n = next((i for i in range(min(len(a), len(b))) if a[i] != b[i]), min(len(a), len(b)))
            problems.append(f"{name}: tile bytes differ at u16 index {n // 2} ({a[n:n+2].hex()} vs {b[n:n+2].hex()})")
            break
    print(f"tiles: {len(ref_tiles)} reference, {len(cand_tiles)} candidate, {frames_equal} byte-identical frames")
    return problems


def main() -> int:
    """Entry point."""
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    problems = compare(Path(sys.argv[1]), Path(sys.argv[2]))
    for p in problems:
        print("DIFF", p)
    print("MATCH" if not problems else "MISMATCH")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
