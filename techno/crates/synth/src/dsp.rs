//! DSP primitives. Everything here is allocation-free after construction.

use core::f32::consts::{PI, TAU};

#[inline]
pub fn fast_tanh(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// Saturation with roughly unity peak level independent of `drive`.
#[inline]
pub fn saturate(x: f32, drive: f32) -> f32 {
    fast_tanh(x * drive) / fast_tanh(drive).max(0.1)
}

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    (db * (core::f32::consts::LN_10 / 20.0)).exp()
}

#[inline]
pub fn gain_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-9).log10()
}

/// Per-sample multiplier that decays to 1 % (-40 dB) after `seconds`.
#[inline]
pub fn decay_coef(seconds: f32, sr: f32) -> f32 {
    (-4.605 / (seconds.max(1e-4) * sr)).exp()
}

/// One-pole smoothing coefficient with time constant `seconds`.
#[inline]
pub fn smooth_coef(seconds: f32, sr: f32) -> f32 {
    1.0 - (-1.0 / (seconds.max(1e-5) * sr)).exp()
}

/// Deterministic white noise (xorshift32).
#[derive(Clone, Debug)]
pub struct Noise {
    state: u32,
}

impl Noise {
    pub fn new(seed: u32) -> Noise {
        Noise { state: seed.max(1) }
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// Parameter smoother (one-pole).
#[derive(Clone, Debug)]
pub struct Smoother {
    pub value: f32,
    pub target: f32,
    coef: f32,
}

impl Smoother {
    pub fn new(value: f32, seconds: f32, sr: f32) -> Smoother {
        Smoother {
            value,
            target: value,
            coef: smooth_coef(seconds, sr),
        }
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        self.value += (self.target - self.value) * self.coef;
        self.value
    }
}

// ----------------------------------------------------------------------------- oscillators

#[inline]
fn poly_blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// Band-limited (PolyBLEP) oscillator.
#[derive(Clone, Debug, Default)]
pub struct Osc {
    pub phase: f32,
}

impl Osc {
    pub fn with_phase(phase: f32) -> Osc {
        Osc { phase }
    }

    #[inline]
    fn advance(&mut self, dt: f32) {
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
    }

    #[inline]
    pub fn sine(&mut self, freq: f32, sr: f32) -> f32 {
        let v = (self.phase * TAU).sin();
        self.advance(freq / sr);
        v
    }

    #[inline]
    pub fn saw(&mut self, freq: f32, sr: f32) -> f32 {
        let dt = (freq / sr).min(0.5);
        let v = 2.0 * self.phase - 1.0 - poly_blep(self.phase, dt);
        self.advance(dt);
        v
    }

    #[inline]
    pub fn square(&mut self, freq: f32, sr: f32) -> f32 {
        let dt = (freq / sr).min(0.5);
        let mut v = if self.phase < 0.5 { 1.0 } else { -1.0 };
        v += poly_blep(self.phase, dt);
        let mut p2 = self.phase + 0.5;
        if p2 >= 1.0 {
            p2 -= 1.0;
        }
        v -= poly_blep(p2, dt);
        self.advance(dt);
        v
    }

    /// Naive square, intentionally aliasing: used for metallic cymbal tones.
    #[inline]
    pub fn raw_square(&mut self, freq: f32, sr: f32) -> f32 {
        let v = if self.phase < 0.5 { 1.0 } else { -1.0 };
        self.advance(freq / sr);
        v
    }
}

/// Six detuned squares at the classic TR-808 cymbal ratios.
#[derive(Clone, Debug)]
pub struct MetalBank {
    osc: [Osc; 6],
}

const METAL_HZ: [f32; 6] = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];

impl MetalBank {
    pub fn new() -> MetalBank {
        let mut osc: [Osc; 6] = Default::default();
        for (i, o) in osc.iter_mut().enumerate() {
            o.phase = i as f32 * 0.137;
        }
        MetalBank { osc }
    }

    #[inline]
    pub fn next(&mut self, scale: f32, sr: f32) -> f32 {
        let mut s = 0.0;
        for (o, f) in self.osc.iter_mut().zip(METAL_HZ) {
            s += o.raw_square(f * scale, sr);
        }
        s * (1.0 / 6.0)
    }
}

impl Default for MetalBank {
    fn default() -> Self {
        Self::new()
    }
}

// ----------------------------------------------------------------------------- filters

/// Topology-preserving state variable filter (Zavalishin). Stable under fast modulation.
#[derive(Clone, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    g: f32,
    k: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    pub fn new(cutoff: f32, resonance: f32, sr: f32) -> Svf {
        let mut f = Svf::default();
        f.set(cutoff, resonance, sr);
        f
    }

