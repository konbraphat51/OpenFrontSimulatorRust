"""Downloads public OpenFront game records into Cache/Records/<commit12>/<gameId>.json.gz.

Stdlib only. HTTP goes through curl (Python urllib is blocked by some proxies); replace `http_get`
to plug another transport. Requests are spaced by at least 0.5 s.

Hold-out guard: games whose start falls on a day in --exclude-day (default 2026-10-02 UTC, the
confirmatory hold-out of the parent research project) are never downloaded.

Usage:
  python Tools/FetchRecords.py --start 2026-10-03T00:00:00Z --end 2026-10-03T00:29:59Z \
      --type Public --mode "Free For All" --commit 4e837520e886 --max 5 [--out Cache/Records]
"""

import argparse
import gzip
import json
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Callable

BASE_URL = "https://api.openfront.io"
DEFAULT_EXCLUDE_DAYS = ["2026-10-02"]
MIN_SPACING_SECONDS = 0.5

HttpGet = Callable[[str], bytes]


def http_get(url: str) -> bytes:
    """Fetches a URL with curl and returns the body; raises on HTTP errors."""
    result = subprocess.run(
        ["curl", "-sSf", "--max-time", "60", "-A", "OpenFrontSimulator-research/0.1", url],
        capture_output=True,
        check=True,
    )
    return result.stdout


def start_day(start: object) -> str:
    """UTC day (YYYY-MM-DD) of a listing `start` value (epoch milliseconds or ISO 8601)."""
    if isinstance(start, (int, float)):
        return datetime.fromtimestamp(start / 1000, tz=timezone.utc).strftime("%Y-%m-%d")
    text = str(start).replace("Z", "+00:00")
    parsed = datetime.fromisoformat(text)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc).strftime("%Y-%m-%d")


def is_excluded(start: object, exclude_days: list[str]) -> bool:
    """True when the game start is on an excluded UTC day."""
    return start_day(start) in exclude_days


class Fetcher:
    """Rate-limited API access with the hold-out guard."""

    def __init__(self, get: HttpGet, exclude_days: list[str], spacing: float = MIN_SPACING_SECONDS):
        self.get = get
        self.exclude_days = exclude_days
        self.spacing = max(spacing, MIN_SPACING_SECONDS)
        self._last = 0.0

    def _spaced_get(self, url: str) -> bytes:
        wait = self._last + self.spacing - time.monotonic()
        if wait > 0:
            time.sleep(wait)
        try:
            return self.get(url)
        finally:
            self._last = time.monotonic()

    def list_games(self, start: str, end: str, limit: int, game_type: str | None, mode: str | None) -> list[dict]:
        """Lists game summaries of one window, minus excluded days."""
        url = f"{BASE_URL}/public/games?start={start}&end={end}&limit={limit}"
        games = json.loads(self._spaced_get(url))
        if game_type:
            games = [g for g in games if g.get("type") == game_type]
        if mode:
            games = [g for g in games if g.get("mode") == mode]
        kept = [g for g in games if not is_excluded(g["start"], self.exclude_days)]
        if len(kept) != len(games):
            print(f"skipped {len(games) - len(kept)} games on excluded days", file=sys.stderr)
        return kept

    def fetch_record(self, summary: dict) -> bytes:
        """Downloads one record; refuses excluded days (checked again on the summary here)."""
        if is_excluded(summary["start"], self.exclude_days):
            raise PermissionError(f"game {summary['game']} starts on an excluded day")
        return self._spaced_get(f"{BASE_URL}/public/game/{summary['game']}")


def save_record(raw: bytes, out_dir: Path, commit_prefix: str | None) -> Path | None:
    """Stores a record under its commit directory; returns None when the commit filter rejects it."""
    record = json.loads(raw)
    commit = str(record.get("gitCommit", ""))[:12]
    if commit_prefix and not commit.startswith(commit_prefix):
        return None
    target = out_dir / commit
    target.mkdir(parents=True, exist_ok=True)
    path = target / f"{record['info']['gameID']}.json.gz"
    path.write_bytes(gzip.compress(raw))
    return path


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--start", required=True, help="window start, ISO 8601 UTC")
    parser.add_argument("--end", required=True, help="window end, ISO 8601 UTC")
    parser.add_argument("--limit", type=int, default=200)
    parser.add_argument("--type", help="game type filter: Public, Private, Singleplayer")
    parser.add_argument("--mode", help='game mode filter: "Free For All" or "Team"')
    parser.add_argument("--commit", help="keep only records whose gitCommit starts with this prefix")
    parser.add_argument("--max", type=int, default=10, help="maximum records to keep")
    parser.add_argument("--out", default="Cache/Records")
    parser.add_argument("--spacing", type=float, default=MIN_SPACING_SECONDS, help="seconds between requests (min 0.5)")
    parser.add_argument("--exclude-day", action="append", default=None, help="UTC day never fetched; repeatable (default 2026-10-02)")
    args = parser.parse_args(argv)
    exclude = args.exclude_day if args.exclude_day is not None else DEFAULT_EXCLUDE_DAYS
    fetcher = Fetcher(http_get, exclude, args.spacing)
    out_dir = Path(args.out)
    kept = 0
    for summary in fetcher.list_games(args.start, args.end, args.limit, args.type, args.mode):
        if kept >= args.max:
            break
        existing = list(out_dir.glob(f"*/{summary['game']}.json.gz"))
        if existing:
            print(f"have {existing[0]}")
            kept += 1
            continue
        saved = save_record(fetcher.fetch_record(summary), out_dir, args.commit)
        if saved is not None:
            print(f"saved {saved}")
            kept += 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
