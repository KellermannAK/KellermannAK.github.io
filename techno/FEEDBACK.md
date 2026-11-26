# Listening feedback log

What the listener liked or disliked, and what was changed because of it. Read this before
changing the profile or the arrangement, so earlier decisions are not undone by accident.

## Favourites

| Seed | Engine | Why it is interesting |
|------|--------|-----------------------|
| 614548 | 0.2.0 (85b1409) | Techno, "Top song" (2x 👍): distorted kick, supersaw lead in pulse style, no long layer, breakdown then drop. |
| 87262, 348372 | techno (5242db3) | 👍 "minimalistic hard", "breakdown was great, the drop after it need not be": sparse pluck lead, hard kick, breakdown with groove or stripped drop afterwards. |
| 184968 | house (e0320cf) | House, "Nice melody": D# dorian, swing 0.55, organ stab main, sine-pluck bass on the offbeats, guitar takes over in breakdowns. |
| 742138 | 0.2.0 | F minor, two roots only (i-VI); hard kick tuned low (43.7 Hz); no open hat; clap + snare backbeat; rolling bass; pluck lead as the only short synth plus a triad pad; **no breakdown** - groove, build -> drop (bass first), groove, build -> drop (stripped). |

## Feedback and resulting changes

| Engine | Feedback | Change |
|--------|----------|--------|
| 0.1.0 | Layered short synth notes sound like "tinkling", not deep techno. | Only one short synth (lead / arp / stab) per bar; a second may only replace it in breakdowns; at most one long layer (pad or drone). Chords are fine. |
| 0.2.0 | Too many melodic sounds; a percussive "pinch" (seed 520584, bar 46). | Tuned toms with a big pitch sweep were the culprit: toms now only in fills, lower, smaller sweep. Rim less resonant, no delay. Arp mostly 8ths over triad / power chord. |
| 0.2.0 | Builds always use the same "fairground" riser; drops always identical; 32-bar breakdowns too long; breakdown -> build too predictable. "Don't forbid, reduce probability." | Per-section riser (noise / gated / swell / rare pitched / none), roll (snare / clap / kick / none), optional high-pass; drop styles (full / bass first / stripped / filter open) and entries; breakdowns mostly 8-16 bars, only 30 % followed by a build. |
| 0.2.0 (9e32f20) | GitHub issue #1, 12 entries (7 👍, 5 👎). Open hats 1 👍 / 5 👎; builds 0 👍 / 2 👎 ("too much at once"); drone layer 2 👍; "punch kick doesn't fit dark techno" (seed 366500: actually the hard kick, but tuned high with lots of click); "8th percussion too melodic" (ride in 8ths); "build was rather acid" (full lead filter sweep in the build); "bass too present, melody in the background too far away" (seed 566557: arp drowned in delay/reverb). | Open hat probability 0.8 -> 0.25; builds drop ride, rim, open hat, pad and in the last quarter the clap; kick tuned 40-50 Hz with less click and longer decay, punch style rare; ride more noise than metal, 8ths rare; build lead filter only opens gently; acid lead rare; bass -1.2 dB, arp +4 dB with far less delay/reverb. |
| 0.2.0 | The 4-chord motif (root changes 4x over 16 bars) is too melodic for dark techno; it is fine in breakdowns sometimes. Wanted: 2-chord changes, no change, thinned chords, octave on top at the end, a "3 and back home" shape. Also slow filter variation on synths (not on the bass, not acid). | Progression shapes drone / two / return (0-x-0-0) / turn (0-0-0-x) / four (rare, < 5 % of songs); harmony everywhere or breakdowns only (50/50). Per phrase: synth octave up in the last bar, chords thinned to root and fifth, delay throws. Per section: slow synth filter movement (rise, fall, swell, 8-bar breathe; +-35 %). |
| 0.2.0 (7387241) | Issue #2, entries 13-23 (entries 24-25 cut off by the URL limit). Arp in 16ths "too much movement" in a drop but cool in the breakdown (944599); 32-bar breakdown "too long"; "fairground build-up" again (pitched riser) and a kick-roll build disliked (736049); busy acid riff "too melodic, odd chords" (742367) vs. a sparse 3-pitch lead "best part" (470750); 💡 drops almost never good; 💡 songs too similar (instruments, chords, melody rhythms). | Arp 16ths only in breakdowns (8ths elsewhere); 32-bar breakdowns very rare; pitched riser off, kick roll rare, acid lead rare; lead styles sparse / pulse / long / riff (riff now rare); two new bass rhythms (syncopated, sparse) and flatter weights; drops: harder kick drive and louder rumble, grooves hold the bass filter back. Drops need a dedicated round. |
| 0.2.0 | Feedback buttons did not deliver anything. | Feedback is now stored on the device instantly and exported as a list (copy / share / one GitHub issue via a plain link). |