    /// `resonance` 0..1 (0 = Butterworth-ish Q 0.5, near 1 = self-oscillation edge).
    #[inline]
    pub fn set(&mut self, cutoff: f32, resonance: f32, sr: f32) {
        let fc = cutoff.clamp(10.0, sr * 0.45);
        self.g = (PI * fc / sr).tan();
        self.k = 2.0 - 1.96 * resonance.clamp(0.0, 1.0);
        self.a1 = 1.0 / (1.0 + self.g * (self.g + self.k));
        self.a2 = self.g * self.a1;
        self.a3 = self.g * self.a2;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut {
            lp: v2,
            bp: v1,
            hp: x - self.k * v1 - v2,
        }
    }

    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.process(x).lp
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        self.process(x).hp
    }
    #[inline]
    pub fn bp(&mut self, x: f32) -> f32 {
        self.process(x).bp
    }

    pub fn reset(&mut self) {
        self.ic1 = 0.0;
        self.ic2 = 0.0;
    }
}

/// One-pole low-pass; `hp()` gives the complementary high-pass.
#[derive(Clone, Debug, Default)]
pub struct OnePole {
    z: f32,
    a: f32,
}

impl OnePole {
    pub fn new(cutoff: f32, sr: f32) -> OnePole {
        let mut f = OnePole::default();
        f.set(cutoff, sr);
        f
    }
    pub fn set(&mut self, cutoff: f32, sr: f32) {
        self.a = 1.0 - (-TAU * cutoff / sr).exp();
    }
    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.z += (x - self.z) * self.a;
        self.z
    }
    #[inline]
    pub fn hp(&mut self, x: f32) -> f32 {
        x - self.lp(x)
    }
}

// ----------------------------------------------------------------------------- envelopes

/// Exponential decay envelope.
#[derive(Clone, Debug, Default)]
pub struct Decay {
    pub value: f32,
    coef: f32,
}

impl Decay {
    pub fn set_time(&mut self, seconds: f32, sr: f32) {
        self.coef = decay_coef(seconds, sr);
    }
    pub fn trigger(&mut self, level: f32) {
        self.value = level;
    }
    pub fn kill(&mut self) {
        self.value = 0.0;
    }
    #[inline]
    pub fn next(&mut self) -> f32 {
        let v = self.value;
        self.value *= self.coef;
        if self.value < 1e-5 {
            self.value = 0.0;
        }
        v
    }
    pub fn is_active(&self) -> bool {
        self.value > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Release,
}

/// Linear attack, exponential decay towards sustain, exponential release.
#[derive(Clone, Debug)]
pub struct Adsr {
    stage: Stage,
    pub value: f32,
    attack_inc: f32,
    decay_coef: f32,
    sustain: f32,
    release_coef: f32,
}

impl Adsr {
    pub fn new(attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) -> Adsr {
        let mut e = Adsr {
            stage: Stage::Idle,
            value: 0.0,
            attack_inc: 0.0,
            decay_coef: 0.0,
            sustain,
            release_coef: 0.0,
        };
        e.set(attack, decay, sustain, release, sr);
        e
    }

    pub fn set(&mut self, attack: f32, decay: f32, sustain: f32, release: f32, sr: f32) {
        self.attack_inc = 1.0 / (attack.max(1e-4) * sr);
        self.decay_coef = decay_coef(decay, sr);
        self.sustain = sustain;
        self.release_coef = decay_coef(release, sr);
    }

    /// Restart from the current value (no click on retrigger).
    pub fn gate_on(&mut self) {
        self.stage = Stage::Attack;
    }

