# Listening feedback log

What the listener liked or disliked, and what was changed because of it. Read this before
changing the profile or the arrangement, so earlier decisions are not undone by accident.

## Favourites

| Seed | Engine | Why it is interesting |
|------|--------|-----------------------|
| 742138 | 0.2.0 | F minor, two roots only (i-VI); hard kick tuned low (43.7 Hz); no open hat; clap + snare backbeat; rolling bass; pluck lead as the only short synth plus a triad pad; **no breakdown** - groove, build -> drop (bass first), groove, build -> drop (stripped). |

## Feedback and resulting changes

| Engine | Feedback | Change |
|--------|----------|--------|
| 0.1.0 | Layered short synth notes sound like "tinkling", not deep techno. | Only one short synth (lead / arp / stab) per bar; a second may only replace it in breakdowns; at most one long layer (pad or drone). Chords are fine. |
| 0.2.0 | Too many melodic sounds; a percussive "pinch" (seed 520584, bar 46). | Tuned toms with a big pitch sweep were the culprit: toms now only in fills, lower, smaller sweep. Rim less resonant, no delay. Arp mostly 8ths over triad / power chord. |
| 0.2.0 | Builds always use the same "fairground" riser; drops always identical; 32-bar breakdowns too long; breakdown -> build too predictable. "Don't forbid, reduce probability." | Per-section riser (noise / gated / swell / rare pitched / none), roll (snare / clap / kick / none), optional high-pass; drop styles (full / bass first / stripped / filter open) and entries; breakdowns mostly 8-16 bars, only 30 % followed by a build. |
| 0.2.0 (9e32f20) | GitHub issue #1, 12 entries (7 👍, 5 👎). Open hats 1 👍 / 5 👎; builds 0 👍 / 2 👎 ("too much at once"); drone layer 2 👍; "punch kick doesn't fit dark techno" (seed 366500: actually the hard kick, but tuned high with lots of click); "8th percussion too melodic" (ride in 8ths); "build was rather acid" (full lead filter sweep in the build); "bass too present, melody in the background too far away" (seed 566557: arp drowned in delay/reverb). | Open hat probability 0.8 -> 0.25; builds drop ride, rim, open hat, pad and in the last quarter the clap; kick tuned 40-50 Hz with less click and longer decay, punch style rare; ride more noise than metal, 8ths rare; build lead filter only opens gently; acid lead rare; bass -1.2 dB, arp +4 dB with far less delay/reverb. |
| 0.2.0 | The 4-chord motif (root changes 4x over 16 bars) is too melodic for dark techno; it is fine in breakdowns sometimes. Wanted: 2-chord changes, no change, thinned chords, octave on top at the end, a "3 and back home" shape. Also slow filter variation on synths (not on the bass, not acid). | Progression shapes drone / two / return (0-x-0-0) / turn (0-0-0-x) / four (rare, < 5 % of songs); harmony everywhere or breakdowns only (50/50). Per phrase: synth octave up in the last bar, chords thinned to root and fifth, delay throws. Per section: slow synth filter movement (rise, fall, swell, 8-bar breathe; +-35 %). |
| 0.2.0 | Feedback buttons did not deliver anything. | Feedback is now stored on the device instantly and exported as a list (copy / share / one GitHub issue via a plain link). |

## Open observations

- Sound design repertoire is still small (listener feedback). Candidates: chorus / phaser on
  synths, tape saturation, bit crushing, stereo widening, reverb swells, filter LFOs, more
  sidechain shapes, noise textures. See RULESETS.md for how new modules should be added
  (versioned, append-only once a genre is frozen).

- The favourite has no breakdown, but breakdowns are liked in other songs ("almost like a
  build-up"): keep them, do not lower their probability further.

## How to analyse

Copy the list from the site's "Dein Feedback" card into a file and run, **at the build the
feedback was given with** (every entry carries its build; seeds sound different in other
builds):

```sh
git checkout <build>            # from the entries
cargo run --release -p techno-cli -- analyze --input feedback.txt
```

It rebuilds each song from its seed and lists what was playing (tracks, section and its
style, sounds, structure), then counts every feature for 👍 vs 👎.

Weighting: a 👎 counts half as much as a 👍 (score = 👍 − 0.5 × 👎), so a few complaints do
not outweigh what was liked repeatedly. 💡 ideas are neutral suggestions: listed separately,
never counted as good or bad.
