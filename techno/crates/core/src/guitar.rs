//! Guitar chords and strumming, so the guitar plays like a guitarist and not like single
//! synthesised strings.
//!
//! **Chord library:** voicings are built the way a player finds them on the neck: six strings
//! in standard tuning (E2 A2 D3 G3 B3 E4), a barre position with the root on the low E or the
//! A string (E shape or A shape, whichever sits lower on the neck), and every other string on
//! the nearest chord tone within four frets of the barre. 3rd (and 7th for seventh chords) are
//! always in; strings below the root string are not played. Any chord quality of the scale
//! works (minor, major, dominant, half-diminished ...), because the tones come from the key.
//!
//! **Strum library:** per bar a pattern of down / up strokes with accents and muted "chuck"
//! strokes (the dampened percussive strum of funk and disco guitar). Up strokes hit the upper
//! strings only, down strokes run from the low strings up.

use crate::theory::Key;

/// Open strings in standard tuning, low to high (MIDI).
pub const OPEN_STRINGS: [i32; 6] = [40, 45, 50, 55, 59, 64];

/// MIDI notes of a guitar voicing, low to high (`None` = string not played).
pub type Voicing = [Option<i32>; 6];

/// Voicing of the chord on scale degree `root` (relative to the key's tonic); triad or
/// seventh chord from the scale.
pub fn voicing(key: Key, root: i32, seventh: bool) -> Voicing {
    let pc = |d: i32| (key.root as i32 + key.degree_to_semitone(d)).rem_euclid(12);
    let root_pc = pc(root);
    let third = pc(root + 2);
    let fifth = pc(root + 4);
    let sev = pc(root + 6);
    let mut tones = vec![root_pc, third, fifth];
    if seventh {
        tones.push(sev);
    }
    // E shape (root on string 0) or A shape (root on string 1): the lower barre wins.
    let fret_e = (root_pc - OPEN_STRINGS[0]).rem_euclid(12);
    let fret_a = (root_pc - OPEN_STRINGS[1]).rem_euclid(12);
    let (root_string, barre) = if fret_e <= fret_a {
        (0, fret_e)
    } else {
        (1, fret_a)
    };
    let mut v: Voicing = [None; 6];
    v[root_string] = Some(OPEN_STRINGS[root_string] + barre);
    // Each higher string: the chord tone reachable within the hand (barre .. barre + 3),
    // preferring tones that are still missing (3rd, 7th), then the lowest fret.
    let mut have: Vec<i32> = vec![root_pc];
    for s in root_string + 1..6 {
        let mut best: Option<(i32, i32)> = None; // (score, midi)
        for f in barre..=barre + 3 {
            let note = OPEN_STRINGS[s] + f;
            let p = note.rem_euclid(12);
            if !tones.contains(&p) {
                continue;
            }
            let missing = !have.contains(&p);
            let important = p == third || (seventh && p == sev);
            let score = (missing as i32) * 4 + (important as i32) * 2 - (f - barre);
            if best.map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, note));
            }
        }
        if let Some((_, note)) = best {
            v[s] = Some(note);
            have.push(note.rem_euclid(12));
        }
    }
    v
}

/// One stroke of a strumming pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// 16th step in the bar.
    pub step: u8,
    /// Down (low strings first) or up (high strings, fewer of them).
    pub down: bool,
    pub vel: f32,
    /// Length in 16ths before the hand damps the strings.
    pub len: u8,
    /// Muted "chuck": strings dampened by the fretting hand, only a percussive click.
    pub muted: bool,
}

const fn d(step: u8, vel: f32, len: u8) -> Stroke {
    Stroke {
        step,
        down: true,
        vel,
        len,
        muted: false,
    }
}
const fn u(step: u8, vel: f32, len: u8) -> Stroke {
    Stroke {
        step,
        down: false,
        vel,
        len,
        muted: false,
    }
}
const fn m(step: u8, down: bool) -> Stroke {
    Stroke {
        step,
        down,
        vel: 0.55,
        len: 1,
        muted: true,
    }
}

/// Strumming patterns (one bar each).
pub const PATTERNS: [&[Stroke]; 6] = [
    // Disco / house offbeat skank: short down strokes on the offbeats.
    &[d(2, 0.9, 1), d(6, 0.9, 1), d(10, 0.9, 1), d(14, 0.9, 1)],
    // Pop strum: D . D U . U D U
    &[
        d(0, 1.0, 4),
        d(4, 0.75, 2),
        u(6, 0.6, 2),
        u(10, 0.6, 2),
        d(12, 0.8, 2),
        u(14, 0.6, 2),
    ],
    // Funk 16ths: chords on the accents, muted chucks in between.
    &[
        d(0, 0.95, 2),
        m(1, false),
        m(2, true),
        u(3, 0.6, 1),
        m(4, true),
        m(5, false),
        d(6, 0.85, 2),
        m(7, false),
        m(8, true),
        m(9, false),
        d(10, 0.85, 2),
        u(11, 0.6, 1),
        m(12, true),
        m(13, false),
        d(14, 0.9, 2),
        m(15, false),
    ],
    // House chop: syncopated stabs.
    &[u(3, 0.7, 2), d(6, 0.95, 3), u(11, 0.7, 2), d(14, 0.95, 2)],
    // Ballad: two long, slowly rolled down strokes.
    &[d(0, 0.9, 8), d(8, 0.75, 8)],
    // Reggae-ish upstroke chops on 2 and 4 plus a ghost.
    &[m(2, true), u(4, 0.85, 2), m(10, true), u(12, 0.85, 2)],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theory::ScaleKind;

    fn notes(v: &Voicing) -> Vec<i32> {
        v.iter().flatten().copied().collect()
    }

    #[test]
    fn voicings_are_playable_and_complete() {
        for scale in [ScaleKind::Dorian, ScaleKind::NaturalMinor, ScaleKind::Major] {
            for key_root in 0..12u8 {
                let key = Key {
                    root: key_root,
                    scale,
                };
                for root in -3..=3 {
                    for seventh in [false, true] {
                        let v = voicing(key, root, seventh);
                        let n = notes(&v);
                        assert!(n.len() >= 4, "{n:?}");
                        // Lowest note is the root.
                        let root_pc =
                            (key_root as i32 + key.degree_to_semitone(root)).rem_euclid(12);
                        assert_eq!(n[0].rem_euclid(12), root_pc);
                        // Third (and seventh) present.
                        let third =
                            (key_root as i32 + key.degree_to_semitone(root + 2)).rem_euclid(12);
                        assert!(n.iter().any(|x| x.rem_euclid(12) == third), "no 3rd {n:?}");
                        if seventh {
                            let sev =
                                (key_root as i32 + key.degree_to_semitone(root + 6)).rem_euclid(12);
                            assert!(n.iter().any(|x| x.rem_euclid(12) == sev), "no 7th {n:?}");
                        }
                        // A hand spans at most four frets and the chord sits low on the neck.
                        let frets: Vec<i32> = v
                            .iter()
                            .enumerate()
                            .filter_map(|(s, x)| x.map(|x| x - OPEN_STRINGS[s]))
                            .collect();
                        let (lo, hi) = (*frets.iter().min().unwrap(), *frets.iter().max().unwrap());
                        assert!(hi - lo <= 3 && hi <= 10, "{frets:?}");
                        // Notes rise from string to string.
                        assert!(n.windows(2).all(|w| w[1] > w[0] - 12));
                    }
                }
            }
        }
    }

    #[test]
    fn patterns_fit_a_bar() {
        for p in PATTERNS {
            for s in p.iter() {
                assert!(s.step < 16 && s.len >= 1);
            }
        }
    }
}
