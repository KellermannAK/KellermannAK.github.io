#!/usr/bin/env bash
# Build the static site: live player plus a rolling archive of DJ mixes per genre.
#
# Every build renders ONE new mix per genre (MIX_SONGS songs, default 10). Its seed comes
# from the commit, so every push brings new songs. The previous mixes are taken over from
# the live site (PREV_URL) so each genre's archive grows up to KEEP mixes (newest first);
# each mix remembers the build it was rendered with, so feedback on old mixes is analysed
# with the right engine.
#
# Usage: scripts/build_site.sh OUT_DIR
#   GENRES     genres to render (default "techno house")
#   MIX_SEED   seed of the new techno mix (default: derived from the commit)
#   MIX_SONGS  songs per new mix (default 10)
#   KEEP       mixes kept per genre incl. the new one (default 4, ~55 MB per mix)
#   PREV_URL   live site to take previous mixes from ("" = none)
# Run from techno/ after `cargo build --release` and
#   `cargo build --release -p techno-web --target wasm32-unknown-unknown`.
set -euo pipefail
out=$1
bin=target/release/techno
commit=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo 0000000000000)}
commit=$(printf '%-12s' "$commit" | tr ' ' 0)
genres=${GENRES:-techno house}
songs=${MIX_SONGS:-10}
keep=${KEEP:-4}
# Scheduled rebuilds run on an unchanged commit: SEED_SALT (e.g. the run number) makes the
# seeds, and so the new mixes, differ from the previous build. Unset = derived from the commit only.
salt=${SEED_SALT:-0}
prev_url=${PREV_URL-https://kellermannak.github.io}
built=$(date -u +%Y-%m-%dT%H:%MZ)
version=$("$bin" version)

mkdir -p "$out/audio"
cp -r site/index.html site/manifest.webmanifest site/sw.js site/app site/icons "$out/"
cp target/wasm32-unknown-unknown/release/techno_web.wasm "$out/app/techno.wasm"

# Techno keeps its original file names (mix-SEED.*); other genres get a prefix.
prefix() { if [ "$1" = techno ]; then echo "mix"; else echo "$1-mix"; fi; }

new=""
for g in $genres; do
  if [ "$g" = techno ]; then
    seed=${MIX_SEED:-$(( (16#${commit:0:6} + salt) % 1000000 ))}
  else
    seed=$(( (16#${commit:6:6} + ${#g} * 7919 + salt * 31) % 1000000 ))
  fi
  f="$out/audio/$(prefix "$g")-$seed"
  echo "rendering $g mix $seed with $songs songs (build ${commit:0:7})"
  "$bin" mix --genre "$g" --seed "$seed" --songs "$songs" --out "$f.wav" --tracklist "$f.json" | tee "$f.txt"
  ffmpeg -loglevel error -y -i "$f.wav" -codec:a libmp3lame -b:a 128k \
    -metadata title="$g mix $seed" -metadata genre="$g" "$f.mp3"
  rm "$f.wav"
  new="$new $g:$seed"
done

# Index of mixes: the new ones first, then the previous ones from the live site.
python3 - "$out" "$new" "$songs" "$commit" "$built" "$keep" "$prev_url" "$version" <<'PY'
import json, sys, urllib.request

out, new, songs, commit, built, keep, prev_url, version = sys.argv[1:9]
keep = int(keep)
prefix = lambda g: "mix" if g == "techno" else f"{g}-mix"
mixes = []
count = {}
for item in new.split():
    g, seed = item.split(":")
    mixes.append({"seed": int(seed), "genre": g, "songs": int(songs), "commit": commit, "built": built})
    count[g] = 1

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
        if not isinstance(m, dict):
            continue
        g = m.get("genre", "techno")  # mixes from before the genre switcher are techno
        if g not in count or any(x["seed"] == m["seed"] and x["genre"] == g for x in mixes):
            continue
        if count[g] >= keep:
            continue
        try:
            for ext in ("mp3", "json", "txt"):
                name = f"{prefix(g)}-{m['seed']}.{ext}"
                data = fetch(f"{prev_url}/audio/{name}")
                with open(f"{out}/audio/{name}", "wb") as f:
                    f.write(data)
            mixes.append({**m, "genre": g})
            count[g] += 1
            print(f"kept {g} mix {m['seed']} (build {m.get('commit', '?')[:7]})")
        except Exception as e:
            print(f"could not take over {g} mix {m.get('seed')}: {e}")

index = {"mixes": mixes, "seeds": [], "commit": commit, "built": built, "version": json.loads(version)}
with open(f"{out}/songs.json", "w") as f:
    json.dump(index, f)
print(f"site written to {out}: " + ", ".join(f"{g} {n}" for g, n in count.items()) + " mixes")
PY
