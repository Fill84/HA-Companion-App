"""Verify the latest recorder state of an existing Home Assistant entity."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from pathlib import Path
import sqlite3


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("entity_id")
    parser.add_argument("expected_state")
    args = parser.parse_args()

    with sqlite3.connect(f"file:{args.database}?mode=ro", uri=True, timeout=5) as database:
        row = database.execute(
            """
            SELECT s.state, s.last_updated_ts
            FROM states AS s
            JOIN states_meta AS m ON m.metadata_id = s.metadata_id
            WHERE m.entity_id = ?
            ORDER BY s.state_id DESC
            LIMIT 1
            """,
            (args.entity_id,),
        ).fetchone()

    if row is None:
        print(f"{args.entity_id}: no recorder state")
        return 1
    state, timestamp = row
    updated = datetime.fromtimestamp(timestamp, timezone.utc).isoformat() if timestamp else "?"
    print(f"{args.entity_id}: {state} at {updated}")
    return int(state != args.expected_state)


if __name__ == "__main__":
    raise SystemExit(main())
