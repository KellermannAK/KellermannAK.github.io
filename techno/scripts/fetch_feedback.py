#!/usr/bin/env python3
"""Fetch listening feedback from Supabase and print it in the site's export format,
ready for `techno analyze --input -`.

Needs read access to all feedback, i.e. the secret service_role key. Provide it as the
environment variable SUPABASE_SERVICE_KEY, or let a credential proxy inject the `apikey`
header (then the variable can stay unset). Never commit the key.

    scripts/fetch_feedback.py [--since 2026-11-25] | target/release/techno analyze --input -
"""
import argparse
import json
import os
import sys
import urllib.parse
import urllib.request

URL = os.environ.get("SUPABASE_URL", "https://djycjopxdbkofrgrbwhx.supabase.co")
ICON = {"up": "👍", "down": "👎", "idea": "💡"}

ap = argparse.ArgumentParser()
ap.add_argument("--since", help="only feedback created after this date/time (ISO)")
args = ap.parse_args()

query = {"select": "*", "order": "created_at.asc"}
if args.since:
    query["created_at"] = f"gt.{args.since}"
key = os.environ.get("SUPABASE_SERVICE_KEY", "injected-by-proxy")
req = urllib.request.Request(
    f"{URL}/rest/v1/feedback?{urllib.parse.urlencode(query)}",
    headers={"apikey": key, "Accept": "application/json"},
)
try:
    with urllib.request.urlopen(req, timeout=60) as r:
        rows = json.load(r)
except Exception as e:  # noqa: BLE001
    sys.exit(f"fetching feedback failed: {e}")

print(f"Techno-Feedback ({len(rows)}) – from Supabase")
print()
for i, row in enumerate(rows, 1):
    build = f" · Build {row['build']}" if row.get("build") else ""
    user = f" · user {row['user_id'][:8]}" if row.get("user_id") else ""
    print(f"{i}. {ICON.get(row['rating'], '?')} {row['where']}{build}{user}")
    if row.get("comment"):
        print(f"   „{row['comment']}“")
