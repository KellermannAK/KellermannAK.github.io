//! Song DNA: the main idea of a song, generated once from its seed and kept for the
//! whole song. Variation later only transforms the DNA, it never re-invents it.
//!
//! The DNA is a plain `Copy` struct with fixed-size arrays, so it can be handed to the
//! audio thread without heap traffic. Generating it may allocate briefly; that happens
//! ahead of time (the next song is pre-generated), never inside the render callback.

use crate::euclid::{euclid, Pattern};
use crate::markov::next_degree;
use crate::profile::{
    ArpPattern, Backbeat, BassRhythm, ChordShape, GuitarStyle, HarmonyScope, HatStyle, InsertKind,
    KeysStyle, KickStyle, LeadKind, LeadStyle, LongLayer, Profile, ProgressionShape, RideStyle,
    ShortSynth,
};
use crate::rng::Rng;
use crate::structure::{SectionKind, SongPlan, SHORT_SYNTHS};
use crate::theory::{is_harsh_interval, midi_to_hz, Key, ScaleKind};
use crate::time::STEPS_PER_BAR;
use crate::tracks::{Track, TrackSet};

pub const MAX_MOTIF_STEPS: usize = 32;
pub const MAX_PROGRESSION: usize = 4;

const KICK_STEPS: Pattern = Pattern::four_on_the_floor();
const OFFBEATS: Pattern = Pattern::from_mask(0x4444, 16);
const BACKBEAT: Pattern = Pattern::from_mask(0x1010, 16);

/// One note of a pitched motif. `degree` is relative to the current progression root.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    pub degree: i8,
    pub velocity: f32,
    pub accent: bool,
    /// Glide into the following note (legato, no retrigger).
    pub slide: bool,
    /// Length in 16th steps.
    pub len: u8,
}

impl Note {
    fn new(degree: i32, velocity: f32) -> Note {
        Note {
            degree: degree as i8,
            velocity,
            accent: false,
            slide: false,
            len: 1,
        }
    }
}

/// A looping pitched pattern of 16 or 32 steps.
#[derive(Clone, Copy, Debug)]
pub struct Motif {
    pub notes: [Option<Note>; MAX_MOTIF_STEPS],
    pub len: u8,
}

impl Motif {
    pub fn empty(len: u8) -> Motif {
        Motif {
            notes: [None; MAX_MOTIF_STEPS],
            len,
        }
    }

    /// Note at an absolute 16th step (the motif loops).
    #[inline]
    pub fn at(&self, step: u32) -> Option<Note> {
        self.notes[(step % self.len as u32) as usize]
    }

    pub fn note_count(&self) -> usize {
        self.notes[..self.len as usize].iter().flatten().count()
    }

    pub fn to_string_grid(&self) -> String {
        let mut s = String::new();
        for i in 0..self.len as usize {
            if i > 0 && i % 16 == 0 {
                s.push('|');
            }
            match self.notes[i] {
                None => s.push('.'),
                Some(n) => {
                    let c = match n.degree {
                        d @ 0..=9 => (b'0' + d as u8) as char,
                        d if d < 0 => (b'a' + (-d - 1).min(25) as u8) as char,
                        _ => '+',
                    };
                    s.push(c);
                }
            }
        }
        s
    }
}

/// Root movement in scale degrees; `roots[0]` is always the tonic.
#[derive(Clone, Copy, Debug)]
pub struct Progression {
    pub roots: [i8; MAX_PROGRESSION],
    pub count: u8,
    pub bars_per_root: u8,
    /// Roots only move in breakdowns; everywhere else the tonic is held.
    pub breakdown_only: bool,
}

impl Progression {
    #[inline]
    pub fn root_at_bar(&self, bar: u32) -> i32 {
        let idx = (bar / self.bars_per_root.max(1) as u32) % self.count.max(1) as u32;
        self.roots[idx as usize] as i32
    }

    pub fn roots(&self) -> &[i8] {
        &self.roots[..self.count as usize]
    }

