//! Melody layer (experiment, house): a real melody over several bars that follows the chords
//! underneath. The long notes of the pad are the foundation: every note is chosen against the
//! chord that sounds in its bar, using the usual rules of tonal melody writing:
//!
//! * **Chord tones carry the melody.** Long notes and notes on the strong beats are tones of
//!   the pad chord (3rd and 7th favoured over root and 5th, the 9th as a colour).
//! * **Other scale tones are decoration.** Short notes on weak beats may be passing tones
//!   (a step, continuing in the same direction) or neighbour tones (a step away and back),
//!   both approached and left by step. A scale tone one semitone above a chord tone is an
//!   *avoid note* (jazz rule): it only appears as a quick passing 16th, never as a held note.
//! * **Phrases are periods.** Eight bars: a four-bar antecedent that ends on a half cadence
//!   (a chord tone that is scale degree 5, 2 or 7 where possible), then a four-bar
//!   consequent that ends on the tonic (perfect authentic cadence; over a chord without the
//!   tonic, on the 3rd, root or 5th of that chord), approached by step, often from the 2nd.
//! * **Anchors:** the first note (3rd, 5th or root of its chord), the two cadence notes and
//!   now and then a leap up or down a fifth / sixth at the start of a half phrase. After a
//!   leap the line turns around and fills the gap by step.
//! * **Shape:** an arch over each half phrase, no repeated pitch on consecutive long notes.
//!
//! Notes are stored relative to the chord root of their bar (like all motifs), so the line
//! follows the progression; the progression cycle always divides the melody length.
//!
//! Removing it again: set `[melody] probability = 0` in the profile (or delete the section).
//! All code is in this module plus the places marked `// melody:` (structure gate, DNA fields,
//! deck voice, track list).

use crate::dna::Progression;
use crate::profile::{ChordShape, MelodyRules};
use crate::rng::Rng;
use crate::theory::Key;

pub const MAX_MELODY_STEPS: usize = 256;

/// Rhythm cells for one bar: (start step, length in 16ths). 0..5 busy, 6..7 sparse,
/// 8..9 cadence (long end note).
const RHYTHMS: [&[(u8, u8)]; 10] = [
    &[(0, 6), (6, 2), (8, 4), (12, 2), (14, 2)],
    &[(2, 4), (6, 2), (8, 2), (10, 6)],
    &[(0, 3), (3, 3), (6, 2), (8, 8)],
    &[(4, 4), (8, 2), (10, 2), (12, 4)],
    &[(0, 2), (2, 2), (4, 4), (10, 2), (12, 4)],
    &[(2, 2), (4, 2), (6, 4), (10, 2), (12, 2), (14, 2)],
    &[(0, 10)],
    &[(2, 8), (12, 4)],
    &[(0, 12)],
    &[(0, 4), (6, 8)],
];
const BUSY: usize = 6;
const SPARSE: [usize; 2] = [6, 7];
const CADENCE: [usize; 2] = [8, 9];

/// One melody note. `degree` is relative to the bar's chord root; `chroma` shifts it by a
/// semitone (unused by the generator now, kept for the player), 0 for scale tones.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MelNote {
    pub degree: i8,
    pub chroma: i8,
    /// Length in 16th steps.
    pub len: u8,
    /// Velocity 0..255.
    pub vel: u8,
}

/// A looping melody of `bars` bars (8 or 16).
#[derive(Clone, Copy, Debug)]
pub struct MelodyLine {
    pub notes: [Option<MelNote>; MAX_MELODY_STEPS],
    pub bars: u8,
}

