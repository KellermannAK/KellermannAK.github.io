//! Song structure (milestone 4): a song is a sequence of sections drawn from a simple
//! grammar, fixed at song start. Every bar gets a `BarPlan` (which tracks play, fills,
//! one-shot FX) and every point in time an `Automation` (filters, sends, riser), both
//! derived deterministically from the plan.

use serde::{Deserialize, Serialize};

use crate::profile::{Profile, Weighted};
use crate::rng::Rng;
use crate::time::seconds_per_bar;
use crate::tracks::{Track, TrackSet};

pub const MAX_SECTIONS: usize = 24;
pub const PHRASE_BARS: u32 = 8;

/// Short, rhythmic synths: at most one of them plays in any bar.
pub const SHORT_SYNTHS: TrackSet =
    TrackSet((1 << Track::Lead as u32) | (1 << Track::Arp as u32) | (1 << Track::Stab as u32));
const MAX_PHRASES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SectionKind {
    Intro,
    Groove,
    Build,
    Breakdown,
    Drop,
    Outro,
}

impl SectionKind {
    pub const fn name(self) -> &'static str {
        match self {
            SectionKind::Intro => "intro",
            SectionKind::Groove => "groove",
            SectionKind::Build => "build",
            SectionKind::Breakdown => "breakdown",
            SectionKind::Drop => "drop",
            SectionKind::Outro => "outro",
        }
    }

    /// Energy at section start and end.
    const fn energy(self) -> [f32; 2] {
        match self {
            SectionKind::Intro => [0.25, 0.45],
            SectionKind::Groove => [0.55, 0.7],
            SectionKind::Build => [0.6, 0.95],
            SectionKind::Breakdown => [0.3, 0.4],
            SectionKind::Drop => [1.0, 0.95],
            SectionKind::Outro => [0.45, 0.2],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub kind: SectionKind,
    pub start: u32,
    pub bars: u32,
}

impl Section {
    pub fn end(&self) -> u32 {
        self.start + self.bars
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    None,
    /// Tom run in the last beat.
    Small,
    /// Rising snare roll over the last two beats.
    Roll,
}

impl Fill {
    /// 16th step where the fill replaces the regular pattern.
    pub fn starts_at(self) -> u32 {
        match self {
            Fill::None => 16,
            Fill::Small => 12,
            Fill::Roll => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiserKind {
    /// Band-passed noise sweeping up.
    Noise,
    /// Noise sweep chopped by a gate that speeds up (quarters -> 32nds).
    Gated,
    /// Bright noise swell, like a reversed cymbal.
    Swell,
    /// Noise plus a detuned saw stack rising a fifth.
    Pitched,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RollKind {
    Snare,
    Clap,
    /// Kick drum roll (classic techno build).
    Kick,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropStyle {
    /// Everything at once.
    Full,
    /// Kick, bass and drums first; synths and layers join after 8 bars.
    BassFirst,
    /// Starts reduced and builds up over 16 bars.
    Stripped,
    /// Full, but a low-pass opens over the first 8 bars.
    FilterOpen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropImpact {
    ImpactCrash,
    Crash,
    None,
}

/// Random flavour of one section (only the fields matching its kind are used).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionStyle {
    pub riser: RiserKind,
    pub roll: RollKind,
    pub highpass: bool,
    pub drop: DropStyle,
    pub impact: DropImpact,
    /// One-bar pause (only riser and roll) right before this drop.
    pub pause_before: bool,
}

impl Default for SectionStyle {
    fn default() -> Self {
        SectionStyle {
            riser: RiserKind::Noise,
            roll: RollKind::Snare,
            highpass: false,
            drop: DropStyle::Full,
            impact: DropImpact::ImpactCrash,
            pause_before: false,
        }
    }
}

/// Per-phrase variation of the DNA (never a re-invention).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Variation {
    /// Lead one octave up.
    pub lead_up: bool,
    /// Lead plays only every other note.
    pub lead_thin: bool,
    /// Percussion track left out for this phrase (rim, tom or ride).
    pub drop_perc: Option<Track>,
    /// Open hat left out for this phrase.
    pub no_open_hat: bool,
}

/// What happens in one bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    pub section: SectionKind,
    /// Bar index inside the section.
    pub section_bar: u32,
    pub section_bars: u32,
    pub energy: f32,
    pub active: TrackSet,
    pub fill: Fill,
    pub crash: bool,
    /// Impact + downlifter on the downbeat.
    pub impact: bool,
    /// Roll in this bar: hit every n 16ths (0 = none).
    pub roll_every: u8,
    pub roll_kind: RollKind,
    pub riser_kind: RiserKind,
    /// 0..1 progress of a build (roll velocity ramps with it).
    pub build_progress: f32,
    pub variation: Variation,
}

/// Continuous parameters, evaluated per tick and smoothed per sample by the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Automation {
    /// Lead filter opening 0..1.
    pub lead_filter: f32,
    /// Bass filter 0..1 (1 = as designed, lower = darker / reduced).
    pub bass_filter: f32,
    /// Deck high-pass 0..1 (0 = off, 1 = ~400 Hz), used at the end of builds.
    pub highpass: f32,
    /// Multiplier for delay / reverb sends.
    pub fx_send: f32,
    /// Riser level 0..1 and sweep progress 0..1.
    pub riser: f32,
    pub riser_progress: f32,
    /// Deck low-pass 0..1 (0 = open, 1 = ~250 Hz), used for filter-open drops.
    pub lowpass: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructureRules {
    /// Target song length in bars (inclusive range, phrase aligned).
    pub target_bars: [u32; 2],
    pub intro_bars: u32,
    pub outro_bars: u32,
    pub groove_bars: Weighted<u32>,
    pub build_bars: Weighted<u32>,
    pub breakdown_bars: Weighted<u32>,
    pub drop_bars: Weighted<u32>,
    pub after_intro: Weighted<SectionKind>,
    pub after_groove: Weighted<SectionKind>,
    pub after_drop: Weighted<SectionKind>,
    /// Probability of a one-bar pause (only riser and roll) right before a drop.
    pub drop_pause_prob: f32,
    /// Probability that a breakdown is followed by a build (otherwise it goes straight
    /// into the drop with a short run-up of its own).
    pub breakdown_to_build_prob: f32,
    pub riser: Weighted<RiserKind>,
    pub roll: Weighted<RollKind>,
    pub build_highpass_prob: f32,
    pub drop_style: Weighted<DropStyle>,
    pub drop_impact: Weighted<DropImpact>,
    /// Per-song jitter of the energy thresholds at which tracks join.
    pub threshold_jitter: f32,
}

/// Energy at or above which a track joins in groove / build / drop sections.
fn base_threshold(t: Track) -> f32 {
    match t {
        Track::Kick | Track::Bass | Track::HatClosed => 0.0,
        Track::Rumble => 0.2,
        Track::Clap => 0.35,
        Track::HatOpen => 0.5,
        Track::Rim => 0.55,
        Track::Stab => 0.6,
        Track::Lead => 0.65,
        Track::Ride => 0.7,
        Track::Arp => 0.75,
        Track::Tom => 0.8,
        Track::Drone => 0.85,
        Track::Pad => 0.95,
        // Event-driven or never energy-driven.
        Track::Snare | Track::Crash | Track::Riser | Track::Impact => 2.0,
    }
}

// Toms are not energy driven: they only play short fills (too melodic as a pattern).
const ENERGY_TRACKS: [Track; 12] = [
    Track::Kick,
    Track::Bass,
    Track::HatClosed,
    Track::Rumble,
    Track::Clap,
    Track::HatOpen,
    Track::Rim,
    Track::Stab,
    Track::Lead,
    Track::Ride,
    Track::Arp,
    Track::Drone,
];

#[derive(Clone, Copy, Debug)]
pub struct SongPlan {
    pub sections: [Section; MAX_SECTIONS],
    pub count: u8,
    pub total_bars: u32,
    thresholds: [f32; Track::COUNT],
    variations: [Variation; MAX_PHRASES],
    /// Random flavour per section (riser, roll, drop style ...).
    pub styles: [SectionStyle; MAX_SECTIONS],
    /// Tracks the song uses at all (some DNA leaves e.g. the arp out).
    pub palette: TrackSet,
    /// The short synth that carries the song.
    pub main_synth: Track,
    /// Short synth that replaces the main one in breakdowns, if any.
    pub alt_synth: Option<Track>,
}

fn len_pick(w: &Weighted<u32>, rng: &mut Rng) -> u32 {
    (w.pick(rng) / PHRASE_BARS).max(1) * PHRASE_BARS
}

impl SongPlan {
    pub fn generate(
        rng: &mut Rng,
        profile: &Profile,
        palette: TrackSet,
        main_synth: Track,
        alt_synth: Option<Track>,
    ) -> SongPlan {
        let r = &profile.structure;
        let target = {
            let lo = r.target_bars[0] / PHRASE_BARS;
            let hi = r.target_bars[1] / PHRASE_BARS;
            rng.range_i32(lo as i32, hi as i32) as u32 * PHRASE_BARS
        };
        let outro = (r.outro_bars / PHRASE_BARS).max(2) * PHRASE_BARS;

        let intro = (r.intro_bars / PHRASE_BARS).max(2) * PHRASE_BARS;
        let mut list: Vec<(SectionKind, u32)> = vec![(SectionKind::Intro, intro)];
        let mut total = intro;
        let mut has_drop = false;
        let mut prev = SectionKind::Intro;
        let mut groove_run = 0;

        let min_of = |w: &Weighted<u32>| {
            w.values
                .iter()
                .copied()
                .min()
                .unwrap_or(PHRASE_BARS)
                .max(PHRASE_BARS)
        };
        // A breakdown or build always resolves into a drop: plan the whole run at once.
        let run_for =
            |kind: SectionKind, rng: &mut Rng, minimal: bool| -> Vec<(SectionKind, u32)> {
                let len = |w: &Weighted<u32>, rng: &mut Rng| {
                    if minimal {
                        min_of(w)
                    } else {
                        len_pick(w, rng)
                    }
                };
                match kind {
                    SectionKind::Breakdown => {
                        let mut v = vec![(SectionKind::Breakdown, len(&r.breakdown_bars, rng))];
                        if rng.chance(r.breakdown_to_build_prob) {
                            v.push((SectionKind::Build, len(&r.build_bars, rng)));
                        }
                        v.push((SectionKind::Drop, len(&r.drop_bars, rng)));
                        v
                    }
                    SectionKind::Build => vec![
                        (SectionKind::Build, len(&r.build_bars, rng)),
                        (SectionKind::Drop, len(&r.drop_bars, rng)),
                    ],
                    SectionKind::Drop => vec![(SectionKind::Drop, len(&r.drop_bars, rng))],
                    _ => vec![(SectionKind::Groove, len(&r.groove_bars, rng))],
                }
            };

        loop {
            let mut next = match prev {
                SectionKind::Intro => r.after_intro.pick(rng),
                SectionKind::Groove => r.after_groove.pick(rng),
                _ => r.after_drop.pick(rng),
            };
            // No endless grooves: after 32 bars of groove something has to happen.
            if next == SectionKind::Groove && groove_run >= 32 {
                next = if rng.chance(0.5) {
                    SectionKind::Breakdown
                } else {
                    SectionKind::Build
                };
            }
            let room = target.saturating_sub(total + outro);
            let mut run = run_for(next, rng, false);
            let run_len = |run: &Vec<(SectionKind, u32)>| run.iter().map(|s| s.1).sum::<u32>();
            if run_len(&run) > room {
                run = run_for(next, rng, true);
            }
            if run_len(&run) > room && next != SectionKind::Groove {
                run = run_for(SectionKind::Build, rng, true);
            }
            if run_len(&run) > room {
                run = vec![(SectionKind::Groove, PHRASE_BARS * 2)];
            }
            if run_len(&run) > room || list.len() + run.len() >= MAX_SECTIONS - 1 {
                break;
            }
            total += run_len(&run);
            has_drop |= run.iter().any(|s| s.0 == SectionKind::Drop);
            for (kind, bars) in run {
                groove_run = if kind == SectionKind::Groove {
                    groove_run + bars
                } else {
                    0
                };
                prev = kind;
                // Merge consecutive grooves into one section.
                match list.last_mut() {
                    Some(last) if last.0 == SectionKind::Groove && kind == SectionKind::Groove => {
                        last.1 += bars
                    }
                    _ => list.push((kind, bars)),
                }
            }
        }
        if !has_drop {
            // Every song gets at least one build and drop, even if it runs long.
            list.push((SectionKind::Build, PHRASE_BARS * 2));
            list.push((SectionKind::Drop, PHRASE_BARS * 4));
            total += PHRASE_BARS * 6;
        }
        // A short song gets a longer last section before the outro.
        if total + outro < r.target_bars[0] {
            let extra = (r.target_bars[0] - total - outro).div_ceil(PHRASE_BARS) * PHRASE_BARS;
            match list.last_mut() {
                Some(last) if matches!(last.0, SectionKind::Groove | SectionKind::Drop) => {
                    last.1 += extra
                }
                _ => list.push((SectionKind::Groove, extra)),
            }
            total += extra;
        }
        list.push((SectionKind::Outro, outro));
        total += outro;

        let mut sections = [Section {
            kind: SectionKind::Outro,
            start: 0,
            bars: 0,
        }; MAX_SECTIONS];
        let mut start = 0;
        for (i, (kind, bars)) in list.iter().enumerate() {
            sections[i] = Section {
                kind: *kind,
                start,
                bars: *bars,
            };
            start += bars;
        }

        let mut thresholds = [2.0f32; Track::COUNT];
        for t in ENERGY_TRACKS {
            let base = base_threshold(t);
            thresholds[t.index()] = if base <= 0.0 {
                0.0
            } else {
                (base + rng.range_f32(-r.threshold_jitter, r.threshold_jitter)).clamp(0.1, 0.98)
            };
        }

        let mut variations = [Variation::default(); MAX_PHRASES];
        const PERC: [Track; 3] = [Track::Rim, Track::Tom, Track::Ride];
        for v in variations.iter_mut() {
            v.lead_up = rng.chance(0.2);
            v.lead_thin = rng.chance(0.2);
            v.drop_perc = rng.chance(0.3).then(|| PERC[rng.range_i32(0, 2) as usize]);
            v.no_open_hat = rng.chance(0.12);
        }

        // Styles come from their own stream so they do not reshuffle the structure.
        let mut st = rng.fork("styles");
        let mut styles = [SectionStyle::default(); MAX_SECTIONS];
        for style in styles.iter_mut() {
            *style = SectionStyle {
                riser: r.riser.pick(&mut st),
                roll: r.roll.pick(&mut st),
                highpass: st.chance(r.build_highpass_prob),
                drop: r.drop_style.pick(&mut st),
                impact: r.drop_impact.pick(&mut st),
                pause_before: st.chance(r.drop_pause_prob),
            };
        }

        SongPlan {
            sections,
            count: list.len() as u8,
            total_bars: total,
            thresholds,
            variations,
            styles,
            palette,
            main_synth,
            alt_synth,
        }
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections[..self.count as usize]
    }

    pub fn section_index_at(&self, bar: u32) -> usize {
        self.sections()
            .iter()
            .position(|s| bar < s.end())
            .unwrap_or(self.count as usize - 1)
    }

    pub fn section_at(&self, bar: u32) -> Section {
        self.sections[self.section_index_at(bar)]
    }

    pub fn first(&self, kind: SectionKind) -> Option<Section> {
        self.sections().iter().copied().find(|s| s.kind == kind)
    }

    pub fn outro_start(&self) -> u32 {
        self.sections[self.count as usize - 1].start
    }

    fn next_kind(&self, idx: usize) -> Option<SectionKind> {
        (idx + 1 < self.count as usize).then(|| self.sections[idx + 1].kind)
    }

    fn energy_at(&self, s: &Section, pos_in_section: f32) -> f32 {
        let [a, b] = s.kind.energy();
        a + (b - a) * (pos_in_section / s.bars as f32).clamp(0.0, 1.0)
    }

    pub fn bar(&self, bar: u32) -> BarPlan {
        use Track::*;
        let idx = self.section_index_at(bar);
        let s = self.sections[idx];
        let sb = bar.saturating_sub(s.start);
        let left = s.bars.saturating_sub(sb); // bars left including this one
        let energy = self.energy_at(&s, sb as f32);
        let variation = self.variations[(bar / PHRASE_BARS) as usize % MAX_PHRASES];
        let by_energy = |e: f32| {
            ENERGY_TRACKS
                .iter()
                .filter(|t| e >= self.thresholds[t.index()])
                .fold(TrackSet::NONE, |m, t| m.with(*t))
        };

        let mut fill = Fill::None;
        let mut roll_every = 0u8;
        let style = self.styles[idx];
        let mut roll_kind = style.roll;
        let mut riser_kind = style.riser;
        let mut build_progress = 0.0;
        let mut impact = false;
        let mut crash = false;

        let mut active = match s.kind {
            SectionKind::Intro => {
                // DJ-friendly: drums and reduced bass only.
                let mut a = TrackSet::of(&[Kick, HatClosed]);
                if sb >= 8 {
                    a = a.with(Rumble).with(Rim);
                }
                if sb >= s.bars / 2 {
                    a = a.with(Bass).with(Clap);
                }
                if left <= 8 {
                    a = a.with(HatOpen);
                }
                a
            }
            SectionKind::Outro => {
                let mut a = TrackSet::of(&[Kick, HatClosed]);
                if sb < s.bars - 8 {
                    a = a.with(Rumble);
                }
                if sb < s.bars / 2 {
                    a = a.with(Bass).with(Clap).with(Rim);
                }
                if sb < 8 {
                    a = a.with(HatOpen);
                }
                a
            }
            SectionKind::Groove => by_energy(energy).minus(TrackSet::of(&[Pad, Drone])),
            SectionKind::Drop => {
                if sb == 0 {
                    impact = style.impact == DropImpact::ImpactCrash;
                    crash = style.impact != DropImpact::None;
                }
                let full = by_energy(energy + 0.05)
                    .with(Pad)
                    .with(Drone)
                    .minus(TrackSet::of(&[Snare]));
                let layers = SHORT_SYNTHS.union(TrackSet::of(&[Pad, Drone, Ride]));
                match style.drop {
                    DropStyle::BassFirst if sb < 8 => full.minus(layers),
                    DropStyle::Stripped if sb < 8 => {
                        full.and(TrackSet::of(&[Kick, Bass, Rumble, HatClosed, Clap]))
                    }
                    DropStyle::Stripped if sb < 16 => full.minus(layers).with(HatOpen),
                    _ => full,
                }
            }
            SectionKind::Build => {
                build_progress = sb as f32 / s.bars as f32;
                let mut a = by_energy(energy).minus(TrackSet::of(&[Pad, Tom]));
                if style.riser != RiserKind::None {
                    a = a.with(Riser);
                }
                // Roll over the second half, getting denser.
                let half = s.bars / 2;
                if sb >= half && style.roll != RollKind::None {
                    a = a.with(roll_track(style.roll));
                    roll_every = if left <= 1 {
                        1
                    } else if left <= half / 2 {
                        2
                    } else {
                        4
                    };
                }
                let pause = self.next_kind(idx) == Some(SectionKind::Drop)
                    && self.styles[idx + 1].pause_before;
                if left == 1 && pause {
                    let mut p = TrackSet::of(&[Riser]);
                    if style.roll != RollKind::None {
                        p = p.with(roll_track(style.roll));
                    }
                    a = p;
                }
                a
            }
            SectionKind::Breakdown => {
                if sb == 0 {
                    impact = true;
                    crash = true;
                }
                let mut a = TrackSet::of(&[Pad, Lead, Drone]);
                if sb >= s.bars / 2 {
                    a = a.with(Stab).with(HatClosed).with(Arp);
                }
                if left <= 2 {
                    a = a.with(Clap);
                }
                // Straight into a drop: a short run-up of its own over the last 4 bars.
                if self.next_kind(idx) == Some(SectionKind::Drop) && left <= 4 {
                    if riser_kind == RiserKind::None {
                        riser_kind = RiserKind::Noise;
                    }
                    a = a.with(Riser).with(HatClosed);
                    build_progress = (4 - left) as f32 / 4.0;
                    if left <= 2 {
                        roll_kind = RollKind::Snare;
                        roll_every = if left == 1 { 1 } else { 2 };
                        a = a.with(Snare);
                    }
                }
                a
            }
        };

        // Phrase fills in steady sections.
        let phrase_end = (sb + 1) % 16 == 0 && left > 1;
        if matches!(s.kind, SectionKind::Groove | SectionKind::Drop) && phrase_end {
            fill = Fill::Small;
        }
        if s.kind == SectionKind::Groove
            && left == 1
            && self.next_kind(idx) == Some(SectionKind::Breakdown)
        {
            fill = Fill::Small;
        }
        if s.kind == SectionKind::Breakdown && left == 1 && roll_every == 0 {
            fill = Fill::Roll;
            active = active.with(Snare);
        }
        if s.kind != SectionKind::Build && s.kind != SectionKind::Breakdown && sb == 0 && bar > 0 {
            crash = crash || matches!(s.kind, SectionKind::Groove | SectionKind::Drop);
        }

        // Variation only in steady sections.
        if matches!(s.kind, SectionKind::Groove | SectionKind::Drop) {
            if let Some(t) = variation.drop_perc {
                active = active.without(t);
            }
            if variation.no_open_hat {
                active = active.without(HatOpen);
            }
        }
        let variation = if matches!(s.kind, SectionKind::Groove | SectionKind::Drop) {
            variation
        } else {
            Variation::default()
        };

        // One short synth at a time: whenever the arrangement asks for any of them, the
        // song's main synth plays (or its alternative in breakdowns), never two at once.
        if !active.and(SHORT_SYNTHS).is_empty() {
            let pick = match (s.kind, self.alt_synth) {
                (SectionKind::Breakdown, Some(alt)) => alt,
                _ => self.main_synth,
            };
            active = active.minus(SHORT_SYNTHS).with(pick);
        }

        BarPlan {
            section: s.kind,
            section_bar: sb,
            section_bars: s.bars,
            energy,
            active: active
                .union(if impact {
                    TrackSet::of(&[Impact])
                } else {
                    TrackSet::NONE
                })
                .and(self.palette),
            fill,
            crash,
            impact,
            roll_every,
            roll_kind,
            riser_kind,
            build_progress,
            variation,
        }
    }

    /// Automation at a fractional bar position.
    pub fn automation(&self, pos_bars: f32) -> Automation {
        let bar = pos_bars.max(0.0) as u32;
        let idx = self.section_index_at(bar);
        let s = self.sections[idx];
        let x = ((pos_bars - s.start as f32) / s.bars as f32).clamp(0.0, 1.0);
        let lerp = |a: f32, b: f32| a + (b - a) * x;
        let mut a = Automation {
            lead_filter: 1.0,
            bass_filter: 1.0,
            highpass: 0.0,
            fx_send: 1.0,
            riser: 0.0,
            riser_progress: 0.0,
            lowpass: 0.0,
        };
        let style = self.styles[idx];
        let into_drop = self.next_kind(idx) == Some(SectionKind::Drop);
        match s.kind {
            SectionKind::Intro => {
                a.bass_filter = lerp(0.35, 0.6);
            }
            SectionKind::Outro => {
                a.bass_filter = lerp(0.6, 0.3);
            }
            SectionKind::Groove => {
                a.lead_filter = lerp(0.45, 0.8);
                a.bass_filter = (0.75 + x * 2.0).min(1.0);
            }
            SectionKind::Build => {
                a.lead_filter = lerp(0.35, 1.0);
                a.fx_send = lerp(1.0, 1.8);
                if style.riser != RiserKind::None {
                    a.riser = x * x;
                    a.riser_progress = x;
                }
                // Filter build (not every time): high-pass sweeps in over the last quarter.
                if style.highpass {
                    a.highpass = ((x - 0.75) * 4.0).clamp(0.0, 1.0) * 0.8;
                }
            }
            SectionKind::Breakdown => {
                a.lead_filter = lerp(0.15, 0.55);
                a.fx_send = 2.2;
                if into_drop {
                    // Short run-up over the last 4 bars.
                    let run = ((pos_bars - (s.end() as f32 - 4.0)) / 4.0).clamp(0.0, 1.0);
                    a.riser = run * run;
                    a.riser_progress = run;
                }
            }
            SectionKind::Drop => {
                a.lead_filter = 1.0;
                if style.drop == DropStyle::FilterOpen {
                    let open = ((pos_bars - s.start as f32) / 8.0).clamp(0.0, 1.0);
                    a.lowpass = (1.0 - open) * (1.0 - open);
                }
            }
        }
        a
    }

    pub fn describe(&self, bpm: f32) -> String {
        let spb = seconds_per_bar(bpm as f64);
        let mut out = String::new();
        for (i, s) in self.sections().iter().enumerate() {
            let t = s.start as f64 * spb;
            let st = self.styles[i];
            let detail = match s.kind {
                SectionKind::Build => format!(
                    "riser {:?}, roll {:?}{}",
                    st.riser,
                    st.roll,
                    if st.highpass { ", high-pass" } else { "" }
                ),
                SectionKind::Drop => format!(
                    "{:?}, {:?}{}",
                    st.drop,
                    st.impact,
                    if st.pause_before && i > 0 && self.sections[i - 1].kind == SectionKind::Build {
                        ", pause before"
                    } else {
                        ""
                    }
                ),
                _ => String::new(),
            };
            out.push_str(&format!(
                "  bar {:>3}  {:>2}:{:02}  {:<9} {:>2} bars  {}\n",
                s.start + 1,
                (t / 60.0) as u32,
                t as u32 % 60,
                s.kind.name(),
                s.bars,
                detail.to_lowercase()
            ));
        }
        let t = self.total_bars as f64 * spb;
        out.push_str(&format!(
            "  total {} bars, {}:{:02}",
            self.total_bars,
            (t / 60.0) as u32,
            t as u32 % 60
        ));
        out
    }
}

fn roll_track(r: RollKind) -> Track {
    match r {
        RollKind::Clap => Track::Clap,
        RollKind::Kick => Track::Kick,
        _ => Track::Snare,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plans() -> impl Iterator<Item = SongPlan> {
        let p = Profile::hard_techno_simple();
        (0..300u64).map(move |s| {
            SongPlan::generate(
                &mut Rng::new(s),
                &p,
                TrackSet::ALL,
                Track::Lead,
                Some(Track::Arp),
            )
        })
    }

    #[test]
    fn grammar_holds() {
        for plan in plans() {
            let s = plan.sections();
            assert_eq!(s[0].kind, SectionKind::Intro);
            assert_eq!(s.last().unwrap().kind, SectionKind::Outro);
            assert!(s.iter().any(|x| x.kind == SectionKind::Drop));
            for w in s.windows(2) {
                match w[0].kind {
                    SectionKind::Breakdown => {
                        assert!(matches!(w[1].kind, SectionKind::Build | SectionKind::Drop))
                    }
                    SectionKind::Build => assert_eq!(w[1].kind, SectionKind::Drop),
                    _ => {}
                }
                assert_eq!(w[0].end(), w[1].start);
            }
        }
    }

    #[test]
    fn phrase_aligned_and_dj_friendly() {
        let p = Profile::hard_techno_simple();
        for plan in plans() {
            for s in plan.sections() {
                assert_eq!(s.bars % PHRASE_BARS, 0);
            }
            assert!(plan.sections()[0].bars >= 32);
            assert!(plan.sections().last().unwrap().bars >= 32);
            assert!(plan.total_bars >= p.structure.target_bars[0]);
            assert!(plan.total_bars <= p.structure.target_bars[1] + 48);
            // Intro and outro: drums and reduced bass only.
            let melodic = TrackSet::of(&[
                Track::Stab,
                Track::Lead,
                Track::Pad,
                Track::Arp,
                Track::Drone,
            ]);
            for bar in (0..32).chain(plan.outro_start()..plan.total_bars) {
                assert!(plan.bar(bar).active.and(melodic).is_empty());
            }
        }
    }

    #[test]
    fn builds_and_drops_vary() {
        use std::collections::HashSet;
        let mut risers = HashSet::new();
        let mut drops = HashSet::new();
        let mut long_breakdowns = 0;
        let mut breakdowns = 0;
        for plan in plans() {
            for (i, s) in plan.sections().iter().enumerate() {
                match s.kind {
                    SectionKind::Build => {
                        risers.insert(format!("{:?}", plan.styles[i].riser));
                    }
                    SectionKind::Drop => {
                        drops.insert(format!("{:?}", plan.styles[i].drop));
                    }
                    SectionKind::Breakdown => {
                        breakdowns += 1;
                        long_breakdowns += (s.bars >= 32) as u32;
                    }
                    _ => {}
                }
            }
        }
        assert!(risers.len() >= 4, "{risers:?}");
        assert_eq!(drops.len(), 4, "{drops:?}");
        assert!(
            long_breakdowns * 5 < breakdowns,
            "32-bar breakdowns should be rare"
        );
    }

    #[test]
    fn breakdown_has_no_kick() {
        for plan in plans() {
            for bar in 0..plan.total_bars {
                let b = plan.bar(bar);
                if b.section == SectionKind::Breakdown {
                    assert!(!b.active.contains(Track::Kick));
                    assert!(!b.active.contains(Track::Bass));
                }
                if b.section == SectionKind::Drop {
                    assert!(b.active.contains(Track::Kick));
                }
            }
        }
    }
}
