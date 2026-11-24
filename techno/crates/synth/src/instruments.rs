//! Synthesized instruments. All sounds are generated in real time; no samples.
//! Every instrument is allocation-free after construction.

use crate::dsp::*;
use techno_core::dna::{
    BassParams, DroneParams, HatParams, KickParams, LeadParams, PadParams, RumbleParams,
    SnareParams, StabParams, ToneParams,
};
use techno_core::profile::{KickStyle, LeadKind};

// ----------------------------------------------------------------------------- kick

/// Hard techno kick: pitch-swept sine, click transient, heavy saturation.
pub struct Kick {
    sr: f32,
    p: KickParams,
    osc: Osc,
    t: f32,
    active: bool,
    vel: f32,
    noise: Noise,
    click_hp: Svf,
    tone_lp: Svf,
}

impl Kick {
    pub fn new(mut p: KickParams, sr: f32) -> Kick {
        // Styles reshape the sampled parameters instead of needing their own ranges.
        let tone = match p.style {
            KickStyle::Hard => 7000.0,
            KickStyle::Punch => {
                p.decay *= 0.75;
                p.pitch_decay *= 0.7;
                p.drive = 1.0 + p.drive * 0.25;
                p.click *= 1.8;
                9000.0
            }
            KickStyle::Distorted => {
                p.decay *= 1.15;
                p.drive *= 2.2;
                4200.0
            }
        };
        Kick {
            sr,
            p,
            osc: Osc::default(),
            t: 0.0,
            active: false,
            vel: 1.0,
            noise: Noise::new(0x4B1C),
            click_hp: Svf::new(2500.0, 0.2, sr),
            tone_lp: Svf::new(tone, 0.0, sr),
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.active = true;
        self.vel = vel;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let p = &self.p;
        let t = self.t;
        let f = p.tune_hz + (p.pitch_start_hz - p.tune_hz) * (-t / p.pitch_decay).exp();
        let body = self.osc.sine(f, self.sr);
        // Short hold keeps the punch, then an exponential tail.
        let amp = if t < 0.012 {
            1.0
        } else {
            (-(t - 0.012) * 4.6 / p.decay).exp()
        };
        let click_env = (-t / 0.0018).exp();
        let click = self.click_hp.hp(self.noise.next()) * click_env * p.click;
        let x = body * amp + click;
        let shaped = match p.style {
            // Hard clip after the soft stage: square-ish, aggressive.
            KickStyle::Distorted => (saturate(x, p.drive) * 1.4).clamp(-1.0, 1.0) * 0.78,
            _ => saturate(x, p.drive),
        };
        let y = self.tone_lp.lp(shaped);
        self.t += 1.0 / self.sr;
        if t > p.decay + 0.12 {
            self.active = false;
        }
        y * self.vel
    }
}

// ----------------------------------------------------------------------------- rumble

/// Rumble: the kick through a long dark reverb, low-passed and saturated. The deep
/// sidechain on its mixer channel makes it pump between the kicks.
pub struct Rumble {
    sr: f32,
    reverb: Reverb,
    hp: Svf,
    lp1: Svf,
    lp2: Svf,
    drive: f32,
}

impl Rumble {
    pub fn new(p: RumbleParams, sr: f32) -> Rumble {
        Rumble {
            sr,
            reverb: Reverb::new(1, p.size, 0.6, 30.0, sr),
            hp: Svf::new(35.0, 0.1, sr),
            lp1: Svf::new(p.lowpass_hz, 0.3, sr),
            lp2: Svf::new(p.lowpass_hz, 0.0, sr),
            drive: p.drive,
        }
    }

    /// Re-tune for a new song, keeping (and clearing) the reverb buffers.
    pub fn reset(&mut self, p: RumbleParams) {
        self.reverb.clear();
        self.reverb.room = p.size;
        self.hp.reset();
        self.lp1 = Svf::new(p.lowpass_hz, 0.3, self.sr);
        self.lp2 = Svf::new(p.lowpass_hz, 0.0, self.sr);
        self.drive = p.drive;
    }

