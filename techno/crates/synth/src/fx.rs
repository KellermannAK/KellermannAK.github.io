//! Insert effects (stereo in, stereo out). Allocation-free after construction.
//!
//! Each effect has an `amount` (0..1) that the arrangement can automate, e.g. more
//! phasing in breakdowns. `amount = 0` is (close to) bypass.

use core::f32::consts::{PI, TAU};

use crate::dsp::{fast_tanh, OnePole, ANTI_DENORMAL};
pub use techno_core::profile::InsertKind;

/// Parameters fixed per song (sampled by the DNA within profile ranges).
#[derive(Clone, Copy, Debug)]
pub struct InsertParams {
    pub kind: InsertKind,
    /// LFO rate in Hz (chorus / phaser), or beats per cycle for the tremolo.
    pub rate: f32,
    /// Base intensity 0..1, scaled by the automated amount.
    pub depth: f32,
}

impl InsertParams {
    pub const NONE: InsertParams = InsertParams {
        kind: InsertKind::None,
        rate: 0.0,
        depth: 0.0,
    };
}

// ----------------------------------------------------------------------------- chorus

/// Two modulated delay lines (one per side, LFOs in quadrature).
pub struct Chorus {
    buf: [Vec<f32>; 2],
    pos: usize,
    phase: f32,
    rate: f32,
    sr: f32,
}

impl Chorus {
    pub fn new(rate: f32, sr: f32) -> Chorus {
        let n = (0.04 * sr) as usize + 4;
        Chorus {
            buf: [vec![0.0; n], vec![0.0; n]],
            pos: 0,
            phase: 0.0,
            rate,
            sr,
        }
    }

    #[inline]
    fn read(buf: &[f32], pos: usize, delay: f32) -> f32 {
        let n = buf.len();
        let d = delay.clamp(1.0, (n - 2) as f32);
        let i = d as usize;
        let frac = d - i as f32;
        let a = buf[(pos + n - i) % n];
        let b = buf[(pos + n - i - 1) % n];
        a + (b - a) * frac
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        let n = self.buf[0].len();
        self.buf[0][self.pos] = l;
        self.buf[1][self.pos] = r;
        self.phase = (self.phase + self.rate / self.sr).fract();
        let base = 0.012 * self.sr;
        let depth = 0.006 * self.sr * amount.clamp(0.0, 1.0);
        let dl = base + depth * (self.phase * TAU).sin();
        let dr = base + depth * (self.phase * TAU + PI / 2.0).sin();
        let wl = Self::read(&self.buf[0], self.pos, dl);
        let wr = Self::read(&self.buf[1], self.pos, dr);
        self.pos = (self.pos + 1) % n;
        let mix = 0.5 * amount.clamp(0.0, 1.0);
        (
            l * (1.0 - mix * 0.5) + wr * mix,
            r * (1.0 - mix * 0.5) + wl * mix,
        )
    }
}

// ----------------------------------------------------------------------------- phaser

const PHASER_STAGES: usize = 6;

/// Six first-order all-pass stages swept by an LFO, with feedback.
pub struct Phaser {
    state: [[(f32, f32); PHASER_STAGES]; 2],
    fb: [f32; 2],
    phase: f32,
    rate: f32,
    sr: f32,
}