impl MelodyLine {
    pub fn empty() -> MelodyLine {
        MelodyLine {
            notes: [None; MAX_MELODY_STEPS],
            bars: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.bars == 0
    }

    /// Note starting at an absolute 16th step (the melody loops).
    #[inline]
    pub fn at(&self, step: u32) -> Option<MelNote> {
        if self.bars == 0 {
            return None;
        }
        self.notes[(step % (self.bars as u32 * 16)) as usize]
    }

    pub fn note_count(&self) -> usize {
        self.notes.iter().flatten().count()
    }

    /// Compact text form for the CLI: one digit / letter per note start, `.` for none,
    /// `|` between bars.
    pub fn to_string_grid(&self) -> String {
        let mut s = String::new();
        for i in 0..self.bars as usize * 16 {
            if i > 0 && i % 16 == 0 {
                s.push('|');
            }
            s.push(match self.notes[i] {
                None => '.',
                Some(n) if n.chroma != 0 => '#',
                Some(n) => match n.degree {
                    d @ 0..=9 => (b'0' + d as u8) as char,
                    d if d < 0 => (b'a' + (-d - 1).min(25) as u8) as char,
                    _ => '+',
                },
            });
        }
        s
    }
}

// ------------------------------------------------------------------ harmony

/// The deck keeps the harmony in one register: VI and VII go below (see `Deck::fold_root`).
pub fn fold_root(root: i32) -> i32 {
    if root >= 4 {
        root - 7
    } else {
        root
    }
}

/// Pitch classes (semitones above the tonic, 0..12) of the chord on `root` (a scale degree).
pub fn chord_pcs(key: Key, root: i32, chord: ChordShape) -> Vec<i32> {
    chord
        .degrees()
        .iter()
        .map(|d| key.degree_to_semitone(root + d).rem_euclid(12))
        .collect()
}

/// How a scale tone relates to the chord underneath.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Tone of the chord.
    Chord,
    /// Scale tone that is not a chord tone and does not clash: a passing / neighbour tone or
    /// an available tension (9th, 11th, 13th).
    Tension,
    /// Scale tone a semitone above a chord tone: only as a quick passing note.
    Avoid,
}

/// Classify the scale degree `x` (counted from the tonic) against the chord pitch classes.
pub fn classify(key: Key, pcs: &[i32], x: i32) -> Tone {
    let pc = key.degree_to_semitone(x).rem_euclid(12);
    if pcs.contains(&pc) {
        Tone::Chord
    } else if pcs.iter().any(|c| (pc - c).rem_euclid(12) == 1) {
        Tone::Avoid
    } else {
        Tone::Tension
    }
}

struct Bar {
    root: i32,
    pcs: Vec<i32>,
}

impl Bar {
    fn tone(&self, key: Key, x: i32) -> Tone {
        classify(key, &self.pcs, x)
    }

    /// How much a chord tone is wanted on a strong beat: 3rd and 7th over root and 5th, the
    /// 9th as a colour.
    fn weight(&self, key: Key, x: i32) -> f32 {
        let pc = key.degree_to_semitone(x).rem_euclid(12);
        let root_pc = key.degree_to_semitone(self.root).rem_euclid(12);
        match (pc - root_pc).rem_euclid(12) {
            0 => 2.0,
            3 | 4 => 3.0,
            7 => 2.0,
            10 | 11 => 2.4,
            _ => 1.5,
        }
    }
}

// ------------------------------------------------------------------ rhythm

fn pick_rhythm(rng: &mut Rng, avoid: &[usize]) -> usize {
    for _ in 0..8 {
        let r = rng.range_i32(0, BUSY as i32 - 1) as usize;
        if !avoid.contains(&r) {
            return r;
        }
    }
    0
}

/// Rhythm cells of one 8-bar phrase: statement, answer, variation, cadence (antecedent);
/// then the same material again ending on the final cadence (consequent).
fn phrase_rhythms(rng: &mut Rng, rest_prob: f32, second: bool) -> [usize; 8] {
    let a = pick_rhythm(rng, &[]);
    let b = pick_rhythm(rng, &[a]);
    let c = pick_rhythm(rng, &[a, b]);
    let cad = CADENCE[rng.range_i32(0, 1) as usize];
    let cad2 = CADENCE[rng.range_i32(0, 1) as usize];
    let mut plan = if second {
        [a, b, c, cad, b, a, c, cad2]
    } else {
        [a, b, a, cad, b, a, c, cad2]
    };
    for (i, r) in plan.iter_mut().enumerate() {
        if i != 3 && i != 7 && i != 0 && rng.chance(rest_prob) {
            *r = SPARSE[rng.range_i32(0, 1) as usize];
        }
    }
    plan
}

