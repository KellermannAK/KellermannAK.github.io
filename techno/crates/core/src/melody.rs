//! Melody layer (experiment, house): a real melody over several bars that follows the chord
//! progression underneath, instead of an arpeggio or a short riff.
//!
//! Notes are stored relative to the chord root of their bar (scale degrees, like every other
//! motif), so the line follows the progression automatically: strong notes sit on chord tones
//! (3rd, 7th and 9th are favoured over root and 5th for a jazzy colour), weak notes walk along
//! the scale, and now and then a short chromatic approach note leads into a chord tone.
//! Rhythms are syncopated jazz-like cells; a phrase is 8 bars (statement, variation, answer,
//! cadence), a melody is one or two phrases and then loops.
//!
//! Removing it again: set `[melody] probability = 0` in the profile (or delete the section).
//! All code is in this module plus the places marked `// melody:` (structure gate, DNA fields,
//! deck voice, track list).

use crate::profile::MelodyRules;
use crate::rng::Rng;

pub const MAX_MELODY_STEPS: usize = 256;

/// Chord tones relative to the chord root, in scale degrees: root, 3rd, 5th, 7th, 9th.
const CHORD_TONES: [i32; 5] = [0, 2, 4, 6, 8];
/// How likely each chord tone is on a strong beat (jazz: 3rd / 7th / 9th over root / 5th).
const TONE_WEIGHTS: [f32; 5] = [1.0, 3.0, 1.4, 3.0, 2.0];

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
/// semitone (approach tones), 0 for every note that belongs to the scale.
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
    /// `|` between bars; chromatic notes in upper case.
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

fn pick_rhythm(rng: &mut Rng, avoid: &[usize]) -> usize {
    for _ in 0..8 {
        let r = rng.range_i32(0, BUSY as i32 - 1) as usize;
        if !avoid.contains(&r) {
            return r;
        }
    }
    0
}

/// Rhythm cells of one 8-bar phrase: statement, answer, variation, cadence; then the
/// answer phrase. `rest_prob` swaps busy bars for sparse ones (the line breathes).
fn phrase_rhythms(rng: &mut Rng, rest_prob: f32, second: bool) -> [usize; 8] {
    let a = pick_rhythm(rng, &[]);
    let b = pick_rhythm(rng, &[a]);
    let c = pick_rhythm(rng, &[a, b]);
    let cad = CADENCE[rng.range_i32(0, 1) as usize];
    let mut plan = if second {
        [a, b, c, cad, b, a, c, cad]
    } else {
        [a, b, a, cad, b, a, c, cad]
    };
    for (i, r) in plan.iter_mut().enumerate() {
        if i != 3 && i != 7 && i != 0 && rng.chance(rest_prob) {
            *r = SPARSE[rng.range_i32(0, 1) as usize];
        }
    }
    plan
}

/// Chord tone near `target` (any octave inside `lo..=hi`), weighted by jazz colour and
/// distance. `avoid` is not chosen again (no repeated pitch on strong beats).
fn chord_tone_near(rng: &mut Rng, target: i32, lo: i32, hi: i32, avoid: Option<i32>) -> i32 {
    let mut cand = [0i32; 16];
    let mut w = [0f32; 16];
    let mut n = 0;
    for oct in -1..=1 {
        for (i, t) in CHORD_TONES.iter().enumerate() {
            let d = t + 7 * oct + 7 * (target.div_euclid(7));
            if d < lo || d > hi || Some(d) == avoid || n == cand.len() {
                continue;
            }
            cand[n] = d;
            w[n] = TONE_WEIGHTS[i] / (1.0 + 0.55 * (d - target).abs() as f32);
            n += 1;
        }
    }
    if n == 0 {
        return target.clamp(lo, hi);
    }
    cand[rng.weighted_index(&w[..n])]
}