    #[inline]
    pub fn process(&mut self, kick: f32) -> f32 {
        let (wet, _) = self.reverb.process(kick);
        let x = self.lp2.lp(self.lp1.lp(self.hp.hp(wet * 6.0)));
        saturate(x, self.drive)
    }
}

// ----------------------------------------------------------------------------- hats

/// Closed and open hi-hat sharing one metallic source. A closed hit chokes the open hat.
pub struct Hats {
    sr: f32,
    metal: MetalBank,
    noise: Noise,
    hp: Svf,
    peak: Svf,
    closed: Decay,
    open: Decay,
    mix: f32,
    scale: f32,
}

impl Hats {
    pub fn new(p: HatParams, sr: f32) -> Hats {
        let mut closed = Decay::default();
        closed.set_time(p.closed_decay, sr);
        let mut open = Decay::default();
        open.set_time(p.open_decay, sr);
        Hats {
            sr,
            metal: MetalBank::new(),
            noise: Noise::new(0x1A7),
            hp: Svf::new(p.tone_hz, 0.15, sr),
            peak: Svf::new(p.tone_hz * 1.25, 0.4, sr),
            closed,
            open,
            mix: p.metal,
            scale: p.tone_hz / 8000.0 * 1.6,
        }
    }

    pub fn trigger_closed(&mut self, vel: f32) {
        self.closed.trigger(vel);
        self.open.kill();
    }

    pub fn trigger_open(&mut self, vel: f32) {
        self.open.trigger(vel);
    }

    /// (closed, open)
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        if !self.closed.is_active() && !self.open.is_active() {
            return (0.0, 0.0);
        }
        let src =
            self.metal.next(self.scale, self.sr) * self.mix + self.noise.next() * (1.0 - self.mix);
        let h = self.hp.hp(src);
        let s = h + self.peak.bp(h) * 0.6;
        (s * 4.0 * self.closed.next(), s * 4.0 * self.open.next())
    }
}

/// Ride or crash cymbal.
pub struct Cymbal {
    sr: f32,
    metal: MetalBank,
    noise: Noise,
    bp: Svf,
    hp: Svf,
    env: Decay,
    scale: f32,
    noise_mix: f32,
}

impl Cymbal {
    pub fn ride(p: ToneParams, sr: f32) -> Cymbal {
        Cymbal::new(p.tone_hz, p.decay, 0.25, 0x51DE, sr)
    }

    pub fn crash(sr: f32) -> Cymbal {
        Cymbal::new(5000.0, 1.8, 0.7, 0xC0A5, sr)
    }