// ------------------------------------------------------------------ generation

struct N {
    bar: usize,
    start: u8,
    len: u8,
    /// Long or on a strong beat: must be a chord tone (or a resolving cadence tone).
    stable: bool,
    x: i32,
}

fn is_stable(start: u8, len: u8) -> bool {
    len >= 4 || start % 8 == 0 || (start % 4 == 0 && len >= 3)
}

/// Scale degrees of `bar`'s chord tones inside `lo..=hi`, with their weights.
fn chord_candidates(bar: &Bar, key: Key, lo: i32, hi: i32) -> Vec<(i32, f32)> {
    (lo..=hi)
        .filter(|x| bar.tone(key, *x) == Tone::Chord)
        .map(|x| (x, bar.weight(key, x)))
        .collect()
}

fn pick_weighted(rng: &mut Rng, c: &[(i32, f32)]) -> Option<i32> {
    if c.is_empty() {
        return None;
    }
    let w: Vec<f32> = c.iter().map(|x| x.1).collect();
    Some(c[rng.weighted_index(&w)].0)
}

/// Target of a half cadence: a chord tone that is 5, 2 or 7 of the key where possible.
fn half_target(rng: &mut Rng, bar: &Bar, key: Key, lo: i32, hi: i32, center: i32) -> i32 {
    let mut c = chord_candidates(bar, key, lo, hi);
    for x in c.iter_mut() {
        let class = match x.0.rem_euclid(7) {
            4 => 3.0,
            1 => 2.5,
            6 => 1.5,
            _ => 1.0,
        };
        x.1 = class / (1.0 + 0.5 * (x.0 - center).abs() as f32);
    }
    pick_weighted(rng, &c).unwrap_or(center)
}

/// Target of the final cadence: the tonic if the chord has it, else 3rd / root / 5th.
fn end_target(rng: &mut Rng, bar: &Bar, key: Key, lo: i32, hi: i32, center: i32) -> (i32, bool) {
    if bar.pcs.contains(&0) {
        let tonics: Vec<(i32, f32)> = (lo..=hi)
            .filter(|x| x.rem_euclid(7) == 0)
            .map(|x| (x, 1.0 / (1.0 + (x - center).abs() as f32)))
            .collect();
        if let Some(x) = pick_weighted(rng, &tonics) {
            return (x, true);
        }
    }
    let mut c = chord_candidates(bar, key, lo, hi);
    for x in c.iter_mut() {
        x.1 /= 1.0 + 0.5 * (x.0 - center).abs() as f32;
    }
    (pick_weighted(rng, &c).unwrap_or(center), false)
}