    pub fn gate_off(&mut self) {
        if self.stage != Stage::Idle {
            self.stage = Stage::Release;
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {}
            Stage::Attack => {
                self.value += self.attack_inc;
                if self.value >= 1.0 {
                    self.value = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.value = self.sustain + (self.value - self.sustain) * self.decay_coef;
            }
            Stage::Release => {
                self.value *= self.release_coef;
                if self.value < 1e-5 {
                    self.value = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.value
    }
}

/// Deterministic sidechain envelope, triggered by the kick (no detector, no latency).
#[derive(Clone, Debug)]
pub struct Duck {
    env: f32,
    attacking: bool,
    attack_inc: f32,
    release_coef: f32,
}

impl Duck {
    pub fn new(release: f32, sr: f32) -> Duck {
        Duck {
            env: 0.0,
            attacking: false,
            attack_inc: 1.0 / (0.003 * sr),
            release_coef: decay_coef(release, sr),
        }
    }
    pub fn trigger(&mut self) {
        self.attacking = true;
    }
    /// 0 = no ducking, 1 = fully ducked.
    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.attacking {
            self.env += self.attack_inc;
            if self.env >= 1.0 {
                self.env = 1.0;
                self.attacking = false;
            }
        } else {
            self.env *= self.release_coef;
        }
        self.env
    }
}

// ----------------------------------------------------------------------------- effects

/// Tempo-synced stereo ping-pong delay with a darkening feedback path.
pub struct PingPongDelay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    pos: usize,
    delay: usize,
    pub feedback: f32,
    damp_l: OnePole,
    damp_r: OnePole,
    hp: OnePole,
}

impl PingPongDelay {
    pub fn new(max_seconds: f32, delay_seconds: f32, feedback: f32, sr: f32) -> PingPongDelay {
        let n = (max_seconds * sr) as usize + 1;
        PingPongDelay {
            buf_l: vec![0.0; n],
            buf_r: vec![0.0; n],
            pos: 0,
            delay: ((delay_seconds * sr) as usize).clamp(1, n - 1),
            feedback,
            damp_l: OnePole::new(3500.0, sr),
            damp_r: OnePole::new(3500.0, sr),
            hp: OnePole::new(250.0, sr),
        }
    }

    #[inline]
    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let n = self.buf_l.len();
        let rd = (self.pos + n - self.delay) % n;
        let out_l = self.buf_l[rd];
        let out_r = self.buf_r[rd];
        let x = self.hp.hp(input);
        // Input enters left, bounces right, back left...
        self.buf_l[self.pos] = x + self.damp_r.lp(out_r) * self.feedback;
        self.buf_r[self.pos] = self.damp_l.lp(out_l) * self.feedback;
        self.pos = (self.pos + 1) % n;
        (out_l, out_r)
    }
}

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Comb {
        Comb {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }
    #[inline]
    fn process(&mut self, x: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * feedback;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Allpass {
        Allpass {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }
    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        let b = self.buf[self.pos];
        let out = b - x;
        self.buf[self.pos] = x + b * 0.5;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
        out
    }
}

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
const STEREO_SPREAD: usize = 23;

/// Freeverb-style reverb. `channels` = 1 for a mono tail (rumble), 2 for stereo.
pub struct Reverb {
    combs: Vec<[Comb; 8]>,
    allpasses: Vec<[Allpass; 4]>,
    pub room: f32,
    pub damp: f32,
    pre_hp: OnePole,
}

impl Reverb {
    pub fn new(channels: usize, room: f32, damp: f32, pre_hp_hz: f32, sr: f32) -> Reverb {
        let scale = sr / 44100.0;
        let mut combs = Vec::new();
        let mut allpasses = Vec::new();
        for ch in 0..channels {
            let spread = ch * STEREO_SPREAD;
            combs.push(COMB_TUNING.map(|t| Comb::new(((t + spread) as f32 * scale) as usize)));
            allpasses
                .push(ALLPASS_TUNING.map(|t| Allpass::new(((t + spread) as f32 * scale) as usize)));
        }
        Reverb {
            combs,
            allpasses,
            room,
            damp,
            pre_hp: OnePole::new(pre_hp_hz, sr),
        }
    }

    /// Mono in, (left, right) out. With one channel both outputs are equal.
    #[inline]
    pub fn process(&mut self, input: f32) -> (f32, f32) {
        let x = self.pre_hp.hp(input) * 0.015;
        let fb = self.room * 0.28 + 0.7;
        let mut out = [0.0f32; 2];
        for (ch, (combs, aps)) in self
            .combs
            .iter_mut()
            .zip(self.allpasses.iter_mut())
            .enumerate()
        {
            let mut s = 0.0;
            for c in combs.iter_mut() {
                s += c.process(x, fb, self.damp);
            }
            for a in aps.iter_mut() {
                s = a.process(s);
            }
            out[ch] = s;
        }
        if self.combs.len() == 1 {
            (out[0], out[0])
        } else {
            (out[0], out[1])
        }
    }
}

/// Stereo-linked feed-forward compressor.
pub struct Compressor {
    threshold_db: f32,
    ratio: f32,
    makeup: f32,
    att: f32,
    rel: f32,
    env_db: f32,
}

impl Compressor {
    pub fn new(
        threshold_db: f32,
        ratio: f32,
        attack: f32,
        release: f32,
        makeup_db: f32,
        sr: f32,
    ) -> Self {
        Compressor {
            threshold_db,
            ratio,
            makeup: db_to_gain(makeup_db),
            att: smooth_coef(attack, sr),
            rel: smooth_coef(release, sr),
            env_db: 0.0,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let level_db = gain_to_db(l.abs().max(r.abs()));
        let over = (level_db - self.threshold_db).max(0.0);
        let target = over * (1.0 - 1.0 / self.ratio);
        let c = if target > self.env_db {
            self.att
        } else {
            self.rel
        };
        self.env_db += (target - self.env_db) * c;
        let g = db_to_gain(-self.env_db) * self.makeup;
        (l * g, r * g)
    }

    pub fn reduction_db(&self) -> f32 {
        self.env_db
    }
}

/// Look-ahead brickwall limiter. Output never exceeds `ceiling`.
pub struct Limiter {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    pos: usize,
    ceiling: f32,
    gain: f32,
    held: f32,
    hold_left: usize,
    att: f32,
    rel: f32,
}

impl Limiter {
    pub fn new(ceiling_db: f32, lookahead: f32, release: f32, sr: f32) -> Limiter {
        let n = ((lookahead * sr) as usize).max(1);
        Limiter {
            buf_l: vec![0.0; n],
            buf_r: vec![0.0; n],
            pos: 0,
            ceiling: db_to_gain(ceiling_db),
            gain: 1.0,
            held: 1.0,
            hold_left: 0,
            att: 1.0 - (-4.6 / n as f32).exp(),
            rel: smooth_coef(release, sr),
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let peak = l.abs().max(r.abs());
        let target = if peak > self.ceiling {
            self.ceiling / peak
        } else {
            1.0
        };
        if target <= self.held {
            self.held = target;
            self.hold_left = self.buf_l.len();
        } else if self.hold_left > 0 {
            self.hold_left -= 1;
        } else {
            self.held = target;
        }
        let c = if self.held < self.gain {
            self.att
        } else {
            self.rel
        };
        self.gain += (self.held - self.gain) * c;

        let dl = self.buf_l[self.pos];
        let dr = self.buf_r[self.pos];
        self.buf_l[self.pos] = l;
        self.buf_r[self.pos] = r;
        self.pos = (self.pos + 1) % self.buf_l.len();

        let c = self.ceiling;
        ((dl * self.gain).clamp(-c, c), (dr * self.gain).clamp(-c, c))
    }

    pub fn gain_db(&self) -> f32 {
        gain_to_db(self.gain)
    }
}

/// Equal-power pan, `pan` in -1..1.
#[inline]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * 0.25 * PI;
    (a.cos(), a.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_respects_ceiling() {
        let sr = 48000.0;
        let mut lim = Limiter::new(-1.0, 0.003, 0.08, sr);
        let ceil = db_to_gain(-1.0);
        let mut n = Noise::new(5);
        for i in 0..48000 {
            let x = n.next() * if i % 4000 < 200 { 6.0 } else { 0.5 };
            let (l, r) = lim.process(x, -x);
            assert!(l.abs() <= ceil + 1e-6 && r.abs() <= ceil + 1e-6);
        }
    }

    #[test]
    fn saw_is_bounded() {
        let mut o = Osc::default();
        for _ in 0..10000 {
            let v = o.saw(440.0, 48000.0);
            assert!(v.abs() <= 1.1);
        }
    }

    #[test]
    fn svf_lowpass_attenuates_highs() {
        let sr = 48000.0;
        let mut f = Svf::new(200.0, 0.0, sr);
        let mut o = Osc::default();
        let mut peak: f32 = 0.0;
        for i in 0..48000 {
            let y = f.lp(o.sine(8000.0, sr));
            if i > 1000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.01, "{peak}");
    }
}