    fn new(tone: f32, decay: f32, noise_mix: f32, seed: u32, sr: f32) -> Cymbal {
        let mut env = Decay::default();
        env.set_time(decay, sr);
        Cymbal {
            sr,
            metal: MetalBank::new(),
            noise: Noise::new(seed),
            bp: Svf::new(tone, 0.35, sr),
            hp: Svf::new(tone * 0.6, 0.1, sr),
            env,
            scale: tone / 5000.0 * 2.3,
            noise_mix,
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.env.trigger(vel);
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.env.is_active() {
            return 0.0;
        }
        let src = self.metal.next(self.scale, self.sr) * (1.0 - self.noise_mix)
            + self.noise.next() * self.noise_mix;
        let x = self.hp.hp(src);
        (x * 0.6 + self.bp.bp(x)) * 2.5 * self.env.next()
    }
}

// ----------------------------------------------------------------------------- clap / snare / perc

/// Clap: band-passed noise with three quick bursts and a tail.
pub struct Clap {
    sr: f32,
    noise: Noise,
    bp: Svf,
    hp: Svf,
    t: f32,
    active: bool,
    vel: f32,
    decay: f32,
}

impl Clap {
    pub fn new(p: ToneParams, sr: f32) -> Clap {
        Clap {
            sr,
            noise: Noise::new(0xC1A9),
            bp: Svf::new(p.tone_hz, 0.45, sr),
            hp: Svf::new(600.0, 0.0, sr),
            t: 0.0,
            active: false,
            vel: 1.0,
            decay: p.decay,
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.t = 0.0;
        self.active = true;
        self.vel = vel;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        let env = if t < 0.03 {
            let k = (t / 0.01).floor();
            (-(t - k * 0.01) / 0.0025).exp()
        } else {
            0.85 * (-(t - 0.03) * 4.6 / self.decay).exp()
        };
        let n = self.noise.next();
        let x = self.bp.bp(n) * 1.6 + self.hp.hp(n) * 0.25;
        self.t += 1.0 / self.sr;
        if t > self.decay + 0.05 {
            self.active = false;
        }
        x * env * self.vel
    }
}

/// Snare for fills and rolls: pitched body plus noise.
pub struct Snare {
    sr: f32,
    p: SnareParams,
    osc: Osc,
    noise: Noise,
    hp: Svf,
    t: f32,
    active: bool,
    vel: f32,
}

impl Snare {
    pub fn new(p: SnareParams, sr: f32) -> Snare {
        Snare {
            sr,
            p,
            osc: Osc::default(),
            noise: Noise::new(0x5AA3),
            hp: Svf::new(1800.0, 0.1, sr),
            t: 0.0,
            active: false,
            vel: 1.0,
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.active = true;
        self.vel = vel;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        let p = &self.p;
        let f = p.tone_hz * (1.0 + 0.6 * (-t / 0.015).exp());
        let body = self.osc.sine(f, self.sr) * (-t * 4.6 / (p.decay * 0.6)).exp();
        let noise = self.hp.hp(self.noise.next()) * (-t * 4.6 / p.decay).exp();
        self.t += 1.0 / self.sr;
        if t > p.decay + 0.05 {
            self.active = false;
        }
        saturate(body * (1.0 - p.snappy * 0.5) + noise * p.snappy, 1.5) * self.vel
    }
}

/// Rim shot: short resonant click.
pub struct Rim {
    sr: f32,
    osc: Osc,
    noise: Noise,
    bp: Svf,
    env: Decay,
    tone: f32,
}

impl Rim {
    pub fn new(p: ToneParams, sr: f32) -> Rim {
        let mut env = Decay::default();
        env.set_time(p.decay, sr);
        Rim {
            sr,
            osc: Osc::default(),
            noise: Noise::new(0x219),
            bp: Svf::new(p.tone_hz, 0.8, sr),
            env,
            tone: p.tone_hz,
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.env.trigger(vel);
        self.osc.phase = 0.0;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.env.is_active() {
            return 0.0;
        }
        let x = self.bp.bp(self.noise.next()) * 2.0 + self.osc.sine(self.tone * 0.5, self.sr) * 0.5;
        x * self.env.next()
    }
}

/// Tuned tom with a pitch drop.
pub struct Tom {
    sr: f32,
    osc: Osc,
    t: f32,
    active: bool,
    vel: f32,
    freq: f32,
    decay: f32,
}

impl Tom {
    pub fn new(p: ToneParams, sr: f32) -> Tom {
        Tom {
            sr,
            osc: Osc::default(),
            t: 0.0,
            active: false,
            vel: 1.0,
            freq: p.tone_hz,
            decay: p.decay,
        }
    }

    pub fn trigger(&mut self, freq: f32, vel: f32) {
        self.freq = freq;
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.active = true;
        self.vel = vel;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        let f = self.freq * (1.0 + 0.8 * (-t / 0.03).exp());
        let amp = (-t * 4.6 / self.decay).exp();
        self.t += 1.0 / self.sr;
        if t > self.decay + 0.05 {
            self.active = false;
        }
        saturate(self.osc.sine(f, self.sr) * amp, 2.0) * self.vel
    }
}

// ----------------------------------------------------------------------------- bass

/// Rolling bass: saw + sine sub through an enveloped low-pass, then saturation.
pub struct Bass {
    sr: f32,
    p: BassParams,
    saw: Osc,
    sub: Osc,
    filt: Svf,
    fenv: Decay,
    amp: Adsr,
    freq: f32,
    gate: u32,
    vel: f32,
    accent: f32,
    /// External filter amount 0..1 (arrangement automation; 1 = as designed).
    pub filter_mul: f32,
}

impl Bass {
    pub fn new(p: BassParams, sr: f32) -> Bass {
        let mut fenv = Decay::default();
        fenv.set_time(p.decay, sr);
        Bass {
            sr,
            p,
            saw: Osc::default(),
            sub: Osc::default(),
            filt: Svf::new(p.cutoff_hz, p.resonance, sr),
            fenv,
            amp: Adsr::new(0.0015, p.decay * 1.5, 0.55, 0.012, sr),
            freq: 55.0,
            gate: 0,
            vel: 1.0,
            accent: 0.0,
            filter_mul: 1.0,
        }
    }

