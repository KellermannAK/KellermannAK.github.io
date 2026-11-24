#!/usr/bin/env bash
# Build the static preview site: render songs for the given seeds as MP3 plus their DNA.
# Usage: scripts/build_site.sh OUT_DIR SEED [SEED...]   (run from techno/, after cargo build --release)
set -euo pipefail
out=$1
shift
bin=target/release/techno
mkdir -p "$out/audio"
cp site/index.html "$out/index.html"

seeds_json=""
for seed in "$@"; do
  "$bin" render --seed "$seed" --out "$out/audio/seed-$seed.wav" > "$out/audio/seed-$seed.render.txt"
  ffmpeg -loglevel error -y -i "$out/audio/seed-$seed.wav" -codec:a libmp3lame -b:a 192k \
    -metadata title="techno seed $seed" "$out/audio/seed-$seed.mp3"
  rm "$out/audio/seed-$seed.wav"
  "$bin" info --seed "$seed" > "$out/audio/seed-$seed.txt"
  seeds_json+="${seeds_json:+,}$seed"
done

commit=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}
built=$(date -u +%Y-%m-%dT%H:%MZ)
printf '{"seeds":[%s],"commit":"%s","built":"%s"}\n' "$seeds_json" "$commit" "$built" > "$out/songs.json"
echo "site written to $out"
