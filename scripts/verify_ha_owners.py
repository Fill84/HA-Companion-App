"""Report only whether Desktop App config entries have an owner, without IDs or secrets."""

import json
import sys


with open(sys.argv[1], encoding="utf-8") as source:
    entries = json.load(source)["data"]["entries"]

for entry in entries:
    if entry.get("domain") != "desktop_app" or entry.get("data", {}).get("is_hub"):
        continue
    data = entry["data"]
    print(f"{data.get('device_name', '?')}: owner_present={bool(data.get('owner_user_id'))}")
