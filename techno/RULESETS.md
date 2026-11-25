# Genre rulesets, freezing and modifiers (concept)

Status: proposal, not implemented yet. Today the style profile (`profiles/*.toml`) already
holds most rules as data; this document describes how to turn it into numbered, freezable
genre rulesets with live modifiers.

## Why

During research the rules change daily, so seeds change meaning all the time; that is fine.
Once a genre sounds right it is **frozen**: from then on a song is fully described by
`genre@version + seed + modifiers` and sounds the same forever. Live changes happen only
through **modifiers** on top of the frozen rules, never by editing them. A genre that does
not work is not patched but replaced by a new one (e.g. `dark-techno-2`).

## Terms

| Term | Meaning |
|------|---------|
| Rule catalog | Global list of rule IDs. Each ID has one fixed meaning in every genre. |
| Ruleset | One genre's values for (some of) the catalog rules. |
| Default | Value of a rule a genre does not set (usually 0 / off / "none"). |
| Freeze | Ruleset version + engine build are pinned; nothing about them changes again. |
| Modifier | Temporary, logged change of one rule's value during a set. |

## Rule IDs: one global catalog, grouped by domain

Sequential numbers per genre ("rule 5") would make rule 5 mean different things in different
genres and comparisons meaningless. Instead every ID has a fixed meaning everywhere; a genre
that does not use a rule simply leaves it at its default, which is exactly the "not set
means 0" comparison you described.

| Range | Domain | Examples (today's profile fields) |
|-------|--------|-----------------------------------|
| 100-199 | Tempo, key, harmony | R101 BPM range, R110 scale weights, R120 progression shape, R121 harmony scope |
| 200-299 | Drums | R201 kick style, R205 kick tuning, R220 open hat probability, R230 ride style |
| 300-399 | Bass | R301 bass rhythm, R305 movement, R310 cutoff range |
| 400-499 | Synths | R401 main short synth, R402 breakdown synth probability, R410 lead kind |
| 500-599 | Sound design | R501 synth filter movement shape, R502 depth, R510 delay throw probability |
| 600-699 | Structure | R601 song length, R610 after-groove weights, R620 riser kinds, R630 drop styles |
| 700-799 | Mix | R701 sidechain depth, R710 per-track levels |
| 800-899 | Set / transitions | R801 transition kinds, R810 Camelot step weights |

Numbers inside a range are appended, never reused: a removed rule keeps its number retired.

## Value types

| Type | Example | Notes |
|------|---------|-------|
| Probability | `R220 = 0.25` | 0..1 |
| Weighted choice | `R201 = { hard = 4, punch = 0.5, distorted = 2 }` | weights, not percentages; 0 = never |
| Range | `R205 = [40, 50]` | sampled uniformly |
| Number | `R601 = 200` | |
| Reference | `R402 = "R401"` | same value as another rule |
| Conditional | `R121 = { when = "R120 == drone", then = 0 }` | see below |

Conditionals are kept deliberately simple: one condition per rule, equality or comparison on
another rule's value, evaluated in catalog order. Anything more complex belongs in code as a
new rule, otherwise the ruleset turns into an unreadable programming language.

## Randomness per rule

Every rule draws from its own random stream (`seed` combined with the rule ID). A modifier
on R220 then only changes R220's decisions; all other choices of the song stay identical.
Without this, changing one probability would reshuffle everything after it. The engine
already works this way for most decisions (`rng.fork("...")`).

## Freezing also pins the engine

Determinism depends on code too (synthesis, timing, how rules are interpreted). A frozen
genre therefore pins the engine build: the site keeps each frozen genre's WebAssembly file
(`app/engines/dark-techno-1.wasm`) next to the current development build. Sound modules
referenced by rules are versioned and append-only (`kick_hard_v1` is never changed;
improvements become `kick_hard_v2`).

## Modifiers

```text
{ rule: R220, op: add | mul | set, value: -0.10, from: bar 96 of song 3, for: 64 bars }
```

- At most about 10 active at once (your estimate sounds right).
- Every modifier is an event with a musical timestamp in the set log, so a set with
  modifiers is still exactly reproducible: `dark-techno-1 · set 4242 · events [...]`.
- Values are clamped to the rule's valid range (a probability never leaves 0..1).
- The live controls planned for v2 (energy, "drop now", mute) become modifiers too.

## Research workflow

1. Research phase (now): edit the profile freely, collect feedback, `techno analyze`.
2. Freeze: assign the version, pin the engine build, publish the wasm.
3. Live: only modifiers. Feedback still collected, it feeds the next genre version.

## Assessment

The idea fits the existing architecture well: rules are already data, decisions already
use separate random streams, and control events were planned from the start. Changes I
recommend compared to the original idea: a global catalog instead of per-genre numbering,
one random stream per rule, and pinning the engine build together with the ruleset.

## Possible implementation steps

1. Give every profile field a catalog ID and add `techno rules`, which prints the ruleset
   as a table (and a comparison table between genres).
2. Modifier engine plus event log, used by the CLI and the web player.
3. Freeze `dark-techno-1`: version, pinned wasm on the site, song address in the UI and in
   feedback.
4. Live modifier controls in the web player.
