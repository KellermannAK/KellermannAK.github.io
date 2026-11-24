#!/usr/bin/env bash
# Build the static preview site: DJ mixes (with tracklists) and single songs as MP3.
# Usage: scripts/build_site.sh OUT_DIR SEED [SEED...]
#   MIXES="7:5 21:4"  set seed:number of songs for each mix (default "7:5")
# Run from techno/ after `cargo build --release` and
#   `cargo build --release -p techno-web --target wasm32-unknown-unknown`.
set -euo pipefail
out=$1
shift
bin=target/release/techno
mixes=${MIXES:-"7:5"}
mkdir -p "$out/audio"
cp -r site/index.html site/manifest.webmanifest site/sw.js site/app site/icons "$out/"
cp target/wasm32-unknown-unknown/release/techno_web.wasm "$out/app/techno.wasm"

mp3() { # wav -> mp3, delete wav
  ffmpeg -loglevel error -y -i "$1" -codec:a libmp3lame -b:a "$2" -metadata title="$3" "${1%.wav}.mp3"
  rm "$1"
}

mix_json=""
for m in $mixes; do
  seed=${m%%:*}
  songs=${m##*:}
  "$bin" mix --seed "$seed" --songs "$songs" --out "$out/audio/mix-$seed.wav" \
    --tracklist "$out/audio/mix-$seed.json" | tee "$out/audio/mix-$seed.txt"
  mp3 "$out/audio/mix-$seed.wav" 160k "techno mix $seed"
  mix_json+="${mix_json:+,}$seed"
done

seeds_json=""
for seed in "$@"; do
  "$bin" render --seed "$seed" --out "$out/audio/seed-$seed.wav" > /dev/null
  mp3 "$out/audio/seed-$seed.wav" 192k "techno seed $seed"
  "$bin" info --seed "$seed" > "$out/audio/seed-$seed.txt"
  seeds_json+="${seeds_json:+,}$seed"
done

commit=${GITHUB_SHA:-$(git rev-parse HEAD 2>/dev/null || echo unknown)}
built=$(date -u +%Y-%m-%dT%H:%MZ)
printf '{"mixes":[%s],"seeds":[%s],"commit":"%s","built":"%s"}\n' \
  "$mix_json" "$seeds_json" "$commit" "$built" > "$out/songs.json"
echo "site written to $out"
