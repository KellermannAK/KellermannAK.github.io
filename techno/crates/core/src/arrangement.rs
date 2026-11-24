//! Placeholder arrangement until milestone 4 (song grammar with sections and energy).
//!
//! It layers the tracks in 16-bar phrases so the DNA can be heard in context:
//! intro -> groove -> build -> breakdown -> drop -> outro. Deterministic, seed-independent.

use crate::time::STEPS_PER_BAR;
use crate::tracks::{Track, TrackSet};

/// What happens in one bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarPlan {
    pub active: TrackSet,
    /// Drum fill in the last two beats (snare roll before a drop, toms otherwise).
    pub fill: Fill,
    /// Crash on the downbeat.
    pub crash: bool,
    /// Lead filter opening 0..1.
    pub lead_filter: f32,
    /// Section label for display.
    pub section: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    None,
    /// Toms / rim variation in the last beat.
    Small,
    /// Snare roll over the last two beats, rising.
    Roll,
}

pub const DEMO_BARS: u32 = 128;

/// Placeholder plan: 8 phrases of 16 bars (about 3:50 at 134 BPM).
pub fn demo_plan(bar: u32) -> BarPlan {
    use Track::*;
    let phrase = bar / 16;
    let in_phrase = bar % 16;
    let last_bar = in_phrase == 15;

    let drums_core = TrackSet::of(&[Kick, HatClosed, Rumble]);
    let groove = drums_core.with(Bass).with(Clap).with(HatOpen);
    let full_drums = groove.with(Rim).with(Ride);

    let (section, mut active, lead_filter) = match phrase {
        0 => ("intro", drums_core, 0.0),
        1 => ("groove", groove, 0.0),
        2 => ("groove+", full_drums.with(Stab), 0.0),
        3 => {
            let f = 0.25 + 0.75 * in_phrase as f32 / 15.0;
            ("build", full_drums.with(Stab).with(Lead).with(Tom), f)
        }
        4 => {
            let pad_only = TrackSet::of(&[Pad, Lead, HatClosed, Ride]);
            let f = 0.15 + 0.45 * in_phrase as f32 / 15.0;
            let set = if in_phrase >= 8 {
                pad_only.with(Clap).with(Stab)
            } else {
                pad_only
            };
            ("breakdown", set, f)
        }
        5 | 6 => (
            "drop",
            full_drums.with(Stab).with(Lead).with(Tom).with(Pad),
            1.0,
        ),
        _ => {
            let set = if in_phrase < 8 {
                groove.with(Rim)
            } else {
                drums_core
            };
            ("outro", set, 0.0)
        }
    };

    // Breathing room: the last bar before a drop drops the kick and bass on beats 3-4
    // is handled by the engine via `Fill::Roll`; here we only pick the fill type.
    let fill = if last_bar && phrase == 4 {
        Fill::Roll
    } else if last_bar && phrase < 7 {
        Fill::Small
    } else {
        Fill::None
    };
    if fill == Fill::Roll {
        active = active.with(Snare);
    }
    let crash = in_phrase == 0 && matches!(phrase, 1 | 2 | 3 | 5 | 6);
    BarPlan {
        active,
        fill,
        crash,
        lead_filter,
        section,
    }
}

/// Steps in a bar where a fill replaces the regular pattern.
pub fn fill_starts_at(fill: Fill) -> u32 {
    match fill {
        Fill::None => STEPS_PER_BAR,
        Fill::Small => 12,
        Fill::Roll => 8,
    }
}
