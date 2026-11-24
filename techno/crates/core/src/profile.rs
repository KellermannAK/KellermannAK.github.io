//! Style profiles are data (TOML), not code. A profile bounds every random choice the
//! generator makes: tempo, harmony, pattern rules and instrument sound ranges.

use serde::{Deserialize, Serialize};

use crate::rng::Rng;
use crate::structure::StructureRules;
use crate::theory::ScaleKind;

/// Closed interval `[min, max]` sampled uniformly.
pub type Range = [f32; 2];

/// Discrete choice: `values[i]` is picked with probability proportional to `weights[i]`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Weighted<T> {
    pub values: Vec<T>,
    pub weights: Vec<f32>,
}

impl<T: Copy> Weighted<T> {
    pub fn pick(&self, rng: &mut Rng) -> T {
        let n = self.values.len().min(self.weights.len());
        assert!(n > 0, "weighted choice without values");
        self.values[rng.weighted_index(&self.weights[..n])]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub tempo: TempoRules,
    pub harmony: HarmonyRules,
    pub drums: DrumRules,
    pub bass: BassRules,
    pub rumble: RumbleRules,
    pub stab: StabRules,
    pub lead: LeadRules,
    pub pad: PadRules,
    pub mix: MixRules,
    pub structure: StructureRules,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TempoRules {
    pub bpm: Range,
    /// 0 = straight, 1 = full triplet swing on odd 16ths.
    pub swing: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HarmonyRules {
    pub scales: Weighted<ScaleKind>,
    /// Number of roots in the progression (1 = drone on the tonic).
    pub progression_length: Weighted<u8>,
    pub bars_per_root: Weighted<u8>,
    /// Scale degrees allowed for progression roots after the first (which is always 0).
    pub progression_degrees: Weighted<i32>,
    /// Interval weights (in scale steps) for melodic lines.
    pub melody_steps: Weighted<i32>,
    /// Interval weights for the bass; 7 / -7 are octave jumps.
    pub bass_steps: Weighted<i32>,
    /// Probability that a passing (non-scale) tone is allowed. v1 profile: 0.
    pub passing_tone_prob: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HatStyle {
    /// Closed hats on the offbeat 8ths only.
    Offbeat,
    /// Driving 16ths, accented offbeats.
    Sixteenths,
    /// Euclidean hits plus the offbeats.
    Euclid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RideStyle {
    Offbeat,
    Eighths,
    Quarters,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DrumRules {
    pub kick: KickSound,
    pub hats: HatRules,
    pub clap: ClapRules,
    pub snare: SnareSound,
    pub rim: PercRules,
    pub tom: PercRules,
    pub ride: RideRules,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KickSound {
    pub tune_hz: Range,
    /// Fold the key's tonic into `tune_hz` so the kick sits with the bass.
    pub tune_to_key: bool,
    pub pitch_start_hz: Range,
    pub pitch_decay: Range,
    pub decay: Range,
    pub drive: Range,
    pub click: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HatRules {
    pub styles: Weighted<HatStyle>,
    pub euclid_hits: [u32; 2],
    pub open_hat_prob: f32,
    pub tone_hz: Range,
    pub closed_decay: Range,
    pub open_decay: Range,
    pub metal: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClapRules {
    /// Probability of an extra ghost clap per bar position (euclid hits).
    pub ghost_prob: f32,
    pub tone_hz: Range,
    pub decay: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SnareSound {
    pub tone_hz: Range,
    pub decay: Range,
    pub snappy: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PercRules {
    pub hits: [u32; 2],
    pub avoid_kick: bool,
    pub tone_hz: Range,
    pub decay: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RideRules {
    pub styles: Weighted<RideStyle>,
    pub tone_hz: Range,
    pub decay: Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BassRhythm {
    /// Three 16ths after every kick: the classic rolling bass.
    Rolling,
    /// One note on each offbeat 8th.
    Offbeat,
    /// Last two 16ths of each beat.
    Gallop,
    /// Second and fourth 16th of each beat.
    Skippy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BassRules {
    pub rhythms: Weighted<BassRhythm>,
    pub motif_bars: Weighted<u8>,
    /// Allow bass notes exactly on the kick (sidechain feel is lost).
    pub allow_on_kick: bool,
    /// Probability that a note leaves the root at all (bass stays predictable).
    pub movement: Range,
    pub degree_range: [i32; 2],
    pub cutoff_hz: Range,
    pub env_mod: Range,
    pub resonance: Range,
    pub decay: Range,
    pub drive: Range,
    pub sub_mix: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RumbleRules {
    pub amount: Range,
    pub lowpass_hz: Range,
    pub drive: Range,
    pub size: Range,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordShape {
    /// Root, third, fifth.
    Triad,
    /// Root, fifth, octave.
    Power,
    /// Root, third, fifth, seventh.
    Seventh,
    /// Root, fifth, tenth: open "dub techno" voicing.
    Dub,
}

impl ChordShape {
    /// Scale-degree offsets from the chord root (diatonic stacking keeps it in key).
    pub fn degrees(self) -> &'static [i32] {
        match self {
            ChordShape::Triad => &[0, 2, 4],
            ChordShape::Power => &[0, 4, 7],
            ChordShape::Seventh => &[0, 2, 4, 6],
            ChordShape::Dub => &[0, 4, 9],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StabRules {
    pub hits: [u32; 2],
    pub chords: Weighted<ChordShape>,
    pub cutoff_hz: Range,
    pub env_mod: Range,
    pub decay: Range,
    pub detune: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadRules {
    pub motif_bars: Weighted<u8>,
    pub density: Range,
    pub slide_prob: f32,
    pub accent_prob: f32,
    pub degree_range: [i32; 2],
    pub cutoff_hz: Range,
    pub resonance: Range,
    pub env_mod: Range,
    pub decay: Range,
    pub drive: Range,
    /// 0 = saw, 1 = square.
    pub square: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PadRules {
    pub chords: Weighted<ChordShape>,
    pub cutoff_hz: Range,
    pub detune: Range,
    pub attack: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MixRules {
    pub sidechain_depth: Range,
    pub sidechain_release: Range,
}

/// The default v1 profile, embedded so the engine works without file access (WASM).
pub const HARD_TECHNO_SIMPLE_TOML: &str = include_str!("../../../profiles/hard_techno_simple.toml");

impl Profile {
    pub fn from_toml(src: &str) -> Result<Profile, toml::de::Error> {
        toml::from_str(src)
    }

    pub fn hard_techno_simple() -> Profile {
        Profile::from_toml(HARD_TECHNO_SIMPLE_TOML).expect("embedded profile must parse")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_profile_parses() {
        let p = Profile::hard_techno_simple();
        assert_eq!(p.id, "hard_techno_simple");
        assert!(p.tempo.bpm[0] >= 120.0);
    }
}