    pub fn trigger(&mut self, freq: f32, vel: f32, accent: bool, gate_samples: u32) {
        self.freq = freq;
        self.vel = vel;
        self.accent = if accent { 1.0 } else { 0.0 };
        self.gate = gate_samples.max(1);
        self.fenv.trigger(1.0);
        self.amp.gate_on();
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.amp.is_active() {
            return 0.0;
        }
        if self.gate > 0 {
            self.gate -= 1;
            if self.gate == 0 {
                self.amp.gate_off();
            }
        }
        let p = &self.p;
        let x = self.saw.saw(self.freq, self.sr) * (1.0 - p.sub_mix * 0.5)
            + self.sub.sine(self.freq, self.sr) * p.sub_mix;
        let fe = self.fenv.next();
        let cutoff = p.cutoff_hz
            * (0.3 + 0.7 * self.filter_mul)
            * (1.0 + p.env_mod * fe * (1.0 + 0.5 * self.accent) * (0.4 + 0.6 * self.filter_mul));
        self.filt.set(cutoff, p.resonance, self.sr);
        let y = self.filt.lp(x);
        saturate(y * self.amp.next(), p.drive) * self.vel
    }
}

// ----------------------------------------------------------------------------- lead

/// Monophonic acid-style lead: saw/square, two resonant low-pass stages, slide, accent.
pub struct Lead {
    sr: f32,
    p: LeadParams,
    osc: Osc,
    f1: Svf,
    f2: Svf,
    fenv: Decay,
    amp: Adsr,
    freq: f32,
    target: f32,
    glide: f32,
    gate: u32,
    vel: f32,
    accent: f32,
    /// External filter opening 0..1 (arrangement automation).
    pub filter_open: f32,
    stack: [Osc; 7],
    modulator: Osc,
    dive: f32,
}

impl Lead {
    pub fn new(p: LeadParams, sr: f32) -> Lead {
        let mut fenv = Decay::default();
        fenv.set_time(p.decay, sr);
        Lead {
            sr,
            p,
            osc: Osc::default(),
            f1: Svf::new(p.cutoff_hz, 0.0, sr),
            f2: Svf::new(p.cutoff_hz, p.resonance, sr),
            fenv,
            amp: Adsr::new(0.002, 0.2, 0.7, 0.02, sr),
            freq: 220.0,
            target: 220.0,
            glide: smooth_coef(0.045, sr),
            gate: 0,
            vel: 1.0,
            accent: 0.0,
            filter_open: 1.0,
            stack: core::array::from_fn(|i| Osc::with_phase(i as f32 * 0.137 % 1.0)),
            modulator: Osc::default(),
            dive: 0.0,
        }
    }

