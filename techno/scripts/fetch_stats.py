#!/usr/bin/env python3
"""Usage statistics summary from Supabase (page views, time on page, listening time).

Needs the secret service_role key like fetch_feedback.py: environment variable
SUPABASE_SERVICE_KEY, or a credential proxy that injects the `apikey` header.

    scripts/fetch_stats.py [--since 2026-11-25]
"""
import argparse
import collections
import json
import os
import statistics
import sys
import urllib.parse
import urllib.request

URL = os.environ.get("SUPABASE_URL", "https://djycjopxdbkofrgrbwhx.supabase.co")
KEY = os.environ.get("SUPABASE_SERVICE_KEY", "injected-by-proxy")

ap = argparse.ArgumentParser()
ap.add_argument("--since", help="only visits after this date/time (ISO)")
args = ap.parse_args()


def fetch(table, extra=None):
    rows, offset = [], 0
    while True:
        q = {"select": "*", "order": "started_at.asc" if table == "visits" else "first_at.asc",
             "limit": 1000, "offset": offset}
        if args.since:
            q["started_at" if table == "visits" else "first_at"] = f"gt.{args.since}"
        req = urllib.request.Request(f"{URL}/rest/v1/{table}?{urllib.parse.urlencode(q)}",
                                     headers={"apikey": KEY, "Accept": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                page = json.load(r)
        except Exception as e:  # noqa: BLE001
            sys.exit(f"fetching {table} failed: {e}")
        rows += page
        if len(page) < 1000:
            return rows
        offset += 1000


def dur(s):
    s = int(s)
    return f"{s // 3600} h {s % 3600 // 60} min" if s >= 3600 else f"{s // 60} min {s % 60} s" if s >= 60 else f"{s} s"


visits, listens = fetch("visits"), fetch("listens")
if not visits:
    sys.exit("no visits yet")
users = {v["user_id"] for v in visits}
listeners = [v for v in visits if v["listen_seconds"] > 0]
print(f"# Usage statistics ({visits[0]['started_at'][:10]} .. {visits[-1]['started_at'][:10]})\n")
print(f"page views        {len(visits)}")
print(f"distinct users    {len(users)}")
print(f"time on page      {dur(sum(v['page_seconds'] for v in visits))} total, "
      f"median {dur(statistics.median(v['page_seconds'] for v in visits))} per visit")
print(f"listening time    {dur(sum(v['listen_seconds'] for v in visits))} total")
print(f"visits with music {len(listeners)} ({100 * len(listeners) // len(visits)} %), "
      f"median {dur(statistics.median(v['listen_seconds'] for v in listeners)) if listeners else '-'}")

for title, key in (("device", "device"), ("browser", "browser")):
    c = collections.Counter(v[key] or "?" for v in visits)
    print(f"\n{title}: " + ", ".join(f"{k} {n}" for k, n in c.most_common(8)))
slow = [v for v in visits if v["wasm_speed"] is not None and v["wasm_speed"] < 4]
print(f"\nslow browsers (speed test < 4x realtime): {len(slow)} visits"
      + (f", e.g. {collections.Counter(v['browser'] for v in slow).most_common(3)}" if slow else ""))

by_song = collections.defaultdict(lambda: {"s": 0, "users": set(), "title": "", "artist": ""})
for l in listens:
    e = by_song[l["song_seed"]]
    e["s"] += l["seconds"]
    e["users"].add(l["user_id"])
    e["title"], e["artist"] = l["title"] or e["title"], l["artist"] or e["artist"]
print("\ntop songs by listening time:")
for seed, e in sorted(by_song.items(), key=lambda kv: -kv[1]["s"])[:10]:
    print(f"  {dur(e['s']):>10}  {len(e['users'])} user(s)  {e['artist']} – {e['title']}  [seed {seed}]")

per_day = collections.Counter(v["started_at"][:10] for v in visits)
print("\nvisits per day: " + ", ".join(f"{d} {n}" for d, n in sorted(per_day.items())[-10:]))
