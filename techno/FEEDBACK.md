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
| 0.2.0 | Feedback buttons did not deliver anything. | Feedback is now stored on the device instantly and exported as a list (copy / share / one GitHub issue via a plain link). |

## Open observations

- The favourite has no breakdown. If more favourites share that, lower `after_groove` /
  `after_drop` weights for "breakdown" further.