    /// `legato`: the previous note slides into this one (no envelope retrigger).
    pub fn trigger(&mut self, freq: f32, vel: f32, accent: bool, gate_samples: u32, legato: bool) {
        self.target = freq;
        self.gate = gate_samples.max(1);
        self.vel = vel;
        self.accent = if accent { 1.0 } else { 0.0 };
        if !(legato && self.amp.is_active()) {
            self.freq = freq;
            self.fenv.trigger(1.0);
            self.amp.gate_on();
            self.dive = 1.0;
            if self.p.kind == LeadKind::Pluck {
                self.modulator.phase = 0.0;
            }
        }
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.amp.is_active() {
            return 0.0;
        }
        if self.gate > 0 {
            self.gate -= 1;
            if self.gate == 0 {
                self.amp.gate_off();
            }
        }
        let p = &self.p;
        self.freq += (self.target - self.freq) * self.glide;
        let fe = self.fenv.next();
        let sr = self.sr;
        let open = 0.3 + 1.5 * self.filter_open;
        let y = match p.kind {
            LeadKind::Acid => {
                let saw = self.osc.saw(self.freq, sr);
                let sq = if saw > 0.0 { 1.0 } else { -1.0 };
                let x = saw * (1.0 - p.square) + sq * p.square * 0.7;
                let cutoff =
                    p.cutoff_hz * open * (1.0 + p.env_mod * fe * (1.0 + 0.8 * self.accent));
                self.f1.set(cutoff, 0.0, sr);
                self.f2.set(cutoff, p.resonance, sr);
                self.f2.lp(self.f1.lp(x))
            }
            LeadKind::Hoover => {
                // Pitch dives into the note from a fourth above.
                self.dive *= 0.9985;
                let f = self.freq * (1.0 + 0.33 * self.dive * self.dive);
                const DET: [f32; 4] = [1.0, 1.008, 0.992, 0.5];
                let mut x = 0.0;
                for (o, d) in self.stack.iter_mut().zip(DET) {
                    x += o.saw(f * d, sr);
                }
                let cutoff = p.cutoff_hz * 3.0 * open * (1.0 + 0.5 * fe);
                self.f1.set(cutoff, 0.15, sr);
                self.f1.lp(x * 0.4)
            }
            LeadKind::Supersaw => {
                const DET: [f32; 7] = [1.0, 1.011, 0.989, 1.022, 0.978, 1.034, 0.966];
                let mut x = 0.0;
                for (o, d) in self.stack.iter_mut().zip(DET) {
                    x += o.saw(self.freq * d, sr);
                }
                let cutoff = p.cutoff_hz * 4.0 * open * (1.0 + 0.4 * fe);
                self.f1.set(cutoff, 0.1, sr);
                self.f1.lp(x * 0.3)
            }
            LeadKind::Pluck => {
                // 2-op FM: modulation index decays with the filter envelope -> bell pluck.
                let m = self.modulator.sine(self.freq * 2.0, sr);
                let index = 0.25 + 2.5 * fe * (0.4 + 0.6 * self.filter_open);
                self.osc.phase = (self.osc.phase + m * index * 0.02).rem_euclid(1.0);
                self.osc.sine(self.freq, sr) * (0.4 + 0.6 * fe)
            }
        };
        saturate(y * self.amp.next() * (1.0 + 0.3 * self.accent), p.drive) * self.vel
    }
}

// ----------------------------------------------------------------------------- stab

const STAB_VOICES: usize = 8;

struct StabVoice {
    a: Osc,
    b: Osc,
    freq: f32,
    filt: Svf,
    fenv: Decay,
    amp: Decay,
    vel: f32,
}

/// Chord stab: detuned saws, plucky filter envelope. Meant to be fed into delay/reverb.
pub struct Stab {
    sr: f32,
    p: StabParams,
    voices: [StabVoice; STAB_VOICES],
    next: usize,
    detune: f32,
}

impl Stab {
    pub fn new(p: StabParams, sr: f32) -> Stab {
        let voices = core::array::from_fn(|i| {
            let mut fenv = Decay::default();
            fenv.set_time(p.decay * 0.6, sr);
            let mut amp = Decay::default();
            amp.set_time(p.decay, sr);
            StabVoice {
                a: Osc::with_phase(i as f32 * 0.31 % 1.0),
                b: Osc::with_phase(i as f32 * 0.57 % 1.0),
                freq: 220.0,
                filt: Svf::new(p.cutoff_hz, 0.3, sr),
                fenv,
                amp,
                vel: 0.0,
            }
        });
        Stab {
            sr,
            p,
            voices,
            next: 0,
            detune: 2f32.powf(p.detune / 12.0),
        }
    }