/// Generate a melody line. Independent of key and progression: degrees are relative to the
/// chord root of each bar, so the line follows any progression.
pub fn generate(rng: &mut Rng, r: &MelodyRules) -> MelodyLine {
    let bars = if r.bars.pick(rng) >= 16 { 16 } else { 8 };
    let [lo, hi] = r.range;
    let mut line = MelodyLine::empty();
    line.bars = bars;

    let mut prev = chord_tone_near(rng, lo + (hi - lo) / 3, lo, hi, None);
    for phrase in 0..(bars / 8) as usize {
        let plan = phrase_rhythms(rng, r.rest_prob, phrase == 1);
        for (b, &cell) in plan.iter().enumerate() {
            let bar = phrase * 8 + b;
            // Arch over the phrase: rises to the middle, falls to the cadence.
            let arch = (core::f32::consts::PI * (b as f32 + 0.5) / 8.0).sin();
            let center = lo as f32 + (hi - lo) as f32 * (0.25 + 0.5 * arch);
            let cadence = b == 3 || b == 7;
            let cells = RHYTHMS[cell];
            let mut degs: [i32; 8] = [0; 8];
            let mut strong: [bool; 8] = [false; 8];
            for (i, &(start, len)) in cells.iter().enumerate() {
                let is_strong = start % 8 == 0 || i == 0 || len >= 6;
                strong[i] = is_strong;
                // Pull towards the arch centre, small steps otherwise.
                let pull = ((center - prev as f32).signum() as i32) * rng.chance(0.45) as i32;
                let step = [-2, -1, -1, 1, 1, 2][rng.range_i32(0, 5) as usize] + pull;
                let mut desired = (prev + step).clamp(lo, hi);
                if desired == prev {
                    // Weak notes do not repeat the pitch (no "dum dum dum").
                    desired = if prev < hi { prev + 1 } else { prev - 1 };
                }
                let last = i + 1 == cells.len();
                degs[i] = if cadence && last {
                    // Cadence: land on the 3rd, the root or the 5th (never a colour tone).
                    let t = [2, 0, 4][rng.weighted_index(&[3.0, 2.5, 1.0])];
                    chord_tone_near(rng, t + 7 * (desired.div_euclid(7)), lo, hi, Some(prev))
                } else if is_strong {
                    chord_tone_near(rng, desired, lo, hi, Some(prev))
                } else {
                    desired
                };
                prev = degs[i];
            }
            // Jazz colour: a short weak note right before a strong one becomes a chromatic
            // approach from below or above.
            let mut chroma = [0i8; 8];
            for i in 0..cells.len().saturating_sub(1) {
                let (start, len) = cells[i];
                if !strong[i]
                    && len <= 2
                    && strong[i + 1]
                    && start % 2 == 0
                    && rng.chance(r.chromatic_prob)
                {
                    degs[i] = degs[i + 1];
                    chroma[i] = if i > 0 && degs[i - 1] < degs[i] {
                        -1
                    } else {
                        1
                    };
                }
            }
            for (i, &(start, len)) in cells.iter().enumerate() {
                let vel = if cadence && i + 1 == cells.len() {
                    200
                } else if strong[i] {
                    230
                } else {
                    175
                };
                line.notes[bar * 16 + start as usize] = Some(MelNote {
                    degree: degs[i] as i8,
                    chroma: chroma[i],
                    len,
                    vel,
                });
            }
        }
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;

    fn lines() -> impl Iterator<Item = MelodyLine> {
        let p = Profile::house();
        (0..300u64).map(move |s| generate(&mut Rng::new(s), &p.melody))
    }

    #[test]
    fn deterministic_and_phrased() {
        let p = Profile::house();
        let a = generate(&mut Rng::new(5), &p.melody);
        let b = generate(&mut Rng::new(5), &p.melody);
        assert_eq!(a.to_string_grid(), b.to_string_grid());
        for l in lines() {
            assert!(l.bars == 8 || l.bars == 16);
            assert!(
                l.note_count() >= 8 * l.bars as usize / 4,
                "not enough notes"
            );
        }
    }

    #[test]
    fn stays_in_range_and_notes_do_not_overlap() {
        let p = Profile::house();
        let [lo, hi] = p.melody.range;
        for l in lines() {
            let mut busy_until = 0usize;
            for i in 0..l.bars as usize * 16 {
                if let Some(n) = l.notes[i] {
                    assert!(i >= busy_until, "overlap at {i}");
                    busy_until = i + n.len as usize;
                    assert!((lo..=hi).contains(&(n.degree as i32)), "{}", n.degree);
                    assert!((-1..=1).contains(&n.chroma));
                    // A note never runs into the next bar.
                    assert!(i % 16 + n.len as usize <= 16);
                }
            }
        }
    }

    #[test]
    fn strong_notes_are_chord_tones() {
        // Strong = on beat 1 or 3 of a bar (start % 8 == 0): chord tones, except approach notes.
        let (mut strong, mut tones) = (0, 0);
        for l in lines() {
            for i in 0..l.bars as usize * 16 {
                if let Some(n) = l.notes[i] {
                    if i % 8 == 0 && n.chroma == 0 {
                        strong += 1;
                        // Root, 3rd, 5th, 7th and 9th (degree 8 = 1 above the octave).
                        tones +=
                            matches!((n.degree as i32).rem_euclid(7), 0 | 1 | 2 | 4 | 6) as u32;
                    }
                }
            }
        }
        assert!(strong > 1000);
        assert!(tones as f32 > 0.97 * strong as f32, "{tones}/{strong}");
    }

    #[test]
    fn some_chromatic_approach_notes_but_not_many() {
        let (mut notes, mut chromatic) = (0, 0);
        for l in lines() {
            for n in l.notes.iter().flatten() {
                notes += 1;
                chromatic += (n.chroma != 0) as u32;
            }
        }
        assert!(chromatic > 0, "no jazz colour at all");
        assert!(
            (chromatic as f32) < 0.2 * notes as f32,
            "{chromatic}/{notes}"
        );
    }
}