    pub fn cycle_bars(&self) -> u32 {
        self.count as u32 * self.bars_per_root as u32
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DrumPatterns {
    pub backbeat: Backbeat,
    pub kick: Pattern,
    pub hat_closed: Pattern,
    pub hat_accent: Pattern,
    pub hat_open: Pattern,
    pub clap: Pattern,
    pub clap_ghost: Pattern,
    pub rim: Pattern,
    pub tom: Pattern,
    pub ride: Pattern,
    pub hat_style: HatStyle,
    pub ride_style: RideStyle,
}

// ----- Sound parameters (sampled once per song within profile ranges) -----

#[derive(Clone, Copy, Debug)]
pub struct KickParams {
    pub style: KickStyle,
    pub tune_hz: f32,
    pub pitch_start_hz: f32,
    pub pitch_decay: f32,
    pub decay: f32,
    pub drive: f32,
    pub click: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HatParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub tone_hz: f32,
    pub closed_decay: f32,
    pub open_decay: f32,
    pub metal: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ToneParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub tone_hz: f32,
    pub decay: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SnareParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub tone_hz: f32,
    pub decay: f32,
    pub snappy: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BassParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub rhythm: BassRhythm,
    pub cutoff_hz: f32,
    pub env_mod: f32,
    pub resonance: f32,
    pub decay: f32,
    pub drive: f32,
    pub sub_mix: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RumbleParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub amount: f32,
    pub lowpass_hz: f32,
    pub drive: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct StabParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub chord: ChordShape,
    /// Notes per hit (1 = single tone).
    pub notes: u8,
    pub cutoff_hz: f32,
    pub env_mod: f32,
    pub decay: f32,
    pub detune: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LeadParams {
    pub kind: LeadKind,
    pub style: LeadStyle,
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay: f32,
    pub drive: f32,
    pub square: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PadParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub chord: ChordShape,
    pub cutoff_hz: f32,
    pub detune: f32,
    pub attack: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct KeysParams {
    pub variant: u8,
    pub style: KeysStyle,
    pub chord: ChordShape,
    pub brightness: f32,
    pub decay: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GuitarParams {
    pub variant: u8,
    pub style: GuitarStyle,
    pub chord: ChordShape,
    pub brightness: f32,
    pub sustain: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MixParams {
    pub sidechain_depth: f32,
    pub sidechain_release: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct DroneParams {
    /// Sound variant (0 = original), see the instrument's docs.
    pub variant: u8,
    pub cutoff_hz: f32,
    pub drive: f32,
    pub detune: f32,
}

/// One insert effect as chosen for a song.
#[derive(Clone, Copy, Debug)]
pub struct InsertSpec {
    pub kind: InsertKind,
    /// LFO rate (Hz) or tremolo cycle (beats).
    pub rate: f32,
    pub depth: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct FxParams {
    pub synth: InsertSpec,
    pub layer: InsertSpec,
    pub hats: InsertSpec,
}

#[derive(Clone, Copy, Debug)]
pub struct SoundParams {
    pub fx: FxParams,
    pub crash_variant: u8,
    /// Arp voice reuses the stab synth; `chord` is the arpeggiated chord.
    pub arp: StabParams,
    pub drone: DroneParams,
    pub kick: KickParams,
    pub hats: HatParams,
    pub clap: ToneParams,
    pub snare: SnareParams,
    pub rim: ToneParams,
    pub tom: ToneParams,
    pub ride: ToneParams,
    pub bass: BassParams,
    pub rumble: RumbleParams,
    pub stab: StabParams,
    pub lead: LeadParams,
    pub pad: PadParams,
    pub mix: MixParams,
    pub piano: KeysParams,
    pub guitar: GuitarParams,
}

/// Everything that defines a song's identity.
#[derive(Clone, Copy, Debug)]
pub struct SongDna {
    pub seed: u64,
    pub bpm: f32,
    pub swing: f32,
    pub key: Key,
    pub progression: Progression,
    pub drums: DrumPatterns,
    pub bass: Motif,
    pub stab: Motif,
    pub lead: Motif,
    pub arp: Motif,
    /// Piano chord rhythm (degree = inversion offset, chords follow the progression).
    pub piano: Motif,
    /// Guitar: muted notes, strums (chords) or picked notes depending on the style.
    pub guitar: Motif,
    pub sound: SoundParams,
    /// Song structure, fixed at song start.
    pub plan: SongPlan,
}

/// Overrides for songs inside a set: the running song dictates tempo, the Camelot
/// wheel dictates the key.
#[derive(Clone, Copy, Debug, Default)]
pub struct Constraints {
    pub bpm: Option<f32>,
    pub key_root: Option<u8>,
}

/// MIDI note of the bass tonic: folded into E1..D#2 (41-78 Hz) for a solid low end.
pub fn bass_base_midi(key: Key) -> i32 {
    28 + (key.root as i32 - 4).rem_euclid(12)
}

impl SongDna {
    /// Title and artist (see `naming`).
    pub fn name(&self, profile: &Profile) -> crate::naming::SongName {
        crate::naming::name(self, &profile.naming)
    }

    /// Progression root (scale degree) at a bar, honouring the harmony scope.
    #[inline]
    pub fn root_at(&self, bar: u32) -> i32 {
        if self.progression.breakdown_only
            && self.plan.section_at(bar).kind != SectionKind::Breakdown
        {
            return 0;
        }
        self.progression.root_at_bar(bar)
    }

    pub fn generate(seed: u64, profile: &Profile) -> SongDna {
        SongDna::generate_with(seed, profile, Constraints::default())
    }

    pub fn generate_with(seed: u64, profile: &Profile, c: Constraints) -> SongDna {
        let root = Rng::new(seed);

        let mut r = root.fork("tempo");
        let mut bpm = r.in_range(profile.tempo.bpm).round();
        let swing = r.in_range(profile.tempo.swing);
        if let Some(b) = c.bpm {
            bpm = b;
        }

        let mut r = root.fork("key");
        let mut key = Key {
            root: r.range_i32(0, 11) as u8,
            scale: profile.harmony.scales.pick(&mut r),
        };
        if let Some(k) = c.key_root {
            key.root = k % 12;
        }

        let progression = gen_progression(&mut root.fork("progression"), profile, key);
        let drums = gen_drums(&mut root.fork("drums"), profile);
        let mut sound = gen_sound(&mut root.fork("sound"), profile, key);
        let stab = gen_stab(&mut root.fork("stab"), profile);
        let (mut bass, bass_rhythm) = gen_bass(&mut root.fork("bass"), profile);
        fit_bass_to_stabs(&mut bass, &stab, sound.stab.chord);
        let mut lead = gen_lead(&mut root.fork("lead"), profile, key, &bass, &progression);
        let mut ls = root.fork("lead-style");
        let lead_style = profile.lead.styles.pick(&mut ls);
        sound.lead.style = lead_style;
        sound.bass.rhythm = bass_rhythm;
        if lead_style != LeadStyle::Riff {
            lead = gen_lead_styled(&mut ls, lead_style, key, &bass, &progression);
        }

        let mut drums = drums;
        // Sound variety and the instrument palette come from their own streams, so
        // adding them did not change the core sound of existing seeds.
        let mut v = root.fork("variety");
        sound.kick.style = profile.drums.kick.styles.pick(&mut v);
        sound.lead.kind = profile.lead.kinds.pick(&mut v);
        drums.backbeat = profile.drums.backbeat.pick(&mut v);
        // One short synth carries the song; another may only replace it in breakdowns.
        let sr = &profile.synths;
        let to_track = |s: ShortSynth| s.track();
        let main_synth = to_track(sr.main.pick(&mut v));
        let alt_synth = if v.chance(sr.breakdown_alt_prob) {
            let others: Vec<(ShortSynth, f32)> = sr
                .main
                .values
                .iter()
                .zip(&sr.main.weights)
                .filter(|(s, _)| to_track(**s) != main_synth)
                .map(|(s, w)| (*s, *w))
                .collect();
            let w: Vec<f32> = others.iter().map(|o| o.1).collect();
            (!others.is_empty()).then(|| to_track(others[v.weighted_index(&w)].0))
        } else {
            None
        };
        let long_layer = match sr.long_layer.pick(&mut v) {
            LongLayer::Pad => Some(Track::Pad),
            LongLayer::Drone => Some(Track::Drone),
            LongLayer::None => None,
        };
        let mut palette = TrackSet::ALL
            .minus(SHORT_SYNTHS)
            .minus(TrackSet::of(&[Track::Pad, Track::Drone]));
        palette = palette.with(main_synth);
        if let Some(t) = alt_synth {
            palette = palette.with(t);
        }
        if let Some(t) = long_layer {
            palette = palette.with(t);
        }
        // Genres without a rumble (house) leave it out completely.
        if profile.rumble.amount[1] <= 0.0 {
            palette = palette.without(Track::Rumble);
        }
        // Companion instrument (own stream; techno has none).
        let mut cr = root.fork("companion");
        let companion = sr
            .companion
            .as_ref()
            .filter(|_| cr.chance(sr.companion_prob))
            .map(|w| to_track(w.pick(&mut cr)))
            .filter(|t| *t != main_synth);
        if let Some(t) = companion {
            palette = palette.with(t);
        }
        // Piano and guitar (own streams, so techno seeds are unchanged).
        let (piano, piano_sound) = gen_piano(&mut root.fork("piano"), profile);
        sound.piano = piano_sound;
        let (guitar, guitar_sound) =
            gen_guitar(&mut root.fork("guitar"), profile, key, &bass, &progression);
        sound.guitar = guitar_sound;
        // Glide leads slide between neighbouring notes: the electronic house glide.
        if sound.lead.kind == LeadKind::Glide {
            let len = lead.len as usize;
            for i in 0..len {
                let next = (1..=4)
                    .map(|k| (i + k) % len)
                    .find(|j| lead.notes[*j].is_some());
                if let (Some(j), Some(n)) = (next, lead.notes[i].as_mut()) {
                    if j > i {
                        n.slide = true;
                        n.len = (j - i) as u8;
                    }
                }
            }
        }
        sound.stab.notes = sr.stab_notes.max(1);
        sound.fx = gen_fx(&mut root.fork("fx"), profile);
        // Instrument sound variants (own stream).
        let vr = &profile.variants;
        let mut vs = root.fork("variants");
        sound.clap.variant = vr.clap.pick(&mut vs);
        sound.snare.variant = vr.snare.pick(&mut vs);
        sound.hats.variant = vr.hats.pick(&mut vs);
        sound.ride.variant = vr.ride.pick(&mut vs);
        sound.crash_variant = vr.crash.pick(&mut vs);
        sound.rim.variant = vr.rim.pick(&mut vs);
        sound.tom.variant = vr.tom.pick(&mut vs);
        sound.bass.variant = vr.bass.pick(&mut vs);
        sound.rumble.variant = vr.rumble.pick(&mut vs);
        sound.stab.variant = vr.stab.pick(&mut vs);
        sound.arp.variant = vr.arp.pick(&mut vs);
        sound.pad.variant = vr.pad.pick(&mut vs);
        sound.drone.variant = vr.drone.pick(&mut vs);
        let (arp, arp_sound) = gen_arp(&mut root.fork("arp"), profile);
        sound.arp = arp_sound;
        let mut r = root.fork("drone");
        sound.drone = DroneParams {
            variant: 0,
            cutoff_hz: r.in_range(profile.drone.cutoff_hz),
            drive: r.in_range(profile.drone.drive),
            detune: r.in_range(profile.drone.detune),
        };
        let mut plan = SongPlan::generate(
            &mut root.fork("structure"),
            profile,
            palette,
            main_synth,
            alt_synth,
        );
        plan.companion = companion;

        SongDna {
            seed,
            bpm,
            swing,
            key,
            progression,
            drums,
            bass,
            stab,
            lead,
            arp,
            piano,
            guitar,
            sound,
            plan,
        }
    }

    /// Multi-line human-readable summary (used by the CLI and the debug view).
    pub fn describe(&self) -> String {
        let d = &self.drums;
        let roots: Vec<String> = self
            .progression
            .roots()
            .iter()
            .map(|r| r.to_string())
            .collect();
        format!(
            "seed       {}\n\
             tempo      {} BPM, swing {:.2}\n\
             key        {} ({})\n\
             roots      [{}] x {} bars{}\n\
             kick       {}   tune {:.1} Hz, drive {:.1}\n\
             hat closed {}   style {:?}\n\
             hat open   {}\n\
             clap       {}   ghost {}\n\
             rim        {}\n\
             tom        {}\n\
             ride       {}   style {:?}\n\
             bass       {}\n\
             stab       {}   chord {:?}\n\
             lead       {}\n\
             pad        chord {:?}\n\
             arp        {}   chord {:?}\n\
             sounds     kick {:?}, lead {:?} ({:?}), bass {:?}, backbeat {:?}\n\
             fx         synth {:?}, layer {:?}, hats {:?}\n\
             variants   clap {} snare {} hats {} ride {} crash {} rim {} tom {} bass {} rumble {} stab {} arp {} pad {} drone {}\n\
             synths     main {}, breakdown {}, companion {}\n\
             piano      {}   {:?} {:?} v{}\n\
             guitar     {}   {:?} {:?} v{}\n\
             palette    {}\n\
             (motif digits = scale degree above root, a/b/c.. = 1/2/3.. below)",
            self.seed,
            self.bpm,
            self.swing,
            self.key.name(),
            self.key.camelot_code(),
            roots.join(", "),
            self.progression.bars_per_root,
            if self.progression.breakdown_only {
                " (breakdowns only)"
            } else {
                ""
            },
            d.kick.to_string_grid(),
            self.sound.kick.tune_hz,
            self.sound.kick.drive,
            d.hat_closed.to_string_grid(),
            d.hat_style,
            d.hat_open.to_string_grid(),
            d.clap.to_string_grid(),
            d.clap_ghost.to_string_grid(),
            d.rim.to_string_grid(),
            d.tom.to_string_grid(),
            d.ride.to_string_grid(),
            d.ride_style,
            self.bass.to_string_grid(),
            self.stab.to_string_grid(),
            self.sound.stab.chord,
            self.lead.to_string_grid(),
            self.sound.pad.chord,
            self.arp.to_string_grid(),
            self.sound.arp.chord,
            self.sound.kick.style,
            self.sound.lead.kind,
            self.sound.lead.style,
            self.sound.bass.rhythm,
            self.drums.backbeat,
            self.sound.fx.synth.kind,
            self.sound.fx.layer.kind,
            self.sound.fx.hats.kind,
            self.sound.clap.variant,
            self.sound.snare.variant,
            self.sound.hats.variant,
            self.sound.ride.variant,
            self.sound.crash_variant,
            self.sound.rim.variant,
            self.sound.tom.variant,
            self.sound.bass.variant,
            self.sound.rumble.variant,
            self.sound.stab.variant,
            self.sound.arp.variant,
            self.sound.pad.variant,
            self.sound.drone.variant,
            self.plan.main_synth.name(),
            self.plan.alt_synth.map(|t| t.name()).unwrap_or("same"),
            self.plan.companion.map(|t| t.name()).unwrap_or("none"),
            self.piano.to_string_grid(),
            self.sound.piano.style,
            self.sound.piano.chord,
            self.sound.piano.variant,
            self.guitar.to_string_grid(),
            self.sound.guitar.style,
            self.sound.guitar.chord,
            self.sound.guitar.variant,
            self.plan
                .palette
                .iter()
                .map(|t| t.name())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

// ---------------------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------------------

fn gen_progression(rng: &mut Rng, p: &Profile, key: Key) -> Progression {
    let h = &p.harmony;
    let mut shape_rng = rng.fork("shape");
    let shape = h.progression.pick(&mut shape_rng);
    let breakdown_only = h.harmony_scope.pick(&mut shape_rng) == HarmonyScope::BreakdownOnly;
    let count = match shape {
        ProgressionShape::Drone => 1,
        ProgressionShape::Two => 2,
        _ => 4,
    };
    let bars_per_root = h.bars_per_root.pick(rng).max(1);
    let mut roots = [0i8; MAX_PROGRESSION];

    // Degrees that would produce an ugly root chord in this scale are filtered out:
    // bII only exists as a strong colour in phrygian, VII in harmonic minor is diminished.
    let allowed = |d: i32| match d {
        // bII in phrygian; ii is a minor chord in major and dorian (diminished in minor).
        1 => matches!(
            key.scale,
            ScaleKind::Phrygian | ScaleKind::Major | ScaleKind::Dorian
        ),
        // vi is diminished in dorian.
        5 => key.scale != ScaleKind::Dorian,
        // VII is diminished in harmonic minor and major.
        6 => !matches!(key.scale, ScaleKind::HarmonicMinor | ScaleKind::Major),
        _ => true,
    };

    for i in 1..count {
        let weights: Vec<f32> = h
            .progression_degrees
            .values
            .iter()
            .zip(&h.progression_degrees.weights)
            .map(|(&d, &w)| {
                if allowed(d) && d != roots[i - 1] as i32 {
                    w
                } else {
                    0.0
                }
            })
            .collect();
        let idx = rng.weighted_index(&weights);
        roots[i] = if weights.iter().any(|w| *w > 0.0) {
            h.progression_degrees.values[idx] as i8
        } else {
            0
        };
    }
    // Shapes with a single excursion keep the tonic in the other slots.
    match shape {
        ProgressionShape::Return => {
            roots[2] = 0;
            roots[3] = 0;
        }
        ProgressionShape::Turn => {
            roots[3] = roots[1];
            roots[1] = 0;
            roots[2] = 0;
        }
        _ => {}
    }
    Progression {
        roots,
        count: count as u8,
        bars_per_root,
        breakdown_only: breakdown_only && count > 1,
    }
}

fn gen_drums(rng: &mut Rng, p: &Profile) -> DrumPatterns {
    let d = &p.drums;

    let hat_style = d.hats.styles.pick(rng);
    let open = rng.chance(d.hats.open_hat_prob);
    let (mut hat_closed, hat_open) = match hat_style {
        HatStyle::Sixteenths => (
            Pattern::from_mask(0xFFFF, 16),
            if open { OFFBEATS } else { Pattern::empty(16) },
        ),
        HatStyle::Offbeat => (
            OFFBEATS,
            if open {
                Pattern::from_mask(1 << 14, 16)
            } else {
                Pattern::empty(16)
            },
        ),
        HatStyle::Euclid => {
            let k =
                rng.range_i32(d.hats.euclid_hits[0] as i32, d.hats.euclid_hits[1] as i32) as u32;
            let rot = rng.range_i32(0, 15);
            let base = euclid(k, 16, rot);
            (base, if open { OFFBEATS } else { Pattern::empty(16) })
        }
    };
    // An open hat is choked by a closed one on the same step, so never play both.
    hat_closed = hat_closed.without(hat_open);

    let mut clap_ghost = Pattern::empty(16);
    if rng.chance(d.clap.ghost_prob) {
        const GHOST_SPOTS: [u32; 5] = [15, 7, 14, 11, 3];
        let s = GHOST_SPOTS[rng.range_i32(0, GHOST_SPOTS.len() as i32 - 1) as usize];
        clap_ghost = Pattern::from_mask(1 << s, 16);
    }

    let perc = |rng: &mut Rng, rules: &crate::profile::PercRules, avoid: Pattern| {
        let k = rng.range_i32(rules.hits[0] as i32, rules.hits[1] as i32) as u32;
        let rot = rng.range_i32(0, 15);
        let mut pat = euclid(k, 16, rot);
        if rules.avoid_kick {
            pat = pat.without(KICK_STEPS);
        }
        pat.without(avoid)
    };
    let rim = perc(rng, &d.rim, BACKBEAT);
    let tom = perc(rng, &d.tom, BACKBEAT.union(rim));

    let ride_style = d.ride.styles.pick(rng);
    let ride = match ride_style {
        RideStyle::Offbeat => OFFBEATS,
        RideStyle::Eighths => Pattern::from_mask(0x5555, 16),
        RideStyle::Quarters => KICK_STEPS,
    };

    DrumPatterns {
        backbeat: Backbeat::Clap,
        kick: KICK_STEPS,
        hat_closed,
        hat_accent: OFFBEATS,
        hat_open,
        clap: BACKBEAT,
        clap_ghost,
        rim,
        tom,
        ride,
        hat_style,
        ride_style,
    }
}

/// 16th positions (0..4) inside beat `beat` (0..4) where the bass plays.
fn bass_positions(r: BassRhythm, beat: u32) -> &'static [u32] {
    match r {
        BassRhythm::Rolling => &[1, 2, 3],
        BassRhythm::Offbeat => &[2],
        BassRhythm::Gallop => &[2, 3],
        BassRhythm::Skippy => &[1, 3],
        BassRhythm::Syncopated => {
            if beat % 2 == 0 {
                &[2, 3]
            } else {
                &[1]
            }
        }
        BassRhythm::Sparse => match beat {
            0 | 2 => &[2],
            3 => &[2, 3],
            _ => &[],
        },
        BassRhythm::Funky => match beat {
            0 => &[2, 3],
            1 => &[2],
            2 => &[3],
            _ => &[1, 3],
        },
    }
}

fn gen_bass(rng: &mut Rng, p: &Profile) -> (Motif, BassRhythm) {
    let b = &p.bass;
    let bars = b.motif_bars.pick(rng).clamp(1, 2) as u32;
    let len = bars * STEPS_PER_BAR;
    let rhythm = b.rhythms.pick(rng);
    let movement = rng.in_range(b.movement);
    let [lo, hi] = b.degree_range;

    let mut m = Motif::empty(len as u8);
    let mut prev = 0;
    let mut last_idx = 0;
    for step in 0..len {
        let pos = step % 4;
        let positions = bass_positions(rhythm, (step / 4) % 4);
        let on = positions.contains(&pos) || (b.allow_on_kick && pos == 0);
        if !on {
            continue;
        }
        // Strong positions (first note after the kick on beats 1 and 3) stay on the root.
        let first_pos = positions.first().copied().unwrap_or(pos);
        let strong = pos == first_pos && (step % 8) < 4;
        let degree = if strong {
            0
        } else if rng.chance(movement) {
            next_degree(rng, prev, &p.harmony.bass_steps, lo, hi, 0.3)
        } else {
            0
        };
        let velocity = if pos == 2 { 1.0 } else { 0.82 };
        let mut n = Note::new(degree, velocity);
        n.accent = pos == 2;
        m.notes[step as usize] = Some(n);
        prev = degree;
        last_idx = step as usize;
    }
    // Phrase ending resolves to root or fifth.
    if let Some(n) = m.notes[last_idx].as_mut() {
        n.degree = if rng.chance(0.7) { 0 } else { 4 };
    }
    (m, rhythm)
}

fn gen_stab(rng: &mut Rng, p: &Profile) -> Motif {
    let s = &p.stab;
    let k = rng.range_i32(s.hits[0] as i32, s.hits[1] as i32) as u32;
    let rot = rng.range_i32(0, 15);
    let mut pat = euclid(k, 16, rot).without(KICK_STEPS);
    if pat.count() == 0 {
        pat = Pattern::from_mask((1 << 2) | (1 << 10), 16);
    }
    let mut m = Motif::empty(16);
    for i in 0..16 {
        if pat.hit(i) {
            let v = if i % 4 == 2 { 1.0 } else { 0.8 };
            m.notes[i as usize] = Some(Note::new(0, v));
        }
    }
    m
}

/// Where the bass plays under a stab, it moves to the nearest chord tone so the two
/// never rub. The bass is the more flexible voice in this style.
fn fit_bass_to_stabs(bass: &mut Motif, stab: &Motif, chord: ChordShape) {
    let tones = chord.degrees();
    for i in 0..bass.len as u32 {
        if stab.at(i).is_none() {
            continue;
        }
        if let Some(n) = bass.notes[i as usize].as_mut() {
            let d = n.degree as i32;
            if !tones.iter().any(|t| (t - d).rem_euclid(7) == 0) {
                n.degree = 0;
            }
        }
    }
}

fn gen_lead(rng: &mut Rng, p: &Profile, key: Key, bass: &Motif, prog: &Progression) -> Motif {
    let l = &p.lead;
    let bars = l.motif_bars.pick(rng).clamp(1, 2) as usize;
    let len = bars * STEPS_PER_BAR as usize;
    let density = rng.in_range(l.density);
    let [lo, hi] = l.degree_range;
    let steps = &p.harmony.melody_steps;

    // Rhythm cell of half a bar; offbeat 16ths are favoured so the riff talks with the kick.
    let mut cell = [false; 8];
    for (i, c) in cell.iter_mut().enumerate() {
        let w = match i % 4 {
            0 => 0.8,
            2 => 1.25,
            _ => 1.05,
        };
        *c = rng.chance((density * w).min(0.95));
    }
    while cell.iter().filter(|c| **c).count() < 3 {
        let i = rng.range_i32(0, 7) as usize;
        cell[i] = true;
    }

    // Pitches: Markov walk, chord tones on strong steps.
    let walk = |rng: &mut Rng, prev: i32, strong: bool| -> i32 {
        let d = next_degree(rng, prev, steps, lo, hi, 0.15);
        if strong && rng.chance(0.6) {
            nearest_chord_tone(d)
        } else {
            d
        }
    };

    let mut m = Motif::empty(len as u8);
    let mut prev = if rng.chance(0.5) { 0 } else { 4 };
    // First half-bar from the cell.
    let mut first = [None; 8];
    for i in 0..8 {
        if cell[i] {
            let strong = i % 4 == 0;
            prev = walk(rng, prev, strong);
            first[i] = Some(prev);
        }
    }
    // Build the full motif: repeat the cell, re-rolling the tail of every other half bar.
    for half in 0..(len / 8) {
        let vary_tail = half % 2 == 1;
        for i in 0..8 {
            let idx = half * 8 + i;
            let mut on = cell[i];
            let mut deg = first[i];
            if vary_tail && i >= 5 {
                on = rng.chance(density);
                deg = if on {
                    Some(walk(rng, prev, false))
                } else {
                    None
                };
            }
            if on {
                let d = deg.unwrap_or(prev);
                prev = d;
                let mut n = Note::new(d, 0.85);
                n.accent = rng.chance(l.accent_prob);
                if n.accent {
                    n.velocity = 1.0;
                }
                m.notes[idx] = Some(n);
            }
        }
    }
    // Resolve the phrase end on root, fifth or octave.
    if let Some(last) = (0..len).rev().find(|&i| m.notes[i].is_some()) {
        const RES: [i32; 3] = [0, 4, 7];
        let d = RES[rng.weighted_index(&[3.0, 2.0, 1.0])];
        m.notes[last].as_mut().unwrap().degree = d as i8;
    }
    // Slides and lengths.
    for i in 0..len {
        let next_on = m.notes[(i + 1) % len].is_some();
        if let Some(n) = m.notes[i].as_mut() {
            if next_on && i + 1 < len && rng.chance(l.slide_prob) {
                n.slide = true;
            } else if !next_on && rng.chance(0.4) {
                n.len = 2;
            }
        }
    }
    avoid_friction_with_bass(&mut m, key, bass, prog);
    m
}

fn gen_arp(rng: &mut Rng, p: &Profile) -> (Motif, StabParams) {
    let a = &p.arp;
    let chord = a.chords.pick(rng);
    let octaves = a.octaves.pick(rng).clamp(1, 2) as i32;
    let rate = a.rate.pick(rng).clamp(1, 4) as usize;
    let pattern = a.patterns.pick(rng);
    let mut tones: Vec<i32> = Vec::new();
    for o in 0..octaves {
        for d in chord.degrees() {
            let t = d + 7 * o;
            if !tones.contains(&t) {
                tones.push(t);
            }
        }
    }
    tones.sort();
    let seq: Vec<i32> = match pattern {
        ArpPattern::Up => tones.clone(),
        ArpPattern::Down => tones.iter().rev().copied().collect(),
        ArpPattern::UpDown => {
            let mut v = tones.clone();
            v.extend(
                tones
                    .iter()
                    .rev()
                    .skip(1)
                    .take(tones.len().saturating_sub(2)),
            );
            v
        }
        ArpPattern::Random => (0..8)
            .map(|_| tones[rng.range_i32(0, tones.len() as i32 - 1) as usize])
            .collect(),
    };
    let mut m = Motif::empty(16);
    let mut k = 0;
    for step in (0..16).step_by(rate) {
        let v = if step % 4 == 0 { 1.0 } else { 0.72 };
        m.notes[step] = Some(Note::new(seq[k % seq.len()], v));
        k += 1;
    }
    let params = StabParams {
        variant: 0,
        chord,
        notes: 1,
        cutoff_hz: rng.in_range(a.cutoff_hz),
        env_mod: rng.in_range(a.env_mod),
        decay: rng.in_range(a.decay),
        detune: 0.04,
    };
    (m, params)
}

fn gen_fx(rng: &mut Rng, p: &Profile) -> FxParams {
    let f = &p.fx;
    let spec = |rng: &mut Rng, kind: InsertKind| {
        let rate = if kind == InsertKind::Tremolo {
            f.tremolo_beats.pick(rng)
        } else {
            rng.in_range(f.lfo_rate)
        };
        InsertSpec {
            kind,
            rate,
            depth: rng.in_range(f.depth),
        }
    };
    let synth_kind = f.synth_insert.pick(rng);
    let layer_kind = f.layer_insert.pick(rng);
    let hats_kind = if rng.chance(f.hat_phaser_prob) {
        InsertKind::Phaser
    } else {
        InsertKind::None
    };
    FxParams {
        synth: spec(rng, synth_kind),
        layer: spec(rng, layer_kind),
        hats: spec(rng, hats_kind),
    }
}

/// Leads that are less melodic than the riff: sparse, pulse or long notes.
fn gen_lead_styled(
    rng: &mut Rng,
    style: LeadStyle,
    key: Key,
    bass: &Motif,
    prog: &Progression,
) -> Motif {
    let len = if style == LeadStyle::Long || rng.chance(0.5) {
        32
    } else {
        16
    };
    let mut m = Motif::empty(len as u8);
    // A small pitch set: the root plus at most two chord tones.
    const POOL: [i32; 4] = [2, 4, 7, -3];
    let a = POOL[rng.range_i32(0, 3) as usize];
    let mut b = POOL[rng.range_i32(0, 3) as usize];
    if b == a {
        b = 0;
    }
    let pitches = [0, a, b];
    let pick = |rng: &mut Rng| pitches[rng.weighted_index(&[3.0, 2.0, 1.0])];
    match style {
        LeadStyle::Sparse => {
            const SPOTS: [usize; 9] = [2, 3, 6, 7, 10, 11, 14, 15, 8];
            for bar in 0..len / 16 {
                let n = rng.range_i32(2, 4) as usize;
                let mut placed = 0;
                let mut guard = 0;
                while placed < n && guard < 40 {
                    guard += 1;
                    let s = bar * 16 + SPOTS[rng.range_i32(0, SPOTS.len() as i32 - 1) as usize];
                    if m.notes[s].is_none() {
                        let mut note = Note::new(pick(rng), if s % 4 == 2 { 1.0 } else { 0.85 });
                        note.len = if rng.chance(0.4) { 2 } else { 1 };
                        m.notes[s] = Some(note);
                        placed += 1;
                    }
                }
            }
        }
        LeadStyle::Pulse => {
            let k = rng.range_i32(5, 9) as u32;
            let pat = euclid(k, 16, rng.range_i32(0, 15));
            for s in 0..len {
                if pat.hit(s as u32) {
                    let mut note = Note::new(0, if s % 4 == 2 { 1.0 } else { 0.7 });
                    note.accent = s % 4 == 2;
                    m.notes[s] = Some(note);
                }
            }
            // Optional octave answer at the very end.
            if rng.chance(0.4) {
                if let Some(last) = (0..len).rev().find(|&s| m.notes[s].is_some()) {
                    m.notes[last].as_mut().unwrap().degree = 7;
                }
            }
        }
        LeadStyle::Long => {
            let mut s = rng.range_i32(0, 2) as usize * 2;
            let mut first = true;
            while s < len {
                let l = [4usize, 6, 8][rng.range_i32(0, 2) as usize].min(len - s);
                let d = if first { 0 } else { pick(rng) };
                first = false;
                let mut note = Note::new(d, 0.9);
                note.len = l as u8;
                m.notes[s] = Some(note);
                s += l + [0usize, 2, 4][rng.range_i32(0, 2) as usize];
            }
        }
        LeadStyle::Riff => {}
    }
    if m.note_count() == 0 {
        m.notes[2] = Some(Note::new(0, 1.0));
    }
    avoid_friction_with_bass(&mut m, key, bass, prog);
    m
}

fn nearest_chord_tone(d: i32) -> i32 {
    const TONES: [i32; 6] = [-3, 0, 2, 4, 7, 9];
    *TONES.iter().min_by_key(|t| (d - **t).abs()).unwrap()
}

/// Lead notes that form a second, tritone or seventh with a simultaneous bass note
/// (under any root of the progression) are nudged to the closest consonant degree.
fn avoid_friction_with_bass(lead: &mut Motif, key: Key, bass: &Motif, prog: &Progression) {
    let span = MAX_MOTIF_STEPS as u32; // lcm of 16 and 32
    let harsh = |lead_deg: i32, step: u32| -> bool {
        let mut s = step;
        while s < span {
            if let Some(b) = bass.at(s) {
                for &r in prog.roots() {
                    let r = r as i32;
                    let li = key.degree_to_semitone(r + lead_deg);
                    let bi = key.degree_to_semitone(r + b.degree as i32);
                    if is_harsh_interval(li - bi) {
                        return true;
                    }
                }
            }
            s += lead.len as u32;
        }
        false
    };
    for i in 0..lead.len as u32 {
        let Some(n) = lead.notes[i as usize] else {
            continue;
        };
        let d = n.degree as i32;
        if !harsh(d, i) {
            continue;
        }
        for off in [1, -1, 2, -2, 3, -3] {
            if !harsh(d + off, i) {
                lead.notes[i as usize].as_mut().unwrap().degree = (d + off) as i8;
                break;
            }
        }
    }
}

fn gen_sound(rng: &mut Rng, p: &Profile, key: Key) -> SoundParams {
    let d = &p.drums;
    let mut kick_tune = rng.in_range(d.kick.tune_hz);
    if d.kick.tune_to_key {
        kick_tune = kick_tune_for_key(key, d.kick.tune_hz);
    }
    SoundParams {
        crash_variant: 0,
        fx: FxParams {
            synth: InsertSpec {
                kind: InsertKind::None,
                rate: 0.0,
                depth: 0.0,
            },
            layer: InsertSpec {
                kind: InsertKind::None,
                rate: 0.0,
                depth: 0.0,
            },
            hats: InsertSpec {
                kind: InsertKind::None,
                rate: 0.0,
                depth: 0.0,
            },
        },
        arp: StabParams {
            variant: 0,
            chord: ChordShape::Triad,
            notes: 1,
            cutoff_hz: 1000.0,
            env_mod: 2.0,
            decay: 0.1,
            detune: 0.05,
        },
        drone: DroneParams {
            variant: 0,
            cutoff_hz: 400.0,
            drive: 2.0,
            detune: 0.1,
        },
        kick: KickParams {
            style: KickStyle::Hard,
            tune_hz: kick_tune,
            pitch_start_hz: rng.in_range(d.kick.pitch_start_hz),
            pitch_decay: rng.in_range(d.kick.pitch_decay),
            decay: rng.in_range(d.kick.decay),
            drive: rng.in_range(d.kick.drive),
            click: rng.in_range(d.kick.click),
        },
        hats: HatParams {
            variant: 0,
            tone_hz: rng.in_range(d.hats.tone_hz),
            closed_decay: rng.in_range(d.hats.closed_decay),
            open_decay: rng.in_range(d.hats.open_decay),
            metal: rng.in_range(d.hats.metal),
        },
        clap: ToneParams {
            variant: 0,
            tone_hz: rng.in_range(d.clap.tone_hz),
            decay: rng.in_range(d.clap.decay),
        },
        snare: SnareParams {
            variant: 0,
            tone_hz: rng.in_range(d.snare.tone_hz),
            decay: rng.in_range(d.snare.decay),
            snappy: rng.in_range(d.snare.snappy),
        },
        rim: ToneParams {
            variant: 0,
            tone_hz: rng.in_range(d.rim.tone_hz),
            decay: rng.in_range(d.rim.decay),
        },
        tom: ToneParams {
            variant: 0,
            tone_hz: rng.in_range(d.tom.tone_hz),
            decay: rng.in_range(d.tom.decay),
        },
        ride: ToneParams {
            variant: 0,
            tone_hz: rng.in_range(d.ride.tone_hz),
            decay: rng.in_range(d.ride.decay),
        },
        bass: BassParams {
            variant: 0,
            rhythm: BassRhythm::Rolling,
            cutoff_hz: rng.in_range(p.bass.cutoff_hz),
            env_mod: rng.in_range(p.bass.env_mod),
            resonance: rng.in_range(p.bass.resonance),
            decay: rng.in_range(p.bass.decay),
            drive: rng.in_range(p.bass.drive),
            sub_mix: rng.in_range(p.bass.sub_mix),
        },
        rumble: RumbleParams {
            variant: 0,
            amount: rng.in_range(p.rumble.amount),
            lowpass_hz: rng.in_range(p.rumble.lowpass_hz),
            drive: rng.in_range(p.rumble.drive),
            size: rng.in_range(p.rumble.size),
        },
        stab: StabParams {
            variant: 0,
            chord: p.stab.chords.pick(rng),
            notes: 1,
            cutoff_hz: rng.in_range(p.stab.cutoff_hz),
            env_mod: rng.in_range(p.stab.env_mod),
            decay: rng.in_range(p.stab.decay),
            detune: rng.in_range(p.stab.detune),
        },
        lead: LeadParams {
            kind: LeadKind::Acid,
            style: LeadStyle::Riff,
            cutoff_hz: rng.in_range(p.lead.cutoff_hz),
            resonance: rng.in_range(p.lead.resonance),
            env_mod: rng.in_range(p.lead.env_mod),
            decay: rng.in_range(p.lead.decay),
            drive: rng.in_range(p.lead.drive),
            square: rng.in_range(p.lead.square),
        },
        pad: PadParams {
            variant: 0,
            chord: p.pad.chords.pick(rng),
            cutoff_hz: rng.in_range(p.pad.cutoff_hz),
            detune: rng.in_range(p.pad.detune),
            attack: rng.in_range(p.pad.attack),
        },
        mix: MixParams {
            sidechain_depth: rng.in_range(p.mix.sidechain_depth),
            sidechain_release: rng.in_range(p.mix.sidechain_release),
        },
        // Filled in from their own streams in `generate_with`.
        piano: KeysParams {
            variant: 0,
            style: KeysStyle::Stabs,
            chord: ChordShape::Seventh,
            brightness: 0.5,
            decay: 1.5,
        },
        guitar: GuitarParams {
            variant: 0,
            style: GuitarStyle::Muted,
            chord: ChordShape::Triad,
            brightness: 0.5,
            sustain: 1.0,
        },
    }
}

/// Classic house piano rhythms over two bars (16th steps, len in steps).
const PIANO_STABS: [&[(usize, u8)]; 5] = [
    // The "M1" push: 1, a-of-1, and-of-2, a-of-3 ...
    &[
        (0, 2),
        (3, 2),
        (6, 3),
        (10, 2),
        (12, 3),
        (16, 2),
        (19, 2),
        (22, 3),
        (26, 4),
    ],
    &[(2, 2), (6, 2), (10, 2), (14, 2), (18, 2), (22, 2), (27, 3)],
    &[(0, 3), (6, 2), (11, 3), (16, 3), (22, 2), (27, 2), (30, 2)],
    &[(3, 2), (6, 4), (14, 2), (19, 2), (22, 4), (30, 2)],
    &[(0, 6), (6, 2), (10, 6), (16, 6), (22, 2), (26, 6)],
];

fn gen_piano(rng: &mut Rng, p: &Profile) -> (Motif, KeysParams) {
    let r = &p.piano;
    let style = r.styles.pick(rng);
    let mut m = Motif::empty(32);
    match style {
        KeysStyle::Stabs => {
            let pat = PIANO_STABS[rng.range_i32(0, PIANO_STABS.len() as i32 - 1) as usize];
            for &(s, len) in pat {
                let mut n = Note::new(0, if s % 4 == 0 { 0.95 } else { 0.8 });
                n.len = len;
                n.accent = s % 8 == 6;
                m.notes[s] = Some(n);
            }
        }
        KeysStyle::Offbeat => {
            for s in (2..32).step_by(4) {
                let mut n = Note::new(0, 0.8);
                n.len = 2;
                m.notes[s] = Some(n);
            }
            if rng.chance(0.5) {
                // A push into the next bar.
                let mut n = Note::new(0, 0.9);
                n.len = 2;
                m.notes[31] = Some(n);
                m.notes[30] = None;
            }
        }
        KeysStyle::Held => {
            let per = if rng.chance(0.5) { 16 } else { 32 };
            for s in (0..32).step_by(per) {
                let mut n = Note::new(0, 0.75);
                n.len = (per - 2) as u8;
                m.notes[s] = Some(n);
            }
            // Soft re-attack on the "and" of 3 now and then.
            if per == 16 && rng.chance(0.5) {
                let mut n = Note::new(0, 0.55);
                n.len = 4;
                m.notes[10] = Some(n);
                m.notes[0].as_mut().unwrap().len = 10;
            }
        }
    }
    let params = KeysParams {
        variant: r.variants.pick(rng),
        style,
        chord: r.chords.pick(rng),
        brightness: rng.in_range(r.brightness),
        decay: rng.in_range(r.decay),
    };
    (m, params)
}

fn gen_guitar(
    rng: &mut Rng,
    p: &Profile,
    key: Key,
    bass: &Motif,
    prog: &Progression,
) -> (Motif, GuitarParams) {
    let r = &p.guitar;
    let style = r.styles.pick(rng);
    let mut m = Motif::empty(16);
    match style {
        GuitarStyle::Muted => {
            // Funky muted 16ths: a euclidean rhythm, mostly root and octave.
            let k = rng.range_i32(6, 10) as u32;
            let pat = euclid(k, 16, rng.range_i32(0, 3));
            const POOL: [i32; 4] = [0, 7, 4, 2];
            for s in 0..16 {
                if pat.hit(s) {
                    let d = POOL[rng.weighted_index(&[4.0, 2.0, 1.5, 0.8])];
                    let mut n = Note::new(d, if s % 4 == 2 { 0.95 } else { 0.7 });
                    n.accent = s % 4 == 2;
                    m.notes[s as usize] = Some(n);
                }
            }
        }
        GuitarStyle::Strum => {
            // Offbeat strums, every other one an up-strum (accent = down).
            m = Motif::empty(16);
            for (i, s) in [2usize, 6, 10, 14].into_iter().enumerate() {
                let mut n = Note::new(0, if i % 2 == 0 { 0.9 } else { 0.7 });
                n.accent = i % 2 == 0;
                n.len = 2;
                m.notes[s] = Some(n);
            }
            if rng.chance(0.5) {
                let mut n = Note::new(0, 0.6);
                n.len = 1;
                m.notes[15] = Some(n);
            }
        }
        GuitarStyle::Picked => {
            m = gen_lead_styled(rng, LeadStyle::Sparse, key, bass, prog);
        }
    }
    if m.note_count() == 0 {
        m.notes[2] = Some(Note::new(0, 0.9));
    }
    if style != GuitarStyle::Picked {
        avoid_friction_with_bass(&mut m, key, bass, prog);
    }
    let params = GuitarParams {
        variant: r.variants.pick(rng),
        style,
        chord: r.chords.pick(rng),
        brightness: rng.in_range(r.brightness),
        sustain: rng.in_range(r.sustain),
    };
    (m, params)
}

/// Kick tuning that sits with the key: the tonic, else the fifth, else the fourth,
/// folded by octaves into `range`. With a range of >= 5 semitones one always fits.
fn kick_tune_for_key(key: Key, range: [f32; 2]) -> f32 {
    for interval in [0, 7, 5] {
        let mut f = midi_to_hz((key.root as i32 + interval) as f32);
        while f < range[0] {
            f *= 2.0;
        }
        if f <= range[1] {
            return f;
        }
    }
    (range[0] * range[1]).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn songs() -> impl Iterator<Item = SongDna> {
        let p = Profile::hard_techno_simple();
        (0..200u64).map(move |s| SongDna::generate(s, &p))
    }

    #[test]
    fn deterministic() {
        let p = Profile::hard_techno_simple();
        let a = SongDna::generate(1234, &p);
        let b = SongDna::generate(1234, &p);
        assert_eq!(a.describe(), b.describe());
    }

    #[test]
    fn different_seeds_differ() {
        let p = Profile::hard_techno_simple();
        let a = SongDna::generate(1, &p).describe();
        let b = SongDna::generate(2, &p).describe();
        assert_ne!(a, b);
    }

    #[test]
    fn only_scale_tones() {
        for dna in songs() {
            for &root in dna.progression.roots() {
                for motif in [&dna.bass, &dna.lead, &dna.stab] {
                    for i in 0..motif.len as u32 {
                        if let Some(n) = motif.at(i) {
                            let semis = dna.key.degree_to_semitone(root as i32 + n.degree as i32);
                            let pc = dna.key.root as i32 + semis;
                            assert!(dna.key.contains_pitch_class(pc), "seed {}", dna.seed);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn kick_on_every_quarter() {
        for dna in songs() {
            for s in 0..16 {
                assert_eq!(dna.drums.kick.hit(s), s % 4 == 0);
            }
        }
    }

    #[test]
    fn bass_never_on_kick() {
        for dna in songs() {
            for s in 0..dna.bass.len as u32 {
                if s % 4 == 0 {
                    assert!(dna.bass.at(s).is_none(), "seed {}", dna.seed);
                }
            }
        }
    }

    #[test]
    fn bass_root_on_strong_positions() {
        for dna in songs() {
            // The first note in bar 1 is always the root.
            let first = (0..16).find_map(|s| dna.bass.at(s)).unwrap();
            assert_eq!(first.degree, 0, "seed {}", dna.seed);
        }
    }

    #[test]
    fn lead_has_no_harsh_intervals_against_bass() {
        for dna in songs() {
            for s in 0..32u32 {
                let (Some(l), Some(b)) = (dna.lead.at(s), dna.bass.at(s)) else {
                    continue;
                };
                for &r in dna.progression.roots() {
                    let r = r as i32;
                    let li = dna.key.degree_to_semitone(r + l.degree as i32);
                    let bi = dna.key.degree_to_semitone(r + b.degree as i32);
                    assert!(!is_harsh_interval(li - bi), "seed {} step {}", dna.seed, s);
                }
            }
        }
    }

    #[test]
    fn bass_stays_in_profile_range() {
        let p = Profile::hard_techno_simple();
        let [lo, hi] = p.bass.degree_range;
        for dna in songs() {
            for s in 0..dna.bass.len as u32 {
                if let Some(n) = dna.bass.at(s) {
                    assert!((lo..=hi).contains(&(n.degree as i32)), "seed {}", dna.seed);
                }
            }
        }
    }

    #[test]
    fn one_short_synth_and_one_long_layer_at_a_time() {
        use crate::structure::SHORT_SYNTHS;
        let long = TrackSet::of(&[Track::Pad, Track::Drone]);
        for dna in songs() {
            assert!(
                dna.plan.palette.and(long).0.count_ones() <= 1,
                "seed {}",
                dna.seed
            );
            for bar in 0..dna.plan.total_bars {
                let a = dna.plan.bar(bar).active;
                assert!(
                    a.and(SHORT_SYNTHS).0.count_ones() <= 1,
                    "seed {} bar {}",
                    dna.seed,
                    bar
                );
            }
        }
    }

    #[test]
    fn harmony_is_sparse() {
        // Dark techno: four distinct roots are rare, and breakdown-only harmony holds the tonic
        // outside breakdowns.
        let p = Profile::hard_techno_simple();
        let mut four = 0;
        let mut moving_bars = 0;
        let mut bars = 0;
        let n = 500;
        for seed in 0..n {
            let dna = SongDna::generate(seed, &p);
            let distinct: std::collections::HashSet<i8> =
                dna.progression.roots().iter().copied().collect();
            four += (distinct.len() >= 4) as u32;
            for bar in 0..dna.plan.total_bars {
                bars += 1;
                moving_bars += (dna.root_at(bar) != 0) as u32;
                if dna.progression.breakdown_only
                    && dna.plan.section_at(bar).kind != SectionKind::Breakdown
                {
                    assert_eq!(dna.root_at(bar), 0);
                }
            }
        }
        assert!(
            four * 20 < n as u32,
            "four-root progressions should be rare: {four}/{n}"
        );
        assert!(
            moving_bars * 4 < bars,
            "the tonic should dominate: {moving_bars}/{bars}"
        );
    }

    fn house_songs() -> impl Iterator<Item = SongDna> {
        let p = Profile::house();
        (0..200u64).map(move |s| SongDna::generate(s, &p))
    }

    #[test]
    fn house_stays_in_key_including_piano_and_guitar() {
        for dna in house_songs() {
            for &root in dna.progression.roots() {
                for motif in [&dna.bass, &dna.lead, &dna.piano, &dna.guitar] {
                    for i in 0..motif.len as u32 {
                        if let Some(n) = motif.at(i) {
                            let semis = dna.key.degree_to_semitone(root as i32 + n.degree as i32);
                            let pc = dna.key.root as i32 + semis;
                            assert!(dna.key.contains_pitch_class(pc), "seed {}", dna.seed);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn house_is_house() {
        use crate::structure::SHORT_SYNTHS;
        let p = Profile::house();
        let (mut with_drop, mut natural, mut companions) = (0, 0, 0);
        for dna in house_songs() {
            assert!(dna.bpm >= p.tempo.bpm[0] && dna.bpm <= p.tempo.bpm[1]);
            assert!(dna.swing >= p.tempo.swing[0]);
            assert!(
                !dna.plan.palette.contains(Track::Rumble),
                "no rumble in house"
            );
            assert_ne!(dna.sound.kick.style, KickStyle::Distorted);
            with_drop += dna
                .plan
                .sections()
                .iter()
                .any(|s| s.kind == SectionKind::Drop) as u32;
            natural += [Track::Piano, Track::Guitar]
                .iter()
                .any(|t| dna.plan.palette.contains(*t)) as u32;
            companions += dna.plan.companion.is_some() as u32;
            for bar in 0..dna.plan.total_bars {
                let b = dna.plan.bar(bar);
                // Main instrument plus at most one companion.
                assert!(
                    b.active.and(SHORT_SYNTHS).0.count_ones() <= 2,
                    "seed {}",
                    dna.seed
                );
                // Soft: no impact hits into breakdowns.
                if b.section == SectionKind::Breakdown {
                    assert!(!b.impact, "seed {} bar {bar}", dna.seed);
                }
            }
        }
        assert!(
            with_drop < 80,
            "drops should be the exception: {with_drop}/200"
        );
        assert!(
            natural > 100,
            "piano / guitar should be common: {natural}/200"
        );
        assert!(companions > 30, "{companions}/200");
    }

    #[test]
    fn house_grooves_have_strings() {
        // The long layer also plays in grooves (house backdrop), not only in drops.
        let mut groove_layer = 0;
        for dna in house_songs() {
            groove_layer += (0..dna.plan.total_bars).any(|bar| {
                let b = dna.plan.bar(bar);
                b.section == SectionKind::Groove && b.active.contains(Track::Pad)
            }) as u32;
        }
        assert!(groove_layer > 120, "{groove_layer}/200");
    }

    #[test]
    fn hats_never_double_open_and_closed() {
        for dna in songs() {
            assert_eq!(dna.drums.hat_closed.mask & dna.drums.hat_open.mask, 0);
        }
    }

    #[test]
    fn kick_tuned_into_range() {
        let p = Profile::hard_techno_simple();
        for dna in songs() {
            let t = dna.sound.kick.tune_hz;
            assert!(
                t >= p.drums.kick.tune_hz[0] && t <= p.drums.kick.tune_hz[1],
                "{t}"
            );
        }
    }

    #[test]
    fn motif_lengths_are_phrase_friendly() {
        for dna in songs() {
            for m in [&dna.bass, &dna.lead, &dna.stab] {
                assert!(m.len == 16 || m.len == 32);
                assert!(m.note_count() > 0);
            }
        }
    }
}
