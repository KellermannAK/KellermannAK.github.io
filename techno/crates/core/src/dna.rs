//! Song DNA: the main idea of a song, generated once from its seed and kept for the
//! whole song. Variation later only transforms the DNA, it never re-invents it.
//!
//! The DNA is a plain `Copy` struct with fixed-size arrays, so it can be handed to the
//! audio thread without heap traffic. Generating it may allocate briefly; that happens
//! ahead of time (the next song is pre-generated), never inside the render callback.

use crate::euclid::{euclid, Pattern};
use crate::markov::next_degree;
use crate::profile::{BassRhythm, ChordShape, HatStyle, Profile, RideStyle};
use crate::rng::Rng;
use crate::theory::{is_harsh_interval, midi_to_hz, Key, ScaleKind};
use crate::time::STEPS_PER_BAR;

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
    pub tune_hz: f32,
    pub pitch_start_hz: f32,
    pub pitch_decay: f32,
    pub decay: f32,
    pub drive: f32,
    pub click: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct HatParams {
    pub tone_hz: f32,
    pub closed_decay: f32,
    pub open_decay: f32,
    pub metal: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ToneParams {
    pub tone_hz: f32,
    pub decay: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SnareParams {
    pub tone_hz: f32,
    pub decay: f32,
    pub snappy: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct BassParams {
    pub cutoff_hz: f32,
    pub env_mod: f32,
    pub resonance: f32,
    pub decay: f32,
    pub drive: f32,
    pub sub_mix: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct RumbleParams {
    pub amount: f32,
    pub lowpass_hz: f32,
    pub drive: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct StabParams {
    pub chord: ChordShape,
    pub cutoff_hz: f32,
    pub env_mod: f32,
    pub decay: f32,
    pub detune: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct LeadParams {
    pub cutoff_hz: f32,
    pub resonance: f32,
    pub env_mod: f32,
    pub decay: f32,
    pub drive: f32,
    pub square: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct PadParams {
    pub chord: ChordShape,
    pub cutoff_hz: f32,
    pub detune: f32,
    pub attack: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct MixParams {
    pub sidechain_depth: f32,
    pub sidechain_release: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SoundParams {
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
    pub sound: SoundParams,
}

/// MIDI note of the bass tonic: folded into E1..D#2 (41-78 Hz) for a solid low end.
pub fn bass_base_midi(key: Key) -> i32 {
    28 + (key.root as i32 - 4).rem_euclid(12)
}

impl SongDna {
    pub fn generate(seed: u64, profile: &Profile) -> SongDna {
        let root = Rng::new(seed);

        let mut r = root.fork("tempo");
        let bpm = r.in_range(profile.tempo.bpm).round();
        let swing = r.in_range(profile.tempo.swing);

        let mut r = root.fork("key");
        let key = Key {
            root: r.range_i32(0, 11) as u8,
            scale: profile.harmony.scales.pick(&mut r),
        };

        let progression = gen_progression(&mut root.fork("progression"), profile, key);
        let drums = gen_drums(&mut root.fork("drums"), profile);
        let sound = gen_sound(&mut root.fork("sound"), profile, key);
        let stab = gen_stab(&mut root.fork("stab"), profile);
        let mut bass = gen_bass(&mut root.fork("bass"), profile);
        fit_bass_to_stabs(&mut bass, &stab, sound.stab.chord);
        let lead = gen_lead(&mut root.fork("lead"), profile, key, &bass, &progression);

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
            sound,
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
             roots      [{}] x {} bars\n\
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
             (motif digits = scale degree above root, a/b/c.. = 1/2/3.. below)",
            self.seed,
            self.bpm,
            self.swing,
            self.key.name(),
            self.key.camelot_code(),
            roots.join(", "),
            self.progression.bars_per_root,
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
        )
    }
}

// ---------------------------------------------------------------------------------------
// Generators
// ---------------------------------------------------------------------------------------

fn gen_progression(rng: &mut Rng, p: &Profile, key: Key) -> Progression {
    let h = &p.harmony;
    let count = (h.progression_length.pick(rng) as usize).clamp(1, MAX_PROGRESSION);
    let bars_per_root = h.bars_per_root.pick(rng).max(1);
    let mut roots = [0i8; MAX_PROGRESSION];

    // Degrees that would produce an ugly root chord in this scale are filtered out:
    // bII only exists as a strong colour in phrygian, VII in harmonic minor is diminished.
    let allowed = |d: i32| match d {
        1 => key.scale == ScaleKind::Phrygian,
        6 => key.scale != ScaleKind::HarmonicMinor,
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
    Progression {
        roots,
        count: count as u8,
        bars_per_root,
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

fn bass_positions(r: BassRhythm) -> &'static [u32] {
    match r {
        BassRhythm::Rolling => &[1, 2, 3],
        BassRhythm::Offbeat => &[2],
        BassRhythm::Gallop => &[2, 3],
        BassRhythm::Skippy => &[1, 3],
    }
}

fn gen_bass(rng: &mut Rng, p: &Profile) -> Motif {
    let b = &p.bass;
    let bars = b.motif_bars.pick(rng).clamp(1, 2) as u32;
    let len = bars * STEPS_PER_BAR;
    let rhythm = b.rhythms.pick(rng);
    let mut positions: Vec<u32> = bass_positions(rhythm).to_vec();
    if b.allow_on_kick {
        positions.insert(0, 0);
    }
    let first_pos = positions[0];
    let movement = rng.in_range(b.movement);
    let [lo, hi] = b.degree_range;

    let mut m = Motif::empty(len as u8);
    let mut prev = 0;
    let mut last_idx = 0;
    for step in 0..len {
        let pos = step % 4;
        if !positions.contains(&pos) {
            continue;
        }
        // Strong positions (first note after the kick on beats 1 and 3) stay on the root.
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
    m
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
        kick: KickParams {
            tune_hz: kick_tune,
            pitch_start_hz: rng.in_range(d.kick.pitch_start_hz),
            pitch_decay: rng.in_range(d.kick.pitch_decay),
            decay: rng.in_range(d.kick.decay),
            drive: rng.in_range(d.kick.drive),
            click: rng.in_range(d.kick.click),
        },
        hats: HatParams {
            tone_hz: rng.in_range(d.hats.tone_hz),
            closed_decay: rng.in_range(d.hats.closed_decay),
            open_decay: rng.in_range(d.hats.open_decay),
            metal: rng.in_range(d.hats.metal),
        },
        clap: ToneParams {
            tone_hz: rng.in_range(d.clap.tone_hz),
            decay: rng.in_range(d.clap.decay),
        },
        snare: SnareParams {
            tone_hz: rng.in_range(d.snare.tone_hz),
            decay: rng.in_range(d.snare.decay),
            snappy: rng.in_range(d.snare.snappy),
        },
        rim: ToneParams {
            tone_hz: rng.in_range(d.rim.tone_hz),
            decay: rng.in_range(d.rim.decay),
        },
        tom: ToneParams {
            tone_hz: rng.in_range(d.tom.tone_hz),
            decay: rng.in_range(d.tom.decay),
        },
        ride: ToneParams {
            tone_hz: rng.in_range(d.ride.tone_hz),
            decay: rng.in_range(d.ride.decay),
        },
        bass: BassParams {
            cutoff_hz: rng.in_range(p.bass.cutoff_hz),
            env_mod: rng.in_range(p.bass.env_mod),
            resonance: rng.in_range(p.bass.resonance),
            decay: rng.in_range(p.bass.decay),
            drive: rng.in_range(p.bass.drive),
            sub_mix: rng.in_range(p.bass.sub_mix),
        },
        rumble: RumbleParams {
            amount: rng.in_range(p.rumble.amount),
            lowpass_hz: rng.in_range(p.rumble.lowpass_hz),
            drive: rng.in_range(p.rumble.drive),
            size: rng.in_range(p.rumble.size),
        },
        stab: StabParams {
            chord: p.stab.chords.pick(rng),
            cutoff_hz: rng.in_range(p.stab.cutoff_hz),
            env_mod: rng.in_range(p.stab.env_mod),
            decay: rng.in_range(p.stab.decay),
            detune: rng.in_range(p.stab.detune),
        },
        lead: LeadParams {
            cutoff_hz: rng.in_range(p.lead.cutoff_hz),
            resonance: rng.in_range(p.lead.resonance),
            env_mod: rng.in_range(p.lead.env_mod),
            decay: rng.in_range(p.lead.decay),
            drive: rng.in_range(p.lead.drive),
            square: rng.in_range(p.lead.square),
        },
        pad: PadParams {
            chord: p.pad.chords.pick(rng),
            cutoff_hz: rng.in_range(p.pad.cutoff_hz),
            detune: rng.in_range(p.pad.detune),
            attack: rng.in_range(p.pad.attack),
        },
        mix: MixParams {
            sidechain_depth: rng.in_range(p.mix.sidechain_depth),
            sidechain_release: rng.in_range(p.mix.sidechain_release),
        },
    }
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