    pub fn trigger_chord(&mut self, freqs: &[f32], vel: f32) {
        for &f in freqs {
            let v = &mut self.voices[self.next];
            self.next = (self.next + 1) % STAB_VOICES;
            v.freq = f;
            v.vel = vel;
            v.fenv.trigger(1.0);
            v.amp.trigger(1.0);
        }
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        let mut out = 0.0;
        let sr = self.sr;
        for v in self.voices.iter_mut() {
            if !v.amp.is_active() {
                continue;
            }
            let x = v.a.saw(v.freq * self.detune, sr) + v.b.saw(v.freq / self.detune, sr);
            let cutoff = self.p.cutoff_hz * (1.0 + self.p.env_mod * v.fenv.next());
            v.filt.set(cutoff, 0.3, sr);
            out += v.filt.lp(x) * v.amp.next() * v.vel;
        }
        out * 0.35
    }
}

// ----------------------------------------------------------------------------- pad

const PAD_VOICES: usize = 8;

struct PadVoice {
    a: Osc,
    b: Osc,
    freq: f32,
    filt_l: Svf,
    filt_r: Svf,
    env: Adsr,
}

/// Atmospheric pad: slow attack, two detuned saws per note spread left/right.
pub struct Pad {
    sr: f32,
    p: PadParams,
    voices: [PadVoice; PAD_VOICES],
    next: usize,
    detune: f32,
    lfo: Osc,
}

impl Pad {
    pub fn new(p: PadParams, sr: f32) -> Pad {
        let voices = core::array::from_fn(|i| PadVoice {
            a: Osc::with_phase(i as f32 * 0.23 % 1.0),
            b: Osc::with_phase(i as f32 * 0.71 % 1.0),
            freq: 220.0,
            filt_l: Svf::new(p.cutoff_hz, 0.1, sr),
            filt_r: Svf::new(p.cutoff_hz, 0.1, sr),
            env: Adsr::new(p.attack, 1.0, 0.9, 1.8, sr),
        });
        Pad {
            sr,
            p,
            voices,
            next: 0,
            detune: 2f32.powf(p.detune / 12.0),
            lfo: Osc::default(),
        }
    }

    /// Release the current chord and start a new one.
    pub fn set_chord(&mut self, freqs: &[f32]) {
        self.release_all();
        for &f in freqs {
            let v = &mut self.voices[self.next];
            self.next = (self.next + 1) % PAD_VOICES;
            v.freq = f;
            v.env.gate_on();
        }
    }

    pub fn release_all(&mut self) {
        for v in self.voices.iter_mut() {
            v.env.gate_off();
        }
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let sr = self.sr;
        let lfo = self.lfo.sine(0.07, sr);
        let cutoff = self.p.cutoff_hz * (1.0 + 0.35 * lfo);
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if !v.env.is_active() {
                continue;
            }
            let e = v.env.next();
            v.filt_l.set(cutoff, 0.1, sr);
            v.filt_r.set(cutoff, 0.1, sr);
            l += v.filt_l.lp(v.a.saw(v.freq * self.detune, sr)) * e;
            r += v.filt_r.lp(v.b.saw(v.freq / self.detune, sr)) * e;
        }
        (l * 0.22, r * 0.22)
    }
}

// ----------------------------------------------------------------------------- fx

/// Build-up riser: a noise sweep plus an optional detuned saw stack climbing an octave.
/// Level and progress come from the arrangement automation (no internal timing).
pub struct Riser {
    sr: f32,
    noise: Noise,
    bp: Svf,
    hp: Svf,
    saws: [Osc; 5],
    saw_lp: Svf,
    base_hz: f32,
    pitched: f32,
}

impl Riser {
    pub fn new(base_hz: f32, pitched: f32, sr: f32) -> Riser {
        Riser {
            sr,
            noise: Noise::new(0x7153),
            bp: Svf::new(400.0, 0.5, sr),
            hp: Svf::new(200.0, 0.0, sr),
            saws: core::array::from_fn(|i| Osc::with_phase(i as f32 * 0.21)),
            saw_lp: Svf::new(800.0, 0.2, sr),
            base_hz,
            pitched,
        }
    }