## Open observations

- Sound design repertoire is still small (listener feedback). Done (0.2.0, round 3): insert
  effects chorus, phaser, crusher, tremolo/autopan, tape on the short synths and the long
  layer, phased hats, effect intensity up in breakdowns and builds, synth filter movement,
  delay throws. Still open: stereo widening, reverb swells, more sidechain shapes, noise /
  atmosphere textures, filter LFOs on percussion. See RULESETS.md for how new modules should be added
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
| house (e0320cf) | Guitar much too loud (mix 361546, end of song 9: picked steel guitar) | Guitar channel -4 dB, picked notes a further -1.8 dB. |
| house (e0320cf) | "Strange double claps" in the previous section | Ghost claps right before the backbeat (steps 3/11) removed for house (`ghost_spots`): with swing they sounded like a flam; the wide clap variant is rare now. |
| house (e0320cf) | Song "crowded", "childish housy" (seed 352168: busy piano stabs + strummed guitar companion, plain triads) | Companion only if one of the two instruments leaves room (held piano, muted/picked guitar, lead); triads rare; busy riff leads rare; held piano more common. |
| house (e0320cf) | 👍 dorian / high swing / pad + guitar breakdown | Swing range 0.35-0.6, dorian weighted up, major down. |
| 0.2.0 (85b1409) | 💡 one techno mix song "rather tech house" (seed 293512) | Noted; candidate for a third genre. |
| techno (5242db3) | Risers with ever faster hits sound like a fairground (3x); roll + halving beats; "cliffhanger instead of a riser, like a breakdown" | Rolls keep their speed (`roll_accelerates = false`), risers/rolls much rarer, new cliffhanger builds (`cliffhanger_prob = 0.4`: no riser, no roll, two silent bars before the drop). |
| techno (5242db3) | Drops "terrible" (3x); "only bass and rumble"; "drop after the breakdown need not be" | `bass_first` drop rare, fewer builds after grooves, after a breakdown the groove continues in 60 % of cases (`after_breakdown`). |
| techno (5242db3) | Chords on the offbeat "too pretty for deep techno"; "no deep techno melody", "too tüdelü" | Stab chords: triad/power favoured over seventh/dub; pad mostly triad; acid and busy riff leads rare. |
| both (5242db3) | Transition "bad with swing", "swing doing crazy stuff" | Decks with different swing flammed during blends. First fix (incoming song takes over the swing) rejected: every song would swing alike. Now the swing glides from the old to the new song's value over the whole transition (both decks share it, applied at step boundaries), the new song then keeps its own swing. House swing 0.3-0.5. |
| house (5242db3) | 8ths with open hat too loud | `open_hat_gain = 0.5` (new mix rule). |
| house (5242db3) | Guitar too loud and too clear again | Guitar a further -3.3 dB, darker (brightness 0.15-0.5). |
| house (5242db3) | Melody 4th tone atonal | `chord_tones_only`: lead notes snap to chord tones. |
| house (5242db3) | Bassline too dirty for nice house | Saw/reese bass variants removed, mostly sine pluck / square, less drive and resonance. |
| house (5242db3) | Hard cut between sections; theme changes liked but too frequent | `pre_breakdown_thin`: last 2 bars before a breakdown without synths; breakdown instrument swap probability 0.5 -> 0.22. |
