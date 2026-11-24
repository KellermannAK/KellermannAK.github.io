//! Keys, scales, Camelot codes. Pitches are stored as scale degrees relative to the key;
//! transposition is just a different `Key`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScaleKind {
    NaturalMinor,
    Phrygian,
    HarmonicMinor,
}

impl ScaleKind {
    pub const ALL: [ScaleKind; 3] = [
        ScaleKind::NaturalMinor,
        ScaleKind::Phrygian,
        ScaleKind::HarmonicMinor,
    ];

    pub const fn intervals(self) -> [i32; 7] {
        match self {
            ScaleKind::NaturalMinor => [0, 2, 3, 5, 7, 8, 10],
            ScaleKind::Phrygian => [0, 1, 3, 5, 7, 8, 10],
            ScaleKind::HarmonicMinor => [0, 2, 3, 5, 7, 8, 11],
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            ScaleKind::NaturalMinor => "minor",
            ScaleKind::Phrygian => "phrygian",
            ScaleKind::HarmonicMinor => "harmonic minor",
        }
    }
}

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    /// Pitch class of the tonic, 0 = C.
    pub root: u8,
    pub scale: ScaleKind,
}

impl Key {
    /// Semitone offset from the tonic for a (possibly negative / multi-octave) scale degree.
    pub fn degree_to_semitone(self, degree: i32) -> i32 {
        let iv = self.scale.intervals();
        let oct = degree.div_euclid(7);
        let idx = degree.rem_euclid(7) as usize;
        oct * 12 + iv[idx]
    }

    /// True if the pitch class (0..12, absolute) belongs to the scale.
    pub fn contains_pitch_class(self, pc: i32) -> bool {
        let rel = (pc - self.root as i32).rem_euclid(12);
        self.scale.intervals().contains(&rel)
    }

    /// Camelot wheel code. All v1 scales are minor-type, so they map to the "A" ring.
    pub fn camelot(self) -> (u8, char) {
        let n = ((self.root as i32 - 9) * 7).rem_euclid(12) + 8;
        (((n - 1) % 12 + 1) as u8, 'A')
    }

    pub fn camelot_code(self) -> String {
        let (n, l) = self.camelot();
        format!("{n}{l}")
    }

    /// Key `steps` positions around the Camelot wheel (same ring). +1 = a fifth up.
    pub fn camelot_step(self, steps: i32) -> Key {
        Key {
            root: (self.root as i32 + 7 * steps).rem_euclid(12) as u8,
            scale: self.scale,
        }
    }

    pub fn name(self) -> String {
        format!(
            "{} {}",
            NOTE_NAMES[self.root as usize % 12],
            self.scale.name()
        )
    }
}

pub fn midi_to_hz(midi: f32) -> f32 {
    440.0 * libm_exp2((midi - 69.0) / 12.0)
}

#[inline]
fn libm_exp2(x: f32) -> f32 {
    (x * core::f32::consts::LN_2).exp()
}

/// Intervals (semitones mod 12) that rub against each other in a dense techno mix.
pub fn is_harsh_interval(semitones: i32) -> bool {
    matches!(semitones.rem_euclid(12), 1 | 2 | 6 | 10 | 11)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camelot_minor_ring() {
        let k = |root| Key {
            root,
            scale: ScaleKind::NaturalMinor,
        };
        assert_eq!(k(9).camelot_code(), "8A"); // A minor
        assert_eq!(k(4).camelot_code(), "9A"); // E minor
        assert_eq!(k(8).camelot_code(), "1A"); // G# / Ab minor
        assert_eq!(k(0).camelot_code(), "5A"); // C minor
        assert_eq!(k(6).camelot_code(), "11A"); // F# minor
    }

    #[test]
    fn camelot_steps_are_neighbours() {
        let k = Key {
            root: 9,
            scale: ScaleKind::NaturalMinor,
        }; // 8A
        assert_eq!(k.camelot_step(1).camelot_code(), "9A");
        assert_eq!(k.camelot_step(-1).camelot_code(), "7A");
        assert_eq!(k.camelot_step(12), k);
    }

    #[test]
    fn degrees_wrap_octaves() {
        let k = Key {
            root: 0,
            scale: ScaleKind::NaturalMinor,
        };
        assert_eq!(k.degree_to_semitone(0), 0);
        assert_eq!(k.degree_to_semitone(7), 12);
        assert_eq!(k.degree_to_semitone(-1), -2);
        assert_eq!(k.degree_to_semitone(-3), -5); // fifth below
        assert_eq!(k.degree_to_semitone(4), 7);
    }
}