    /// `level` 0..1, `progress` 0..1 through the build.
    #[inline]
    pub fn process(&mut self, level: f32, progress: f32) -> f32 {
        if level <= 1e-4 {
            return 0.0;
        }
        let cutoff = 300.0 * (progress * 5.0 * core::f32::consts::LN_2).exp(); // 300 Hz .. 9.6 kHz
        self.bp.set(cutoff, 0.55, self.sr);
        let n = self.noise.next();
        let noise = self.bp.bp(n) * 1.5 + self.hp.hp(n) * 0.15 * progress;
        let mut saw = 0.0;
        if self.pitched > 0.0 {
            let f = self.base_hz * (1.0 + progress); // one octave up over the build
            const DETUNE: [f32; 5] = [1.0, 1.006, 0.994, 1.013, 0.987];
            for (o, d) in self.saws.iter_mut().zip(DETUNE) {
                saw += o.saw(f * d, self.sr);
            }
            self.saw_lp
                .set(600.0 + 6000.0 * progress * progress, 0.2, self.sr);
            saw = self.saw_lp.lp(saw) * 0.25 * self.pitched;
        }
        (noise + saw) * level
    }
}

/// Impact on a drop / breakdown downbeat: sub boom, noise crack and a falling
/// noise downlifter over about two bars.
pub struct Impact {
    sr: f32,
    osc: Osc,
    noise: Noise,
    down_bp: Svf,
    crack_hp: Svf,
    t: f32,
    active: bool,
    down_len: f32,
}

impl Impact {
    pub fn new(bar_seconds: f32, sr: f32) -> Impact {
        Impact {
            sr,
            osc: Osc::default(),
            noise: Noise::new(0x1A9C7),
            down_bp: Svf::new(6000.0, 0.4, sr),
            crack_hp: Svf::new(1500.0, 0.0, sr),
            t: 0.0,
            active: false,
            down_len: bar_seconds * 2.0,
        }
    }

    pub fn trigger(&mut self) {
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.active = true;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let t = self.t;
        let boom_f = 35.0 + 60.0 * (-t / 0.08).exp();
        let boom = self.osc.sine(boom_f, self.sr) * (-t * 4.6 / 1.6).exp();
        let n = self.noise.next();
        let crack = self.crack_hp.hp(n) * (-t / 0.05).exp();
        let x = (t / self.down_len).min(1.0);
        let down_f = 7000.0 * (-x * 3.5).exp() + 150.0;
        self.down_bp.set(down_f, 0.4, self.sr);
        let down = self.down_bp.bp(n) * (1.0 - x) * (1.0 - x) * 0.8;
        self.t += 1.0 / self.sr;
        if t > self.down_len.max(1.6) + 0.1 {
            self.active = false;
        }
        saturate(boom * 0.9, 1.5) + crack * 0.5 + down
    }
}

// ----------------------------------------------------------------------------- drone

/// Long, evolving drone: detuned saws on root and fifth plus a sub octave, slowly
/// breathing low-pass, saturation. Follows the progression root with a glide.
pub struct Drone {
    sr: f32,
    p: DroneParams,
    osc: [Osc; 5],
    lfo: Osc,
    filt: Svf,
    env: Adsr,
    freq: f32,
    target: f32,
    glide: f32,
}

impl Drone {
    pub fn new(p: DroneParams, sr: f32) -> Drone {
        Drone {
            sr,
            p,
            osc: core::array::from_fn(|i| Osc::with_phase(i as f32 * 0.19 % 1.0)),
            lfo: Osc::default(),
            filt: Svf::new(p.cutoff_hz, 0.3, sr),
            env: Adsr::new(2.5, 1.0, 1.0, 3.0, sr),
            freq: 110.0,
            target: 110.0,
            glide: smooth_coef(0.4, sr),
        }
    }

    pub fn play(&mut self, freq: f32) {
        if !self.env.is_active() {
            self.freq = freq;
        }
        self.target = freq;
        self.env.gate_on();
    }

    pub fn release(&mut self) {
        self.env.gate_off();
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.env.is_active() {
            return 0.0;
        }
        let sr = self.sr;
        self.freq += (self.target - self.freq) * self.glide;
        let d = 2f32.powf(self.p.detune / 12.0);
        let f = self.freq;
        let x = self.osc[0].saw(f * d, sr)
            + self.osc[1].saw(f / d, sr)
            + 0.7 * self.osc[2].saw(f * 1.5 * d, sr)
            + 0.7 * self.osc[3].saw(f * 1.5 / d, sr)
            + 0.8 * self.osc[4].sine(f * 0.5, sr);
        let lfo = self.lfo.sine(0.045, sr);
        self.filt.set(self.p.cutoff_hz * (1.0 + 0.6 * lfo), 0.3, sr);
        saturate(self.filt.lp(x * 0.3) * self.env.next(), self.p.drive) * 0.6
    }
}
