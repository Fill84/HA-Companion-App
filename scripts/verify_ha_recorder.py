"""Read recent Home Assistant recorder states without needing an API token."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path
import sqlite3
import time


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("--fresh-seconds", type=int, default=180)
    parser.add_argument("entity_ids", nargs="+")
    args = parser.parse_args()

    connection = sqlite3.connect(f"file:{args.database}?mode=ro", uri=True, timeout=5)
    try:
        now = time.time()
        ok = True
        for entity_id in args.entity_ids:
            row = connection.execute(
                """
                SELECT s.state, s.last_updated_ts
                FROM states AS s
                JOIN states_meta AS m ON m.metadata_id = s.metadata_id
                WHERE m.entity_id = ?
                ORDER BY s.state_id DESC
                LIMIT 1
                """,
                (entity_id,),
            ).fetchone()
            if row is None:
                print(f"{entity_id}: no recorder state")
                ok = False
                continue
            state, timestamp = row
            fresh = timestamp is not None and 0 <= now - timestamp <= args.fresh_seconds
            numeric = state not in (None, "unknown", "unavailable")
            stamp = datetime.fromtimestamp(timestamp, timezone.utc).isoformat() if timestamp else "?"
            print(f"{entity_id}: {state} at {stamp} (fresh={fresh})")
            ok = ok and fresh and numeric
        return 0 if ok else 1
    finally:
        connection.close()


if __name__ == "__main__":
    raise SystemExit(main())
