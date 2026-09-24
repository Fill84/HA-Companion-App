"""Check numeric attributes of an existing Home Assistant recorder entity."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sqlite3


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("database", type=Path)
    parser.add_argument("entity_id")
    parser.add_argument("attribute_keys", nargs="+")
    args = parser.parse_args()

    with sqlite3.connect(f"file:{args.database}?mode=ro", uri=True, timeout=5) as database:
        row = database.execute(
            """
            SELECT a.shared_attrs
            FROM states AS s
            JOIN states_meta AS m ON m.metadata_id = s.metadata_id
            LEFT JOIN state_attributes AS a ON a.attributes_id = s.attributes_id
            WHERE m.entity_id = ?
            ORDER BY s.state_id DESC
            LIMIT 1
            """,
            (args.entity_id,),
        ).fetchone()

    if row is None or row[0] is None:
        print(f"{args.entity_id}: no recorder attributes")
        return 1
    attributes = json.loads(row[0])
    selected = {key: attributes.get(key) for key in args.attribute_keys}
    print(f"{args.entity_id}: {json.dumps(selected, sort_keys=True)}")
    return int(
        any(type(value) not in (int, float) for value in selected.values())
    )


if __name__ == "__main__":
    raise SystemExit(main())