/// Generate a melody line over `prog` (played by `chord` on the pad).
pub fn generate(
    rng: &mut Rng,
    r: &MelodyRules,
    key: Key,
    prog: &Progression,
    chord: ChordShape,
) -> MelodyLine {
    let cycle = prog.cycle_bars().max(1) as u8;
    // The progression cycle divides the melody length, so bar i of the melody always has the
    // chord it was written for.
    let bars: u8 = if cycle > 8 || r.bars.pick(rng) >= 16 {
        16
    } else {
        8
    };
    let [lo, hi] = r.range;
    let center = (lo + hi) / 2;
    let harm: Vec<Bar> = (0..bars as u32)
        .map(|b| {
            let root = fold_root(prog.root_at_bar(b));
            Bar {
                root,
                pcs: chord_pcs(key, root, chord),
            }
        })
        .collect();

    // ---- notes of the whole line, bar by bar
    let mut notes: Vec<N> = Vec::new();
    for phrase in 0..(bars / 8) as usize {
        let plan = phrase_rhythms(rng, r.rest_prob, phrase == 1);
        for (b, &cell) in plan.iter().enumerate() {
            for (i, &(start, len)) in RHYTHMS[cell].iter().enumerate() {
                notes.push(N {
                    bar: phrase * 8 + b,
                    start,
                    len,
                    // The first note of a phrase is an anchor, whatever its length.
                    stable: is_stable(start, len) || (b == 0 && i == 0),
                    x: center,
                });
            }
        }
    }

    // ---- anchors: phrase start, half cadence, final cadence
    let last_in_bar = |notes: &[N], bar: usize| notes.iter().rposition(|n| n.bar == bar);
    let mut targets: Vec<Option<i32>> = vec![None; notes.len()];
    let mut tonic_end: Vec<bool> = vec![false; notes.len()];
    for phrase in 0..(bars / 8) as usize {
        let first = notes.iter().position(|n| n.bar == phrase * 8).unwrap();
        // Start on the 3rd, 5th or root of the first chord, near the middle.
        let mut c = chord_candidates(&harm[phrase * 8], key, lo, hi);
        for x in c.iter_mut() {
            x.1 /= 1.0 + 0.6 * (x.0 - center).abs() as f32;
        }
        targets[first] = pick_weighted(rng, &c);
        let half = last_in_bar(&notes, phrase * 8 + 3).unwrap();
        targets[half] = Some(half_target(rng, &harm[phrase * 8 + 3], key, lo, hi, center));
        let end = last_in_bar(&notes, phrase * 8 + 7).unwrap();
        let (t, tonic) = end_target(rng, &harm[phrase * 8 + 7], key, lo, hi, center);
        targets[end] = Some(t);
        tonic_end[end] = tonic;
    }

    // ---- pass 1: the stable notes (chord tones), anchors fixed
    let mut prev = center;
    let mut last_step = 0i32; // signed distance of the last move in scale steps
    for k in 0..notes.len() {
        if !notes[k].stable {
            continue;
        }
        let bar = notes[k].bar;
        let h = &harm[bar];
        // Next stable note, if it is an anchor: approach it from the side we come from.
        let next_anchor = (k + 1..notes.len())
            .find(|j| notes[*j].stable)
            .and_then(|j| targets[j].map(|t| (t, tonic_end[j], j)));
        let x = if let Some(t) = targets[k] {
            // An anchor more than an octave away from the previous note moves to the
            // nearest tone of the same kind that can be reached.
            if (t - prev).abs() <= 7 {
                t
            } else if tonic_end[k] {
                (lo..=hi)
                    .filter(|x| x.rem_euclid(7) == 0 && (x - prev).abs() <= 7)
                    .min_by_key(|x| (x - t).abs())
                    .unwrap_or(t)
            } else {
                chord_candidates(h, key, lo, hi)
                    .into_iter()
                    .filter(|(x, _)| (x - prev).abs() <= 7)
                    .min_by_key(|(x, _)| (x - t).abs())
                    .map(|(x, _)| x)
                    .unwrap_or(t)
            }
        } else {
            let arch = ((core::f32::consts::PI * ((bar % 4) as f32 + 0.5) / 4.0).sin()) as f32;
            let arch_center = lo as f32 + (hi - lo) as f32 * (0.25 + 0.5 * arch);
            let mut desired = prev + [-2, -1, -1, 1, 1, 2][rng.range_i32(0, 5) as usize];
            if rng.chance(0.4) {
                desired += ((arch_center - desired as f32).signum()) as i32;
            }
            // Special leap at the start of a half phrase: a fifth or sixth, then gap fill.
            let half_start = notes[k].start == 0 && bar % 4 == 0 && bar > 0;
            if half_start && rng.chance(r.leap_prob) {
                let dir = if prev < center { 1 } else { -1 };
                desired = prev + dir * (4 + rng.range_i32(0, 1));
            }
            if let Some((t, _, _)) = next_anchor {
                // Step towards the cadence note.
                desired = t + (prev - t).signum() * (1 + rng.range_i32(0, 1));
            }
            let mut c = chord_candidates(h, key, lo, hi);
            // Resolving appoggiatura before a tonic cadence: the 2nd or 7th of the key,
            // resolving by step onto the tonic (only if it does not clash with the chord).
            if let Some((t, true, _)) = next_anchor {
                for d in [1, -1] {
                    let x = t + d;
                    if (lo..=hi).contains(&x) && h.tone(key, x) == Tone::Tension {
                        c.push((x, 2.5));
                    }
                }
            }
            // Never more than an octave away from the previous note.
            let near_enough: Vec<(i32, f32)> = c
                .iter()
                .copied()
                .filter(|(x, _)| (x - prev).abs() <= 7)
                .collect();
            let c = if near_enough.is_empty() {
                c
            } else {
                near_enough
            };
            let gap_fill = last_step.abs() >= 4;
            let filtered: Vec<(i32, f32)> = c
                .iter()
                .copied()
                .filter(|(x, _)| {
                    !gap_fill
                        || ((x - prev).signum() == -last_step.signum() && (x - prev).abs() <= 2)
                })
                .collect();
            let pool = if filtered.is_empty() { c } else { filtered };
            let weighted: Vec<(i32, f32)> = pool
                .iter()
                .map(|(x, w)| {
                    let d = (x - desired).abs() as f32;
                    let near = 1.0 / (1.0 + 0.45 * d * d);
                    let repeat = if *x == prev { 0.15 } else { 1.0 };
                    (*x, w * near * repeat)
                })
                .collect();
            pick_weighted(rng, &weighted).unwrap_or(prev)
        };
        last_step = x - prev;
        notes[k].x = x;
        prev = x;
    }

    // ---- pass 2: the short notes between them: passing, neighbour and chord tones
    for k in 0..notes.len() {
        if notes[k].stable {
            continue;
        }
        let h = &harm[notes[k].bar];
        let p = if k > 0 { notes[k - 1].x } else { center };
        let nxt = (k + 1..notes.len())
            .find(|j| notes[*j].stable)
            .map(|j| notes[j].x)
            .unwrap_or(p);
        let len = notes[k].len;
        let mut c: Vec<(i32, f32)> = Vec::new();
        let mut add = |x: i32, w: f32, h: &Bar| {
            if x < lo || x > hi {
                return;
            }
            let mult = match h.tone(key, x) {
                Tone::Chord => 1.0,
                Tone::Tension => 0.3,
                // Avoid notes only as a quick passing 16th.
                Tone::Avoid if len <= 1 => 0.08,
                Tone::Avoid => return,
            };
            // Held non-chord notes are rare: three steps or more is a chord tone's job.
            let held = if len >= 3 && h.tone(key, x) != Tone::Chord {
                0.2
            } else {
                1.0
            };
            c.push((x, w * mult * held));
        };
        let dist = nxt - p;
        if dist.abs() >= 2 {
            add(p + dist.signum(), 4.0, h); // passing tone
        } else {
            add(p + 1, 2.2, h); // upper neighbour
            add(p - 1, 1.6, h); // lower neighbour
        }
        for x in p - 3..=p + 3 {
            if x != p && (lo..=hi).contains(&x) && h.tone(key, x) == Tone::Chord {
                let d = (x - p).abs() as f32;
                c.push((x, h.weight(key, x) / (1.0 + 0.4 * d * d)));
            }
        }
        if h.tone(key, p) == Tone::Chord && (lo..=hi).contains(&p) {
            c.push((p, 0.8));
        }
        // Short notes lead towards the next long note: never further away from it than we
        // started (plus the one step of a neighbour tone), and never a leap beyond an octave.
        let reach = (p - nxt).abs().max(1) + 1;
        let towards: Vec<(i32, f32)> = c
            .iter()
            .copied()
            .filter(|(x, _)| (x - nxt).abs() <= reach && (x - nxt).abs() <= 7)
            .collect();
        let mut c = if towards.is_empty() { c } else { towards };
        // After a leap the line turns around and fills the gap by step.
        let before = if k >= 2 {
            notes[k - 1].x - notes[k - 2].x
        } else {
            0
        };
        if before.abs() >= 4 {
            for (x, w) in c.iter_mut() {
                let back = (*x - p).signum() == -before.signum() && (*x - p).abs() <= 2;
                *w *= if back { 3.0 } else { 0.2 };
            }
        }
        notes[k].x = pick_weighted(rng, &c).unwrap_or(nxt);
    }

    // ---- write the line
    let mut line = MelodyLine::empty();
    line.bars = bars;
    for (k, n) in notes.iter().enumerate() {
        let cadence_end = targets[k].is_some() && n.stable && n.len >= 8;
        let vel = if cadence_end {
            200
        } else if n.stable {
            230
        } else {
            175
        };
        line.notes[n.bar * 16 + n.start as usize] = Some(MelNote {
            degree: (n.x - harm[n.bar].root) as i8,
            chroma: 0,
            len: n.len,
            vel,
        });
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Profile;
    use crate::theory::ScaleKind;

    fn prog(count: u8, bars_per_root: u8, roots: [i8; 4]) -> Progression {
        Progression {
            roots,
            count,
            bars_per_root,
            breakdown_only: false,
        }
    }

    fn setups() -> Vec<(Key, Progression, ChordShape)> {
        let mut v = Vec::new();
        for scale in [ScaleKind::Dorian, ScaleKind::NaturalMinor, ScaleKind::Major] {
            for (count, bpr, roots) in [
                (2, 2, [0, 3, 0, 0]),
                (4, 2, [0, 5, 3, 4]),
                (4, 1, [0, 3, 5, 4]),
                (2, 4, [0, 4, 0, 0]),
            ] {
                for chord in [ChordShape::Triad, ChordShape::Seventh, ChordShape::Ninth] {
                    v.push((Key { root: 5, scale }, prog(count, bpr, roots), chord));
                }
            }
        }
        v
    }

    #[test]
    fn deterministic_and_complete() {
        let p = Profile::house();
        let (key, pr, ch) = setups().remove(5);
        let a = generate(&mut Rng::new(5), &p.melody, key, &pr, ch);
        let b = generate(&mut Rng::new(5), &p.melody, key, &pr, ch);
        assert_eq!(a.to_string_grid(), b.to_string_grid());
        assert!(a.note_count() >= 8);
    }

    #[test]
    fn length_fits_the_progression_cycle() {
        let p = Profile::house();
        for (key, pr, ch) in setups() {
            for s in 0..20 {
                let l = generate(&mut Rng::new(s), &p.melody, key, &pr, ch);
                assert_eq!(
                    l.bars as u32 % pr.cycle_bars(),
                    0,
                    "cycle {}",
                    pr.cycle_bars()
                );
            }
        }
    }

    /// Absolute scale degrees of every note, with the chord of its bar.
    fn absolute(l: &MelodyLine, pr: &Progression) -> Vec<(usize, u8, u8, i32)> {
        let mut v = Vec::new();
        for i in 0..l.bars as usize * 16 {
            if let Some(n) = l.notes[i] {
                let bar = i / 16;
                let root = fold_root(pr.root_at_bar(bar as u32));
                v.push((bar, (i % 16) as u8, n.len, root + n.degree as i32));
            }
        }
        v
    }

    #[test]
    fn stable_notes_are_chord_tones_and_held_notes_never_avoid_notes() {
        let p = Profile::house();
        let [lo, hi] = p.melody.range;
        let (mut stable, mut chordal, mut bad_held) = (0, 0, 0);
        for (key, pr, ch) in setups() {
            for s in 0..30 {
                let l = generate(&mut Rng::new(s), &p.melody, key, &pr, ch);
                for (bar, start, len, x) in absolute(&l, &pr) {
                    assert!((lo..=hi).contains(&x), "range {x}");
                    let pcs = chord_pcs(key, fold_root(pr.root_at_bar(bar as u32)), ch);
                    let tone = classify(key, &pcs, x);
                    if is_stable(start, len) {
                        stable += 1;
                        chordal += (tone == Tone::Chord) as u32;
                    }
                    if len >= 2 && tone == Tone::Avoid {
                        bad_held += 1;
                    }
                }
            }
        }
        // Only the resolving appoggiatura before a tonic cadence is a stable non-chord tone.
        assert!(chordal as f32 > 0.95 * stable as f32, "{chordal}/{stable}");
        assert_eq!(bad_held, 0, "avoid notes must only be quick passing notes");
    }

    #[test]
    fn phrases_end_on_cadence_notes() {
        let p = Profile::house();
        let (mut ends, mut tonic_when_possible, mut possible, mut half_ok) = (0, 0, 0, 0);
        for (key, pr, ch) in setups() {
            for s in 0..30 {
                let l = generate(&mut Rng::new(s), &p.melody, key, &pr, ch);
                let notes = absolute(&l, &pr);
                for phrase in 0..l.bars as usize / 8 {
                    let end = notes.iter().rev().find(|n| n.0 == phrase * 8 + 7).unwrap();
                    let pcs = chord_pcs(key, fold_root(pr.root_at_bar(end.0 as u32)), ch);
                    ends += 1;
                    assert_eq!(classify(key, &pcs, end.3), Tone::Chord, "final note");
                    if pcs.contains(&0) {
                        possible += 1;
                        tonic_when_possible += (end.3.rem_euclid(7) == 0) as u32;
                    }
                    let half = notes.iter().rev().find(|n| n.0 == phrase * 8 + 3).unwrap();
                    let pcs = chord_pcs(key, fold_root(pr.root_at_bar(half.0 as u32)), ch);
                    assert_eq!(
                        classify(key, &pcs, half.3),
                        Tone::Chord,
                        "half cadence note"
                    );
                    half_ok += matches!(half.3.rem_euclid(7), 4 | 1 | 6) as u32;
                }
            }
        }
        assert!(ends > 500);
        assert_eq!(
            tonic_when_possible, possible,
            "perfect cadence on the tonic when the chord has it"
        );
        assert!(
            half_ok as f32 > 0.4 * ends as f32,
            "half cadences on 5 / 2 / 7: {half_ok}/{ends}"
        );
    }

    #[test]
    fn leaps_are_the_exception_and_are_followed_by_steps_back() {
        let p = Profile::house();
        let (mut leaps, mut filled, mut intervals) = (0u32, 0u32, 0u32);
        for (key, pr, ch) in setups() {
            for s in 0..30 {
                let l = generate(&mut Rng::new(s), &p.melody, key, &pr, ch);
                let xs: Vec<i32> = absolute(&l, &pr).iter().map(|n| n.3).collect();
                for w in xs.windows(3) {
                    intervals += 1;
                    let (jump, back) = (w[1] - w[0], w[2] - w[1]);
                    assert!(jump.abs() <= 7, "no leap beyond an octave: {w:?} seed {s}");
                    if jump.abs() >= 4 {
                        leaps += 1;
                        filled += (back.signum() == -jump.signum() && back.abs() <= 2) as u32;
                    }
                }
            }
        }
        // A fifth or more is the exception ...
        assert!(
            leaps > 20 && (leaps as f32) < 0.1 * intervals as f32,
            "{leaps}/{intervals}"
        );
        // ... and the line then turns around by step.
        assert!(
            filled as f32 > 0.7 * leaps as f32,
            "gap fill {filled}/{leaps}"
        );
    }
}
