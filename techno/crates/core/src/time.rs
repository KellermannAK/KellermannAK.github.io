//! Musical time base. Everything is scheduled in ticks, never in seconds.

pub const PPQ: u32 = 96;
pub const BEATS_PER_BAR: u32 = 4;
pub const STEPS_PER_BEAT: u32 = 4;
pub const STEPS_PER_BAR: u32 = BEATS_PER_BAR * STEPS_PER_BEAT;
pub const TICKS_PER_STEP: u32 = PPQ / STEPS_PER_BEAT;
pub const TICKS_PER_BAR: u32 = PPQ * BEATS_PER_BAR;

/// Absolute position in ticks since song start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tick(pub u64);

impl Tick {
    pub const fn from_bar(bar: u32) -> Tick {
        Tick(bar as u64 * TICKS_PER_BAR as u64)
    }
    pub const fn from_bar_step(bar: u32, step: u32) -> Tick {
        Tick(bar as u64 * TICKS_PER_BAR as u64 + step as u64 * TICKS_PER_STEP as u64)
    }
    pub const fn bar(self) -> u32 {
        (self.0 / TICKS_PER_BAR as u64) as u32
    }
    pub const fn tick_in_bar(self) -> u32 {
        (self.0 % TICKS_PER_BAR as u64) as u32
    }
    /// 16th step within the bar, 0..16.
    pub const fn step_in_bar(self) -> u32 {
        self.tick_in_bar() / TICKS_PER_STEP
    }
    pub const fn tick_in_step(self) -> u32 {
        self.tick_in_bar() % TICKS_PER_STEP
    }
    pub fn to_musical(self) -> MusicalTime {
        let tib = self.tick_in_bar();
        MusicalTime {
            bar: self.bar(),
            beat: tib / PPQ,
            tick: tib % PPQ,
        }
    }
}

/// Human-readable position: bar (0-based), beat 0..4, tick 0..PPQ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusicalTime {
    pub bar: u32,
    pub beat: u32,
    pub tick: u32,
}

impl core::fmt::Display for MusicalTime {
    /// 1-based like a DAW: "17.1.0" is the downbeat of bar 17.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{}.{}", self.bar + 1, self.beat + 1, self.tick)
    }
}

pub fn samples_per_tick(bpm: f64, sample_rate: f64) -> f64 {
    sample_rate * 60.0 / (bpm * PPQ as f64)
}

pub fn seconds_per_bar(bpm: f64) -> f64 {
    60.0 / bpm * BEATS_PER_BAR as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_decomposition() {
        let t = Tick::from_bar_step(3, 5);
        assert_eq!(t.bar(), 3);
        assert_eq!(t.step_in_bar(), 5);
        assert_eq!(t.tick_in_step(), 0);
        assert_eq!(t.to_musical().to_string(), "4.2.24");
    }
}
