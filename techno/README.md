# Algorithmic Techno Generator

Endless electronic music, generated and synthesized live. No samples, no vocals. Every song
comes from a seed and can be reproduced exactly. The music is not AI-generated: it comes from
hand-written rules, random seeds and synthesis code; only the code itself was written with AI. Two genres, each with its own rule tree:

- **Techno** (`profiles/hard_techno_simple.toml`): hard, pounding, predictable, 134-136 BPM.
- **House** (`profiles/house.toml`): softer and slower (120-124 BPM, swing), mostly dorian /
  minor, rootless m9 / maj9 and seventh chord loops, bongos, organ bass, formant "vox" stabs, piano (modal synthesis), guitar (Karplus-Strong
  string model), electronic glide leads, strings, a round bass; breakdowns flow back into
  the groove, drops are rare and soft. Own word library for titles and artists
  (`profiles/words_house.toml`).

The site switches genres with the arrows under the header (the page turns like a cube; every
genre has its own colours). Feedback and listening statistics store the genre.

Requirements: [Claude Docs page "Requirements: Algorithmischer Techno-Generator"](https://claude.ai/code/artifact/a30876a6-5dab-457a-b661-d056b7b7c405).

## Status

| # | Milestone | State |
|---|-----------|-------|
| 1 | Engine skeleton: clock, kick + hi-hat, offline WAV render | done |
| 2 | All drum tracks, euclidean patterns, mixer, sidechain | done |
| 3 | Song DNA with bass and synths, scale and Markov rules | done |
| 4 | Song structure with rise / drop / breakdown and energy | done |
| 5 | Web version (WASM, AudioWorklet), runs on phones | done |
| 6 | Two decks, transitions, Camelot key choice, endless set | done |
| 7 | Song logs, replay, WAV export | |

Extra: sound variety (kick styles, backbeats, lead kinds, arp, drone, per-song palette).

## Quick start

```sh
cargo build --release
./target/release/techno mix    --seed 7 --songs 5 --out mix.wav --tracklist mix.json
./target/release/techno info   --seed 42                 # song DNA and structure
./target/release/techno info   --seed 42 --genre house   # same for house (also: render, mix)
./target/release/techno render --seed 42 --out 42.wav    # one song, ~5-7 min
./target/release/techno render --seed 42 --start-bar 96 --bars 8 --solo bass,lead --out dbg.wav
./target/release/techno render --seed 42 --start-bar 96 --bars 8 --solo lead --no-fx   # A/B without inserts
cargo test
PREV_URL= MIX_SONGS=2 scripts/build_site.sh /tmp/site     # site with one new mix (needs ffmpeg)
```

Listening feedback: the site has thumbs up / down buttons (live set and mixes). They capture
the exact moment (set seed, song seed, bar, section, engine version, profile, build) and open a
pre-filled GitHub issue titled `Feedback 👍/👎 ...` with repro commands.

Manual format: **"mix X, minute Y, problem"** or **"seed X, bar Y, problem"** - everything is
deterministic, so every moment is exactly reproducible.

### Web version (live, endless)

`crates/web` compiles the engine to WebAssembly with a plain C ABI (`techno_new`,
`techno_render`, `techno_status`, ...), no wasm-bindgen. `site/app/worklet.js` runs it inside
an AudioWorklet; `site/app/live.js` drives the UI (play/pause, volume, new set, set seed,
status: song, key, section, energy, next transition), keeps the screen awake (Wake Lock),
sets Media Session metadata and unmutes iOS (audio session "playback"). The page is an
installable PWA (manifest + network-first service worker).

The live set with seed N is the same as `techno mix --seed N`. Decks are recycled between
songs (`Deck::recycle`, bit-identical to a fresh deck), so no audio buffers are allocated on
the audio thread; preparing the next song costs ~0.05 ms. In V8 the engine renders at ~16x
realtime on a desktop core (`node` benchmark, `cargo run --release -p techno-web --example bench`).

```sh
cargo build --release -p techno-web --target wasm32-unknown-unknown   # -> techno_web.wasm
PREV_URL= MIX_SONGS=1 scripts/build_site.sh /tmp/site                # quick local site
```

Known limit: browsers (iOS in particular) do not reliably keep Web Audio running with the
screen locked or the page in the background; a native app would be needed for that.

### Song structure

Sections come from a grammar in the profile (`[structure]`): intro (32 bars, drums and
reduced bass only) -> groove / build -> drop / breakdown -> build -> drop ... -> outro (32 bars).
A build always leads into a drop; a breakdown leads into a build (30 %) or straight into the drop with a 4-bar run-up of its own. Breakdowns are mostly 8-16 bars. Each section has an energy
curve; tracks join when the energy passes their (per-song jittered) threshold. Every build picks its own riser
(noise sweep, gated noise, swell, rarely a pitched rise, or none), roll (snare, clap, kick or
none) and maybe a high-pass sweep. Every drop picks a style (full, bass first, stripped,
filter opening) and an entry (impact + crash, crash, dry), sometimes after a one-bar pause. Every 8-bar phrase can vary the DNA
(lead octave, thinned lead, dropped percussion) without re-inventing it.

### Instrument variants

Every instrument has 3-4 sound variants (`[variants]` in the profile; synthesis in
`crates/synth/src/instruments.rs`): clap classic / tight / wide / gritty, snare standard /
tight / noisy / metallic, hats metal / noise / dark / shaker, ride standard / bell / washy,
crash standard / dark / splash, rim click / wood block / clave, tom standard / noisy / boom,
kick hard / punch / distorted / deep, bass saw+sub / reese / square / sine pluck, rumble
standard / tight / deep wash, stab and arp saws / pulse / FM bell / organ, pad saws / hollow /
sine choir / strings, drone saws / sub / noise. Each song draws its own set; levels are
matched to variant 0 within about 1 dB (`cargo run --release -p techno-engine --example variants`).

### Song names

Every song gets a title and an artist (`crates/core/src/naming.rs`, word library in
`profiles/words.toml`, probabilities in `[naming]`). Titles combine techno words by simple
patterns, with a suffix only where it fits ("(Dub)", "(Tool)"). Artists come from a style
fingerprint (kick style, main synth / lead kind, long layer): similar songs share a small
artist pool and usually get its regular artist; 20 % get a new one-off artist, and now and
then two artists collaborate. Deterministic per seed, no memory between songs.

### DJ sets

`Set` runs two decks on one clock. The incoming song starts on a bar boundary of the running
one, at its tempo, in the same or a neighbouring Camelot key. Transitions: cut (into the new
song's first groove), fast blend (8-16 bars), slow blend (32-64 bars) and filter blend
(high-pass out, low-pass in). The low end of both decks is never mixed: it is swapped hard at
a phrase boundary.

## Layout

```
profiles/hard_techno_simple.toml   techno profile: every random choice is bounded here
profiles/house.toml                house profile (own rule tree, same engine)
profiles/words*.toml               title / artist word libraries per genre
crates/core     deterministic, audio-free: time base (ticks), PRNG, theory, euclid,
                Markov chain, profile, song DNA, song structure / energy / automation
crates/synth    DSP (PolyBLEP osc, TPT SVF, envelopes, delay, reverb, compressor,
                limiter), synthesized instruments, channel strips, master bus
crates/engine   deck: DNA -> note triggers on the musical clock -> instruments -> mix;
                set: two decks, transitions, DJ strips (fader, low kill, filters)
crates/cli      `techno` binary: render songs and mixes to WAV, DNA inspection
site/, scripts/ preview page (GitHub Pages) and its build script
```

### Visuals mode

The button next to the account circle hides everything below the genre switcher and shows
two turntables with a mixer: the records spin while their deck is audible (and slow down to a
stop on pause), artist and title run around the record label, the mixer shows both channel
faders, the low-band kills, BPM, key, section, energy and the next transition. All of it is
read from the engine status (`techno_status`, fields 17-25); the buttons in the mixer press
the controls of the live card. `?visuals=1` opens the page in this mode.

Every song gets its own record label, drawn from its seed (`app/labels.js`): artist and title
curved along the rim, straight, on a diagonal band, in two halves or minimal, in one of nine
colour schemes and six type styles, sometimes with a ring, dots, bars, a triangle or a line.
Next to the heart the mixer has the thumbs up / down and idea buttons of the live card.

The mixer also plays the archived mixes (switch LIVE / MIX at its top): the records and faders
are redrawn from the playing position and the tracklist (`app/mixvisual.js`; song i sits on
deck i % 2, the fader curves are those of the live DJ strips, the blend timing comes from
`blend_bars` / `in_bar` / `swap_after` in the mix JSON, estimated for older mixes), with
previous / next song, a position slider and a switch between the mixes of the genre.

### Melody layer (experiment, house) - currently switched off

(`probability = 0` in `profiles/house.toml` since the melodies were judged bad; the arpeggiator is
not affected. Set it back to 0.8 to try again.)

Songs can carry a real melody over 8 or 16 bars instead of an arpeggio. It is written against
the chords underneath (the long notes of the pad), with the usual rules of tonal melody:

- **Chord tones carry it.** Long notes and notes on strong beats are tones of the pad chord
  (3rd and 7th favoured, 9th as colour). Other scale tones are only short passing or neighbour
  tones, approached and left by step; a tone a semitone above a chord tone (jazz "avoid note")
  only as a quick passing 16th. Less allowed notes are drawn less often.
- **Periods.** Four bars ending on a half cadence (a chord tone that is degree 5, 2 or 7 where
  possible), four bars ending on the tonic (perfect cadence; over a chord without the tonic on
  its 3rd / root / 5th), approached by step, often from the 2nd.
- **Anchors and leaps.** Fixed anchors (first note, the two cadence notes) with random lines in
  between; now and then a leap of a fifth / sixth at the start of a half phrase, after which
  the line turns and fills the gap by step. An arch over each half phrase.
- It plays only in breakdowns (after a few bars of strings), as the only short instrument,
  with the song's own lead instrument (almost always) or a piano. Under it comes a stripped
  groove, only the bass, only the rhythm, or nothing but the strings (drawn per breakdown).
  Songs with a melody get a progression that really moves and strings underneath.

Code: `crates/core/src/melody.rs` plus the places marked `// melody:`. Switch it off with
`probability = 0` in the `[melody]` section of `profiles/house.toml` (or delete the section).

### Determinism

- All randomness goes through `core::rng::Rng` (xoshiro256**, seeded), never system time.
- Each generator uses its own forked sub-stream (`rng.fork("bass")`), so changing one
  generator does not reshuffle the others.
- Time is musical: 96 ticks per beat, 16th-step sequencing.
- Test `bit_identical_render` checks that the same seed renders bit-identical audio.

### Song DNA (generated once per song)

Key and scale (natural minor / phrygian / harmonic minor), root progression (often a drone),
bass motif (1-2 bars, never on the kick, root on strong positions), stab rhythm and chord,
lead riff (half-bar cell + variation, Markov walk, chord tones on strong steps, resolves to
root/fifth, slides and accents, nudged away from harsh intervals against the bass),
drum patterns (four-on-the-floor kick, euclidean percussion), and sound parameters for every
instrument within the profile's ranges.

### Instruments (all synthesized)

Kick (hard / punch / distorted; pitch sweep, click, saturation, tuned to the key), rumble (kick -> dark reverb ->
low-pass -> drive, deep sidechain), rolling bass, closed/open hats (808-style metal + noise,
choke), clap and/or snare backbeat, snare rolls, rim, tuned toms, ride, crash, chord stab
(delay/reverb), lead (acid with slides and accents, hoover, supersaw or FM pluck), arpeggiator,
drone, pad, riser (noise sweep + rising saw stack), impact with downlifter. Master: glue compressor, saturation,
look-ahead limiter at -0.8 dBFS.
