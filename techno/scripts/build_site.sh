#!/usr/bin/env bash
# Build the static site: live player plus a rolling archive of DJ mixes.
#
# Every build renders ONE new mix (MIX_SONGS songs, default 10). Its seed comes from the
# commit, so every push brings new songs. The previous mixes are taken over from the live
# site (PREV_URL) so the archive grows up to KEEP mixes (newest first); each mix remembers
# the build it was rendered with, so feedback on old mixes is analysed with the right engine.
#
# Usage: scripts/build_site.sh OUT_DIR
#   MIX_SEED   seed of the new mix (default: derived from the commit)
#   MIX_SONGS  songs in the new mix (default 10)
#   KEEP       mixes kept in the archive incl. the new one (default 6, ~55 MB per mix)
#   PREV_URL   live site to take previous mixes from ("" = none)
# Run from techno/ after `cargo build --release` and
#   `cargo build --release -p techno-web --target wasm32-unknown-unknown`.
set -euo pipefail
out=$1
bin=target/release/techno
commit=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo 000000)}
seed=${MIX_SEED:-$((16#${commit:0:6} % 1000000))}
songs=${MIX_SONGS:-10}
keep=${KEEP:-6}
prev_url=${PREV_URL-https://kellermannak.github.io}
built=$(date -u +%Y-%m-%dT%H:%MZ)
version=$("$bin" version)

mkdir -p "$out/audio"
cp -r site/index.html site/manifest.webmanifest site/sw.js site/app site/icons "$out/"
cp target/wasm32-unknown-unknown/release/techno_web.wasm "$out/app/techno.wasm"

echo "rendering mix $seed with $songs songs (build ${commit:0:7})"
"$bin" mix --seed "$seed" --songs "$songs" --out "$out/audio/mix-$seed.wav" \
  --tracklist "$out/audio/mix-$seed.json" | tee "$out/audio/mix-$seed.txt"
ffmpeg -loglevel error -y -i "$out/audio/mix-$seed.wav" -codec:a libmp3lame -b:a 128k \
  -metadata title="techno mix $seed" "$out/audio/mix-$seed.mp3"
rm "$out/audio/mix-$seed.wav"

# Index of mixes: the new one first, then the previous ones from the live site.
python3 - "$out" "$seed" "$songs" "$commit" "$built" "$keep" "$prev_url" "$version" <<'PY'
import json, sys, urllib.request

out, seed, songs, commit, built, keep, prev_url, version = sys.argv[1:9]
keep = int(keep)
mixes = [{"seed": int(seed), "songs": int(songs), "commit": commit, "built": built}]

def fetch(url):
    with urllib.request.urlopen(url, timeout=60) as r:
        return r.read()

if prev_url:
    try:
        prev = json.loads(fetch(f"{prev_url}/songs.json"))
    except Exception as e:
        print(f"no previous site index ({e}); starting a new archive")
        prev = {}
    for m in prev.get("mixes", []):
        # Only archive entries from push builds (objects); old fixed mixes are dropped.
        if not isinstance(m, dict) or m.get("seed") == int(seed):
            continue
        if len(mixes) >= keep:
            break
        try:
            for ext in ("mp3", "json", "txt"):
                data = fetch(f"{prev_url}/audio/mix-{m['seed']}.{ext}")
                with open(f"{out}/audio/mix-{m['seed']}.{ext}", "wb") as f:
                    f.write(data)
            mixes.append(m)
            print(f"kept mix {m['seed']} (build {m.get('commit', '?')[:7]})")
        except Exception as e:
            print(f"could not take over mix {m.get('seed')}: {e}")

index = {"mixes": mixes, "seeds": [], "commit": commit, "built": built, "version": json.loads(version)}
with open(f"{out}/songs.json", "w") as f:
    json.dump(index, f)
print(f"site written to {out}: {len(mixes)} mixes")
PY