impl Phaser {
    pub fn new(rate: f32, sr: f32) -> Phaser {
        Phaser {
            state: [[(0.0, 0.0); PHASER_STAGES]; 2],
            fb: [0.0; 2],
            phase: 0.0,
            rate,
            sr,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        let amount = amount.clamp(0.0, 1.0);
        self.phase = (self.phase + self.rate / self.sr).fract();
        let mut out = [l, r];
        for (ch, x) in out.iter_mut().enumerate() {
            // Right channel LFO offset for width.
            let lfo = 0.5 + 0.5 * ((self.phase + ch as f32 * 0.25) * TAU).sin();
            let fc = 200.0 * (lfo * 3.5 * core::f32::consts::LN_2 * 2.0).exp(); // 200 Hz .. ~25 kHz cap below
            let t = (PI * fc.min(self.sr * 0.45) / self.sr).tan();
            let g = (t - 1.0) / (t + 1.0);
            let mut y = *x + self.fb[ch] * 0.5 * amount;
            for s in self.state[ch].iter_mut() {
                let (x1, y1) = *s;
                let o = g * y + x1 - g * y1 + ANTI_DENORMAL;
                *s = (y, o);
                y = o;
            }
            self.fb[ch] = y;
            *x = *x * (1.0 - 0.5 * amount) + y * 0.5 * amount;
        }
        (out[0], out[1])
    }
}

// ----------------------------------------------------------------------------- crusher

/// Bit depth and sample rate reduction (grit), smoothed a little so it is not harsh.
pub struct Crusher {
    hold: [f32; 2],
    count: f32,
    smooth: [OnePole; 2],
}

impl Crusher {
    pub fn new(sr: f32) -> Crusher {
        Crusher {
            hold: [0.0; 2],
            count: 0.0,
            smooth: [OnePole::new(9000.0, sr), OnePole::new(9000.0, sr)],
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        let amount = amount.clamp(0.0, 1.0);
        // Sample-and-hold factor 1 .. 8, 16 .. 6 bits.
        let factor = 1.0 + 7.0 * amount;
        let levels = 2f32.powf(16.0 - 10.0 * amount);
        self.count += 1.0;
        if self.count >= factor {
            self.count -= factor;
            self.hold = [l, r];
        }
        let q = |x: f32| (x * levels).round() / levels;
        let cl = self.smooth[0].lp(q(self.hold[0]));
        let cr = self.smooth[1].lp(q(self.hold[1]));
        let mix = amount.min(0.7);
        (l * (1.0 - mix) + cl * mix, r * (1.0 - mix) + cr * mix)
    }
}

// ----------------------------------------------------------------------------- tremolo

/// Tempo-synced tremolo / autopan: the gain moves left and right in opposite phase.
pub struct Tremolo {
    phase: f32,
    /// Cycles per sample.
    inc: f32,
}

impl Tremolo {
    /// `beats_per_cycle`: 0.5 = 8ths, 1 = quarters, 2 = half notes.
    pub fn new(bpm: f32, beats_per_cycle: f32, sr: f32) -> Tremolo {
        Tremolo {
            phase: 0.0,
            inc: bpm / 60.0 / beats_per_cycle.max(0.125) / sr,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        let amount = amount.clamp(0.0, 1.0) * 0.7;
        self.phase = (self.phase + self.inc).fract();
        let m = (self.phase * TAU).sin();
        (
            l * (1.0 - amount * (0.5 + 0.5 * m)),
            r * (1.0 - amount * (0.5 - 0.5 * m)),
        )
    }
}

// ----------------------------------------------------------------------------- tape

/// Warm, slightly asymmetric saturation with a gentle top-end roll-off.
pub struct Tape {
    tone: [OnePole; 2],
}

impl Tape {
    pub fn new(sr: f32) -> Tape {
        Tape {
            tone: [OnePole::new(7000.0, sr), OnePole::new(7000.0, sr)],
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        let drive = 1.0 + 3.0 * amount.clamp(0.0, 1.0);
        let sat = |x: f32| {
            let y = fast_tanh((x + 0.08) * drive) - fast_tanh(0.08 * drive);
            y / fast_tanh(drive).max(0.3)
        };
        // Saturation raises the RMS; the trim keeps the perceived level about equal.
        let trim = 1.0 / (1.0 + 0.6 * amount.clamp(0.0, 1.0));
        let wl = self.tone[0].lp(sat(l)) * trim;
        let wr = self.tone[1].lp(sat(r)) * trim;
        let mix = amount.clamp(0.0, 1.0);
        (l * (1.0 - mix) + wl * mix, r * (1.0 - mix) + wr * mix)
    }
}

// ----------------------------------------------------------------------------- slot

/// One insert slot holding the chosen effect.
pub enum Insert {
    None,
    Chorus(Chorus),
    Phaser(Phaser),
    Crusher(Crusher),
    Tremolo(Tremolo),
    Tape(Tape),
}

impl Insert {
    pub fn new(p: InsertParams, bpm: f32, sr: f32) -> Insert {
        match p.kind {
            InsertKind::None => Insert::None,
            InsertKind::Chorus => Insert::Chorus(Chorus::new(p.rate, sr)),
            InsertKind::Phaser => Insert::Phaser(Phaser::new(p.rate, sr)),
            InsertKind::Crusher => Insert::Crusher(Crusher::new(sr)),
            InsertKind::Tremolo => Insert::Tremolo(Tremolo::new(bpm, p.rate, sr)),
            InsertKind::Tape => Insert::Tape(Tape::new(sr)),
        }
    }

    pub fn is_none(&self) -> bool {
        matches!(self, Insert::None)
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32, amount: f32) -> (f32, f32) {
        match self {
            Insert::None => (l, r),
            Insert::Chorus(f) => f.process(l, r, amount),
            Insert::Phaser(f) => f.process(l, r, amount),
            Insert::Crusher(f) => f.process(l, r, amount),
            Insert::Tremolo(f) => f.process(l, r, amount),
            Insert::Tape(f) => f.process(l, r, amount),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(kind: InsertKind) -> (f32, bool) {
        let sr = 48000.0;
        let mut fx = Insert::new(
            InsertParams {
                kind,
                rate: 0.5,
                depth: 1.0,
            },
            135.0,
            sr,
        );
        let mut peak = 0.0f32;
        let mut finite = true;
        for i in 0..48000 {
            let x = (i as f32 * 220.0 / sr * TAU).sin() * 0.8;
            let (l, r) = fx.process(x, x, 1.0);
            peak = peak.max(l.abs()).max(r.abs());
            finite &= l.is_finite() && r.is_finite();
        }
        (peak, finite)
    }

    #[test]
    fn all_inserts_are_stable_and_bounded() {
        for k in [
            InsertKind::None,
            InsertKind::Chorus,
            InsertKind::Phaser,
            InsertKind::Crusher,
            InsertKind::Tremolo,
            InsertKind::Tape,
        ] {
            let (peak, finite) = run(k);
            assert!(finite, "{k:?} produced NaN/inf");
            assert!(peak < 1.6 && peak > 0.05, "{k:?} peak {peak}");
        }
    }
}
