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
    pub arp: ArpRules,
    pub drone: DroneRules,
    pub synths: SynthRules,
    pub fx: FxRules,
    pub naming: crate::naming::NamingRules,
    pub variants: VariantRules,
    pub mix: MixRules,
    pub structure: StructureRules,
    /// Piano (house). Optional so older profiles stay valid.
    #[serde(default)]
    pub piano: PianoRules,
    /// Plucked guitar (house).
    #[serde(default)]
    pub guitar: GuitarRules,
}

fn zero_variant() -> Weighted<u8> {
    Weighted {
        values: vec![0],
        weights: vec![1.0],
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeysStyle {
    /// Classic house piano: syncopated chord stabs.
    Stabs,
    /// Offbeat chords, one per beat.
    Offbeat,
    /// One long chord per bar or two (ballad / deep house).
    Held,
}

/// Piano: physically inspired modal synthesis (no samples).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PianoRules {
    pub styles: Weighted<KeysStyle>,
    pub chords: Weighted<ChordShape>,
    /// 0 = dark felt, 1 = bright.
    pub brightness: Range,
    /// Ring time of the low partials in seconds.
    pub decay: Range,
    /// 0 grand, 1 felt, 2 electric (tine), 3 bright house piano.
    pub variants: Weighted<u8>,
}

impl Default for PianoRules {
    fn default() -> Self {
        PianoRules {
            styles: Weighted {
                values: vec![KeysStyle::Stabs],
                weights: vec![1.0],
            },
            chords: Weighted {
                values: vec![ChordShape::Seventh],
                weights: vec![1.0],
            },
            brightness: [0.5, 0.5],
            decay: [1.5, 1.5],
            variants: zero_variant(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuitarStyle {
    /// Muted single-note 16ths (funk / disco house).
    Muted,
    /// Chord strums on the offbeats.
    Strum,
    /// A few sustained notes, like a sparse lead.
    Picked,
}

/// Plucked guitar: Karplus-Strong string model (no samples).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GuitarRules {
    pub styles: Weighted<GuitarStyle>,
    pub chords: Weighted<ChordShape>,
    /// 0 = dull, 1 = bright pick.
    pub brightness: Range,
    /// String sustain in seconds.
    pub sustain: Range,
    /// 0 nylon (soft), 1 steel, 2 clean electric.
    pub variants: Weighted<u8>,
}

impl Default for GuitarRules {
    fn default() -> Self {
        GuitarRules {
            styles: Weighted {
                values: vec![GuitarStyle::Muted],
                weights: vec![1.0],
            },
            chords: Weighted {
                values: vec![ChordShape::Triad],
                weights: vec![1.0],
            },
            brightness: [0.5, 0.5],
            sustain: [1.0, 1.0],
            variants: zero_variant(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickStyle {
    /// Pitch-swept sine into tanh saturation: pounding.
    Hard,
    /// 909-like: short, clicky, little drive.
    Punch,
    /// Long, round, soft: more body, less click.
    Deep,
    /// Hard-clipped and darkened: rave / industrial.
    Distorted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Backbeat {
    Clap,
    Snare,
    ClapSnare,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadKind {
    /// Resonant two-stage low-pass, slides, accents.
    Acid,
    /// Detuned saw stack with a pitch dive into each note.
    Hoover,
    /// Seven detuned saws, open filter.
    Supersaw,
    /// Two-operator FM pluck.
    Pluck,
    /// Soft saw/sine lead with long portamento and delayed vibrato (house glides).
    Glide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadStyle {
    /// Busy riff from a half-bar cell with variation (many notes, Markov melody).
    Riff,
    /// 2-4 notes per bar, at most 3 pitches.
    Sparse,
    /// One pitch, rhythmic (euclidean), accents.
    Pulse,
    /// Few long held notes.
    Long,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArpPattern {
    Up,
    Down,
    UpDown,
    Random,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArpRules {
    pub patterns: Weighted<ArpPattern>,
    /// Steps per note: 1 = 16ths, 2 = 8ths.
    pub rate: Weighted<u8>,
    pub octaves: Weighted<u8>,
    pub chords: Weighted<ChordShape>,
    pub cutoff_hz: Range,
    pub env_mod: Range,
    pub decay: Range,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DroneRules {
    pub cutoff_hz: Range,
    pub drive: Range,
    pub detune: Range,
}

/// Sound variant per instrument (0 = original). Each instrument's synthesis code defines
/// what its variants sound like; the profile only weights them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VariantRules {
    pub clap: Weighted<u8>,
    pub snare: Weighted<u8>,
    pub hats: Weighted<u8>,
    pub ride: Weighted<u8>,
    pub crash: Weighted<u8>,
    pub rim: Weighted<u8>,
    pub tom: Weighted<u8>,
    pub bass: Weighted<u8>,
    pub rumble: Weighted<u8>,
    pub stab: Weighted<u8>,
    pub arp: Weighted<u8>,
    pub pad: Weighted<u8>,
    pub drone: Weighted<u8>,
}

/// Embedded house profile.
pub const HOUSE_TOML: &str = include_str!("../../../profiles/house.toml");

/// Genres the engine knows, in the order of the site's genre switcher.
pub const GENRES: [&str; 2] = ["techno", "house"];

/// Insert effect on a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertKind {
    None,
    Chorus,
    Phaser,
    Crusher,
    Tremolo,
    Tape,
}

/// Which insert effects a song uses and how strongly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FxRules {
    /// On the short synths (lead / arp / stab).
    pub synth_insert: Weighted<InsertKind>,
    /// On the long layer (pad / drone).
    pub layer_insert: Weighted<InsertKind>,
    /// Probability of phased hi-hats.
    pub hat_phaser_prob: f32,
    /// LFO rate for chorus / phaser in Hz.
    pub lfo_rate: Range,
    /// Tremolo cycle length in beats.
    pub tremolo_beats: Weighted<f32>,
    pub depth: Range,
    /// Effect intensity multiplier in breakdowns.
    pub breakdown_boost: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortSynth {
    Lead,
    Arp,
    Stab,
    Piano,
    Guitar,
}

impl ShortSynth {
    pub const fn track(self) -> crate::tracks::Track {
        use crate::tracks::Track;
        match self {
            ShortSynth::Lead => Track::Lead,
            ShortSynth::Arp => Track::Arp,
            ShortSynth::Stab => Track::Stab,
            ShortSynth::Piano => Track::Piano,
            ShortSynth::Guitar => Track::Guitar,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LongLayer {
    Pad,
    Drone,
    None,
}

/// Which synths a song uses and how they may overlap.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthRules {
    /// The one short synth that carries the song.
    pub main: Weighted<ShortSynth>,
    /// Probability that a different short synth takes over in breakdowns. It replaces the
    /// main synth there, it never plays at the same time.
    pub breakdown_alt_prob: f32,
    /// At most one sustained background layer.
    pub long_layer: Weighted<LongLayer>,
    /// Notes per stab hit: 1 = single tone, more = chords.
    pub stab_notes: u8,
    /// Second instrument that may play together with the main one (house: guitar under
    /// piano). Techno has none.
    #[serde(default)]
    pub companion: Option<Weighted<ShortSynth>>,
    #[serde(default)]
    pub companion_prob: f32,
    /// Energy from which the companion joins grooves and drops.
    #[serde(default = "default_companion_energy")]
    pub companion_energy: f32,
}

fn default_companion_energy() -> f32 {
    0.62
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
    /// Shape of the root movement (drone = tonic only).
    pub progression: Weighted<ProgressionShape>,
    /// Where root movement happens: everywhere, or only in breakdowns (tonic elsewhere).
    pub harmony_scope: Weighted<HarmonyScope>,
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
pub enum ProgressionShape {
    /// Tonic only.
    Drone,
    /// Tonic and one other root, alternating.
    Two,
    /// One excursion, then home: 0 x 0 0.
    Return,
    /// Turn at the end of the phrase: 0 0 0 x.
    Turn,
    /// Four different roots (very melodic, rare in dark techno).
    Four,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarmonyScope {
    Everywhere,
    BreakdownOnly,
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
    /// What plays on 2 and 4.
    pub backbeat: Weighted<Backbeat>,
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
    pub styles: Weighted<KickStyle>,
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
    /// 16th steps a ghost clap may use. Unset: the techno spots (incl. right before the
    /// backbeat). House leaves those out: with swing they sound like a sloppy double clap.
    #[serde(default)]
    pub ghost_spots: Option<Vec<u32>>,
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
    /// Alternates per beat: last two 16ths, then only the second 16th.
    Syncopated,
    /// Few notes: offbeat on beats 1 and 3, two 16ths before the next bar.
    Sparse,
    /// House bounce: offbeat, push before beat 3, answer at the end of the bar.
    Funky,
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
    /// Root, third, fifth, ninth: the warm house chord.
    Ninth,
}

impl ChordShape {
    /// Scale-degree offsets from the chord root (diatonic stacking keeps it in key).
    pub fn degrees(self) -> &'static [i32] {
        match self {
            ChordShape::Triad => &[0, 2, 4],
            ChordShape::Power => &[0, 4, 7],
            ChordShape::Seventh => &[0, 2, 4, 6],
            ChordShape::Dub => &[0, 4, 9],
            ChordShape::Ninth => &[0, 2, 4, 8],
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
    pub kinds: Weighted<LeadKind>,
    /// Melodic style of the lead motif.
    pub styles: Weighted<LeadStyle>,
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

    pub fn house() -> Profile {
        Profile::from_toml(HOUSE_TOML).expect("embedded house profile must parse")
    }

    /// Embedded profile by genre name ("techno", "house") or profile id.
    pub fn by_genre(genre: &str) -> Option<Profile> {
        match genre {
            "techno" | "hard_techno_simple" => Some(Profile::hard_techno_simple()),
            "house" => Some(Profile::house()),
            _ => None,
        }
    }

    /// Short genre name of this profile.
    pub fn genre(&self) -> &str {
        if self.naming.library.is_empty() {
            "techno"
        } else {
            &self.naming.library
        }
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
        assert_eq!(p.genre(), "techno");
    }

    #[test]
    fn house_profile_parses() {
        let p = Profile::house();
        assert_eq!(p.genre(), "house");
        assert!(p.tempo.bpm[1] < Profile::hard_techno_simple().tempo.bpm[0]);
        for g in GENRES {
            assert_eq!(Profile::by_genre(g).unwrap().genre(), g);
        }
    }
}
