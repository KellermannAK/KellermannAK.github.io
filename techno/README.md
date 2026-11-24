# Algorithmic Techno Generator

Endless, hard, pounding, predictable techno, generated and synthesized live. No samples,
no vocals. Every song comes from a seed and can be reproduced exactly.

Requirements: [Claude Docs page "Requirements: Algorithmischer Techno-Generator"](https://claude.ai/code/artifact/a30876a6-5dab-457a-b661-d056b7b7c405).

## Status

| # | Milestone | State |
|---|-----------|-------|
| 1 | Engine skeleton: clock, kick + hi-hat, offline WAV render | done |
| 2 | All drum tracks, euclidean patterns, mixer, sidechain | done |
| 3 | Song DNA with bass and synths, scale and Markov rules | done |
| 4 | Song structure with rise / drop / breakdown and energy | next (a fixed placeholder arrangement is used until then) |
| 5 | Web version (WASM, AudioWorklet), runs on phones | |
| 6 | Two decks, transitions, Camelot key choice, endless set | |
| 7 | Song logs, replay, WAV export | |

## Quick start

```sh
cargo build --release
./target/release/techno info   --seed 42                 # show the song DNA
./target/release/techno render --seed 42 --out 42.wav    # ~4 min song, ~15-20x realtime
./target/release/techno render --seed 42 --start-bar 80 --bars 8 --solo bass,lead --out dbg.wav
cargo test
```

Feedback format: **"seed X, bar Y, problem"** - thanks to determinism every bar is
exactly reproducible (`--start-bar` jumps straight there).

## Layout

```
profiles/hard_techno_simple.toml   style profile: every random choice is bounded here
crates/core     deterministic, audio-free: time base (ticks), PRNG, theory, euclid,
                Markov chain, profile, song DNA, placeholder arrangement
crates/synth    DSP (PolyBLEP osc, TPT SVF, envelopes, delay, reverb, compressor,
                limiter), synthesized instruments, channel strips, master bus
crates/engine   deck: DNA -> note triggers on the musical clock -> instruments -> mix
crates/cli      `techno` binary: offline render to WAV, DNA inspection
```

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

Kick (pitch sweep, click, saturation, tuned to the key), rumble (kick -> dark reverb ->
low-pass -> drive, deep sidechain), rolling bass, closed/open hats (808-style metal + noise,
choke), clap, snare (fills), rim, tuned toms, ride, crash, chord stab (delay/reverb), acid
lead (two resonant LP stages, slide, accent), pad. Master: glue compressor, saturation,
look-ahead limiter at -0.8 dBFS.
