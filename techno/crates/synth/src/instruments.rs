//! Synthesized instruments. All sounds are generated in real time; no samples.
//! Every instrument is allocation-free after construction.

use crate::dsp::*;
use techno_core::dna::{
    BassParams, DroneParams, GuitarParams, HatParams, KeysParams, KickParams, LeadParams,
    PadParams, RumbleParams, SnareParams, StabParams, ToneParams,
};
use techno_core::profile::{KickStyle, LeadKind};
use techno_core::structure::RiserKind;

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
    /// External drive multiplier (harder kick in drops).
    pub drive_mul: f32,
    pitch_env: f32,
    pitch_coef: f32,
    amp_env: f32,
    amp_coef: f32,
    click_env: f32,
    click_coef: f32,
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
            KickStyle::Deep => {
                // Long, round and soft: more body, less click and drive.
                p.decay *= 1.4;
                p.pitch_decay *= 1.3;
                p.drive *= 0.6;
                p.click *= 0.5;
                5000.0
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
            drive_mul: 1.0,
            pitch_env: 0.0,
            pitch_coef: (-1.0 / (p.pitch_decay * sr)).exp(),
            amp_env: 0.0,
            amp_coef: (-4.6 / (p.decay * sr)).exp(),
            click_env: 0.0,
            click_coef: (-1.0 / (0.0018 * sr)).exp(),
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.active = true;
        self.vel = vel;
        self.pitch_env = 1.0;
        self.amp_env = 1.0;
        self.click_env = 1.0;
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if !self.active {
            return 0.0;
        }
        let p = &self.p;
        let t = self.t;
        // Envelopes are recursive multiplications (same curves as exp(), far cheaper).
        let f = p.tune_hz + (p.pitch_start_hz - p.tune_hz) * self.pitch_env;
        self.pitch_env *= self.pitch_coef;
        let body = self.osc.sine(f, self.sr);
        // Short hold keeps the punch, then an exponential tail.
        let amp = if t < 0.012 {
            1.0
        } else {
            let a = self.amp_env;
            self.amp_env *= self.amp_coef;
            a
        };
        let click_env = self.click_env;
        // Flush tiny values: they would turn into slow denormal floats.
        self.click_env = if click_env < 1e-6 {
            0.0
        } else {
            click_env * self.click_coef
        };
        let click = self.click_hp.hp(self.noise.next()) * click_env * p.click;
        let x = body * amp + click;
        let shaped = match p.style {
            // Hard clip after the soft stage: square-ish, aggressive.
            KickStyle::Distorted => {
                (saturate(x, p.drive * self.drive_mul) * 1.4).clamp(-1.0, 1.0) * 0.78
            }
            _ => saturate(x, p.drive * self.drive_mul),
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

/// Rumble variants: 0 standard, 1 tight (short room, brighter), 2 deep wash (long, dark, dirty).
pub struct Rumble {
    sr: f32,
    reverb: Reverb,
    hp: Svf,
    lp1: Svf,
    lp2: Svf,
    drive: f32,
    gain: f32,
}

fn rumble_shape(p: &RumbleParams) -> (f32, f32, f32, f32) {
    // (room, low-pass, drive, gain)
    match p.variant {
        1 => (
            (p.size * 0.8).min(0.95),
            p.lowpass_hz * 1.25,
            p.drive * 0.8,
            1.15,
        ),
        2 => (
            (p.size * 1.08).min(0.97),
            p.lowpass_hz * 0.8,
            p.drive * 1.4,
            0.9,
        ),
        _ => (p.size, p.lowpass_hz, p.drive, 1.0),
    }
}

impl Rumble {
    pub fn new(p: RumbleParams, sr: f32) -> Rumble {
        let (room, lp, drive, gain) = rumble_shape(&p);
        Rumble {
            sr,
            reverb: Reverb::new(1, room, 0.6, 30.0, sr),
            hp: Svf::new(35.0, 0.1, sr),
            lp1: Svf::new(lp, 0.3, sr),
            lp2: Svf::new(lp, 0.0, sr),
            drive,
            gain,
        }
    }

    /// Re-tune for a new song, keeping (and clearing) the reverb buffers.
    pub fn reset(&mut self, p: RumbleParams) {
        let (room, lp, drive, gain) = rumble_shape(&p);
        self.reverb.clear();
        self.reverb.room = room;
        self.hp.reset();
        self.lp1 = Svf::new(lp, 0.3, self.sr);
        self.lp2 = Svf::new(lp, 0.0, self.sr);
        self.drive = drive;
        self.gain = gain;
    }

    #[inline]
    pub fn process(&mut self, kick: f32) -> f32 {
        let (wet, _) = self.reverb.process(kick);
        let x = self.lp2.lp(self.lp1.lp(self.hp.hp(wet * 6.0)));
        saturate(x, self.drive) * self.gain
    }
}

// ----------------------------------------------------------------------------- hats

/// Closed and open hi-hat sharing one source. A closed hit chokes the open hat.
/// Variants: 0 metal + noise, 1 pure noise (crisp), 2 dark metallic, 3 shaker.
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
    gain: f32,
    variant: u8,
    attack: f32,
}

impl Hats {
    pub fn new(p: HatParams, sr: f32) -> Hats {
        let (mix, tone, closed_t, open_t, gain) = match p.variant {
            1 => (
                0.0,
                p.tone_hz * 1.1,
                p.closed_decay * 0.8,
                p.open_decay,
                2.6,
            ),
            2 => (
                (p.metal + 0.4).min(1.0),
                p.tone_hz * 0.7,
                p.closed_decay * 1.3,
                p.open_decay * 1.2,
                5.4,
            ),
            3 => (
                0.0,
                p.tone_hz * 0.75,
                p.closed_decay * 1.6,
                p.open_decay * 0.8,
                1.15,
            ),
            _ => (p.metal, p.tone_hz, p.closed_decay, p.open_decay, 4.0),
        };
        let mut closed = Decay::default();
        closed.set_time(closed_t, sr);
        let mut open = Decay::default();
        open.set_time(open_t, sr);
        Hats {
            sr,
            metal: MetalBank::new(),
            noise: Noise::new(0x1A7),
            hp: Svf::new(tone, 0.15, sr),
            peak: Svf::new(tone * 1.25, if p.variant == 3 { 0.6 } else { 0.4 }, sr),
            closed,
            open,
            mix,
            scale: tone / 8000.0 * 1.6,
            gain,
            variant: p.variant,
            attack: 1.0,
        }
    }

    pub fn trigger_closed(&mut self, vel: f32) {
        self.closed.trigger(vel);
        self.open.kill();
        self.attack = 0.0;
    }

    pub fn trigger_open(&mut self, vel: f32) {
        self.open.trigger(vel);
        self.attack = 0.0;
    }

    /// (closed, open)
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        if !self.closed.is_active() && !self.open.is_active() {
            return (0.0, 0.0);
        }
        let n = self.noise.next();
        let src = if self.mix > 0.0 {
            self.metal.next(self.scale, self.sr) * self.mix + n * (1.0 - self.mix)
        } else {
            n
        };
        let h = self.hp.hp(src);
        let mut s = h + self.peak.bp(h) * 0.6;
        // The shaker has a soft attack (about 4 ms) instead of a hard transient.
        if self.variant == 3 && self.attack < 1.0 {
            self.attack = (self.attack + 1.0 / (0.004 * self.sr)).min(1.0);
            s *= self.attack;
        }
        (
            s * self.gain * self.closed.next(),
            s * self.gain * self.open.next(),
        )
    }
}

/// Ride or crash cymbal.
/// Ride variants: 0 standard, 1 bell, 2 washy. Crash variants: 0 standard, 1 dark, 2 splash.
pub struct Cymbal {
    sr: f32,
    metal: MetalBank,
    noise: Noise,
    bp: Svf,
    hp: Svf,
    env: Decay,
    scale: f32,
    noise_mix: f32,
    gain: f32,
    bp_mix: f32,
}

impl Cymbal {
    pub fn ride(p: ToneParams, sr: f32) -> Cymbal {
        match p.variant {
            1 => Cymbal::new(p.tone_hz * 1.15, p.decay * 0.8, 0.25, 0.6, 3.4, 0x51DE, sr),
            2 => Cymbal::new(p.tone_hz * 0.85, p.decay * 1.4, 0.85, 0.2, 1.55, 0x51DE, sr),
            _ => Cymbal::new(p.tone_hz, p.decay, 0.6, 0.35, 2.5, 0x51DE, sr),
        }
    }

    pub fn crash(variant: u8, sr: f32) -> Cymbal {
        match variant {
            1 => Cymbal::new(3500.0, 2.4, 0.75, 0.3, 2.0, 0xC0A5, sr),
            2 => Cymbal::new(6500.0, 0.8, 0.6, 0.35, 3.5, 0xC0A5, sr),
            _ => Cymbal::new(5000.0, 1.8, 0.7, 0.35, 2.5, 0xC0A5, sr),
        }
    }

    fn new(
        tone: f32,
        decay: f32,
        noise_mix: f32,
        res: f32,
        gain: f32,
        seed: u32,
        sr: f32,
    ) -> Cymbal {
        let mut env = Decay::default();
        env.set_time(decay, sr);
        Cymbal {
            sr,
            metal: MetalBank::new(),
            noise: Noise::new(seed),
            bp: Svf::new(tone, res, sr),
            hp: Svf::new(tone * 0.6, 0.1, sr),
            env,
            scale: tone / 5000.0 * 2.3,
            noise_mix,
            gain,
            bp_mix: if res > 0.5 { 1.6 } else { 1.0 },
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
        (x * 0.6 + self.bp.bp(x) * self.bp_mix) * self.gain * self.env.next()
    }
}

// ----------------------------------------------------------------------------- clap / snare / perc

/// Clap: band-passed noise bursts and a tail.
/// Variants: 0 classic (3 bursts), 1 tight (one burst, short), 2 wide (4 spread bursts,
/// long tail, lower), 3 gritty (saturated, brighter).
pub struct Clap {
    sr: f32,
    noise: Noise,
    bp: Svf,
    hp: Svf,
    t: f32,
    active: bool,
    vel: f32,
    decay: f32,
    bursts: f32,
    spacing: f32,
    grit: f32,
    gain: f32,
}

impl Clap {
    pub fn new(p: ToneParams, sr: f32) -> Clap {
        let (tone, res, decay, bursts, spacing, grit, gain) = match p.variant {
            1 => (p.tone_hz * 1.3, 0.35, p.decay * 0.5, 1.0, 0.01, 0.0, 1.25),
            2 => (p.tone_hz * 0.8, 0.5, p.decay * 1.5, 4.0, 0.013, 0.0, 0.95),
            3 => (p.tone_hz * 1.15, 0.4, p.decay * 0.9, 3.0, 0.009, 1.0, 0.55),
            _ => (p.tone_hz, 0.45, p.decay, 3.0, 0.01, 0.0, 1.0),
        };
        Clap {
            sr,
            noise: Noise::new(0xC1A9),
            bp: Svf::new(tone, res, sr),
            hp: Svf::new(600.0, 0.0, sr),
            t: 0.0,
            active: false,
            vel: 1.0,
            decay,
            bursts,
            spacing,
            grit,
            gain,
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
        let burst_end = self.bursts * self.spacing;
        let env = if t < burst_end {
            let k = (t / self.spacing).floor();
            (-(t - k * self.spacing) / 0.0025).exp()
        } else {
            0.85 * (-(t - burst_end) * 4.6 / self.decay).exp()
        };
        let n = self.noise.next();
        let mut x = self.bp.bp(n) * 1.6 + self.hp.hp(n) * 0.25;
        if self.grit > 0.0 {
            x = saturate(x * 2.5, 2.0) * 0.8 + self.hp.hp(n) * 0.3;
        }
        self.t += 1.0 / self.sr;
        if t > self.decay + burst_end + 0.02 {
            self.active = false;
        }
        x * env * self.vel * self.gain
    }
}

/// Snare for backbeats, fills and rolls: pitched body plus noise.
/// Variants: 0 standard, 1 tight (higher, short), 2 noisy/industrial, 3 metallic ping.
pub struct Snare {
    sr: f32,
    p: SnareParams,
    osc: Osc,
    osc2: Osc,
    noise: Noise,
    hp: Svf,
    bp: Svf,
    t: f32,
    active: bool,
    vel: f32,
}

impl Snare {
    pub fn new(mut p: SnareParams, sr: f32) -> Snare {
        let (hp, bp) = match p.variant {
            1 => {
                p.tone_hz *= 1.25;
                p.decay *= 0.7;
                (2500.0, 4000.0)
            }
            2 => {
                p.snappy = (p.snappy + 0.3).min(1.0);
                p.decay *= 1.2;
                (900.0, 2200.0)
            }
            3 => {
                p.decay *= 0.6;
                (2000.0, 3200.0)
            }
            _ => (1800.0, 3000.0),
        };
        Snare {
            sr,
            p,
            osc: Osc::default(),
            osc2: Osc::default(),
            noise: Noise::new(0x5AA3),
            hp: Svf::new(hp, 0.1, sr),
            bp: Svf::new(bp, 0.5, sr),
            t: 0.0,
            active: false,
            vel: 1.0,
        }
    }

    pub fn trigger(&mut self, vel: f32) {
        self.t = 0.0;
        self.osc.phase = 0.0;
        self.osc2.phase = 0.0;
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
        let mut body = self.osc.sine(f, self.sr) * (-t * 4.6 / (p.decay * 0.6)).exp();
        let n = self.noise.next();
        let noise = match p.variant {
            2 => saturate(self.bp.bp(n) * 2.0, 2.5) * (-t * 4.6 / p.decay).exp(),
            _ => self.hp.hp(n) * (-t * 4.6 / p.decay).exp(),
        };
        if p.variant == 3 {
            // Inharmonic second partial: a short metallic ping.
            body += self.osc2.sine(f * 2.7, self.sr) * (-t / 0.02).exp() * 0.5;
        }
        self.t += 1.0 / self.sr;
        if t > p.decay + 0.05 {
            self.active = false;
        }
        // Level-matched to variant 0 (measured with the `variants` example).
        let gain = match p.variant {
            1 => 1.2,
            2 => 0.65,
            3 => 1.25,
            _ => 1.0,
        };
        saturate(body * (1.0 - p.snappy * 0.5) + noise * p.snappy, 1.5) * self.vel * gain
    }
}

/// Rim: short percussive click. Variants: 0 resonant click, 1 wood block, 2 clave.
pub struct Rim {
    sr: f32,
    osc: Osc,
    noise: Noise,
    bp: Svf,
    env: Decay,
    tone: f32,
    variant: u8,
}

impl Rim {
    pub fn new(p: ToneParams, sr: f32) -> Rim {
        let decay = match p.variant {
            1 => p.decay * 1.2,
            2 => p.decay * 0.7,
            _ => p.decay,
        };
        let mut env = Decay::default();
        env.set_time(decay, sr);
        Rim {
            sr,
            osc: Osc::default(),
            noise: Noise::new(0x219),
            bp: Svf::new(p.tone_hz, 0.45, sr),
            env,
            tone: p.tone_hz,
            variant: p.variant,
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
        let x = match self.variant {
            // Wood block: hollow mid tone, a touch of noise.
            1 => {
                (self.osc.sine(self.tone * 0.4, self.sr) * 1.4
                    + self.bp.bp(self.noise.next()) * 0.6)
                    * 0.59
            }
            // Clave: bright, almost pure tone.
            2 => self.osc.sine(self.tone * 1.1, self.sr) * 1.03,
            _ => {
                self.bp.bp(self.noise.next()) * 2.0 + self.osc.sine(self.tone * 0.5, self.sr) * 0.5
            }
        };
        x * self.env.next()
    }
}

/// Tuned tom with a pitch drop. Variants: 0 standard, 1 noisy, 2 deep boom.
pub struct Tom {
    sr: f32,
    osc: Osc,
    t: f32,
    active: bool,
    vel: f32,
    freq: f32,
    decay: f32,
    noise: Noise,
    skin: Svf,
    skin_mix: f32,
    sweep: f32,
    pitch: f32,
}

impl Tom {
    pub fn new(p: ToneParams, sr: f32) -> Tom {
        let (decay, skin_mix, sweep, pitch) = match p.variant {
            1 => (p.decay * 0.8, 1.2, 0.25, 1.0),
            2 => (p.decay * 1.5, 0.3, 0.5, 0.75),
            _ => (p.decay, 0.5, 0.35, 1.0),
        };
        Tom {
            sr,
            osc: Osc::default(),
            t: 0.0,
            active: false,
            vel: 1.0,
            freq: p.tone_hz,
            decay,
            noise: Noise::new(0x7037),
            skin: Svf::new(p.tone_hz * 4.0, 0.2, sr),
            skin_mix,
            sweep,
            pitch,
        }
    }

    pub fn trigger(&mut self, freq: f32, vel: f32) {
        self.freq = freq * self.pitch;
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
        // Small pitch drop (a big one sounds like a laser "pew") plus a bit of skin noise
        // keeps the tom percussive rather than melodic.
        let f = self.freq * (1.0 + self.sweep * (-t / 0.025).exp());
        let amp = (-t * 4.6 / self.decay).exp();
        let skin = self.skin.bp(self.noise.next()) * (-t / 0.02).exp() * self.skin_mix;
        self.t += 1.0 / self.sr;
        if t > self.decay + 0.05 {
            self.active = false;
        }
        saturate(self.osc.sine(f, self.sr) * amp + skin, 1.3) * self.vel
    }
}

// ----------------------------------------------------------------------------- bass

/// Rolling bass through an enveloped low-pass, then saturation.
/// Variants: 0 saw + sine sub, 1 reese (two detuned saws + sub), 2 square + sub,
/// 3 sine pluck (FM click on a sine).
pub struct Bass {
    sr: f32,
    p: BassParams,
    saw: Osc,
    saw2: Osc,
    sub: Osc,
    filt: Svf,
    fenv: Decay,
    amp: Adsr,
    freq: f32,
    gate: u32,
    vel: f32,
    accent: f32,
    gain: f32,
    /// External filter amount 0..1 (arrangement automation; 1 = as designed).
    pub filter_mul: f32,
}

impl Bass {
    pub fn new(p: BassParams, sr: f32) -> Bass {
        let mut fenv = Decay::default();
        fenv.set_time(p.decay, sr);
        // Level-matched to variant 0 (measured with the `variants` example).
        let gain = match p.variant {
            1 => 0.73,
            2 => 0.53,
            3 => 0.77,
            _ => 1.0,
        };
        Bass {
            sr,
            p,
            saw: Osc::default(),
            saw2: Osc::with_phase(0.37),
            sub: Osc::default(),
            filt: Svf::new(p.cutoff_hz, p.resonance, sr),
            fenv,
            amp: Adsr::new(0.0015, p.decay * 1.5, 0.55, 0.012, sr),
            freq: 55.0,
            gate: 0,
            vel: 1.0,
            accent: 0.0,
            gain,
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
        let sr = self.sr;
        let fe = self.fenv.next();
        let sub = self.sub.sine(self.freq, sr);
        let x = match p.variant {
            1 => {
                (self.saw.saw(self.freq * 1.006, sr) + self.saw2.saw(self.freq * 0.994, sr))
                    * 0.5
                    * (1.0 - p.sub_mix * 0.5)
                    + sub * p.sub_mix
            }
            2 => self.saw.square(self.freq, sr) * 0.7 * (1.0 - p.sub_mix * 0.5) + sub * p.sub_mix,
            3 => {
                // FM: the modulation index follows the filter envelope -> click, then sine.
                let m = self.saw2.sine(self.freq * 2.0, sr) * fe * 1.5;
                self.saw.phase = (self.saw.phase + m * 0.02).rem_euclid(1.0);
                self.saw.sine(self.freq, sr) * 0.6 + sub * 0.4
            }
            _ => self.saw.saw(self.freq, sr) * (1.0 - p.sub_mix * 0.5) + sub * p.sub_mix,
        };
        let cutoff = p.cutoff_hz
            * (0.3 + 0.7 * self.filter_mul)
            * (1.0 + p.env_mod * fe * (1.0 + 0.5 * self.accent) * (0.4 + 0.6 * self.filter_mul));
        self.filt.set(cutoff, p.resonance, sr);
        let y = if p.variant == 3 { x } else { self.filt.lp(x) };
        saturate(y * self.amp.next(), p.drive) * self.vel * self.gain
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
    /// Samples since the last attack (glide vibrato fades in).
    since: u32,
    played: bool,
    vib: Osc,
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
            since: 0,
            played: false,
            vib: Osc::default(),
        }
        .configured()
    }

    fn configured(mut self) -> Lead {
        if self.p.kind == LeadKind::Glide {
            // Long portamento, soft attack, longer release: the house glide.
            self.glide = smooth_coef(0.09, self.sr);
            self.amp = Adsr::new(0.012, 0.3, 0.85, 0.12, self.sr);
        }
        self
    }

    /// `legato`: the previous note slides into this one (no envelope retrigger).
    pub fn trigger(&mut self, freq: f32, vel: f32, accent: bool, gate_samples: u32, legato: bool) {
        self.target = freq;
        self.gate = gate_samples.max(1);
        self.vel = vel;
        self.accent = if accent { 1.0 } else { 0.0 };
        let glide = self.p.kind == LeadKind::Glide;
        if !(legato && self.amp.is_active()) {
            // Glide leads always slide from the previous pitch (except the very first note).
            if !glide || !self.played {
                self.freq = freq;
            }
            self.played = true;
            self.since = 0;
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
            LeadKind::Glide => {
                // Saw + square through a soft low-pass, a sine an octave below, delayed vibrato.
                self.since = self.since.saturating_add(1);
                let t = self.since as f32 / sr;
                let depth = 0.007 * ((t - 0.25) * 2.5).clamp(0.0, 1.0);
                let f = self.freq * (1.0 + depth * self.vib.sine(5.2, sr));
                let x = self.osc.saw(f, sr) * (1.0 - 0.5 * p.square)
                    + self.stack[0].square(f * 1.003, sr) * 0.35 * (0.3 + p.square)
                    + self.modulator.sine(f * 0.5, sr) * 0.45;
                let cutoff = p.cutoff_hz * 1.6 * open * (1.0 + 0.6 * p.env_mod * fe);
                self.f1
                    .set(cutoff.min(12000.0), 0.12 + 0.3 * p.resonance, sr);
                self.f1.lp(x * 0.6)
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
    c: Osc,
    freq: f32,
    filt: Svf,
    fenv: Decay,
    amp: Decay,
    vel: f32,
}

/// Chord stab (also used for the arp): plucky filter envelope, fed into delay/reverb.
/// Variants: 0 detuned saws, 1 pulse (hollow squares), 2 FM bell, 3 organ (sine, octave, fifth).
pub struct Stab {
    sr: f32,
    p: StabParams,
    voices: [StabVoice; STAB_VOICES],
    next: usize,
    detune: f32,
    /// External cutoff multiplier (slow filter movement from the arrangement).
    pub filter_mul: f32,
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
                c: Osc::with_phase(i as f32 * 0.13 % 1.0),
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
            filter_mul: 1.0,
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
            if self.p.variant == 2 {
                v.c.phase = 0.0;
            }
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
            let fe = v.fenv.next();
            let x = match self.p.variant {
                1 => {
                    (v.a.square(v.freq * self.detune, sr) + v.b.square(v.freq / self.detune, sr))
                        * 0.6
                }
                2 => {
                    // FM bell: inharmonic modulator, index decays with the filter envelope.
                    let m = v.c.sine(v.freq * 3.5, sr) * fe * 2.0;
                    v.a.phase = (v.a.phase + m * 0.02).rem_euclid(1.0);
                    v.a.sine(v.freq, sr) * 1.12
                }
                3 => {
                    (v.a.sine(v.freq, sr)
                        + 0.6 * v.b.sine(v.freq * 2.0, sr)
                        + 0.4 * v.c.sine(v.freq * 3.0, sr))
                        * 0.9
                }
                _ => v.a.saw(v.freq * self.detune, sr) + v.b.saw(v.freq / self.detune, sr),
            };
            let y = if self.p.variant == 2 || self.p.variant == 3 {
                x // sine-based voices need no filter
            } else {
                let cutoff = self.p.cutoff_hz * self.filter_mul * (1.0 + self.p.env_mod * fe);
                v.filt.set(cutoff, 0.3, sr);
                v.filt.lp(x)
            };
            out += y * v.amp.next() * v.vel;
        }
        out * 0.35
    }
}

// ----------------------------------------------------------------------------- pad

const PAD_VOICES: usize = 8;

struct PadVoice {
    a: Osc,
    b: Osc,
    c: Osc,
    d: Osc,
    freq: f32,
    filt_l: Svf,
    filt_r: Svf,
    env: Adsr,
}

/// Atmospheric pad: slow attack, two detuned oscillators per note spread left/right.
/// Variants: 0 saws, 1 hollow squares, 2 soft sine choir (with vibrato), 3 strings (3 saws per side).
pub struct Pad {
    sr: f32,
    p: PadParams,
    voices: [PadVoice; PAD_VOICES],
    next: usize,
    detune: f32,
    lfo: Osc,
    ctrl: u32,
}

impl Pad {
    pub fn new(p: PadParams, sr: f32) -> Pad {
        let voices = core::array::from_fn(|i| PadVoice {
            a: Osc::with_phase(i as f32 * 0.23 % 1.0),
            b: Osc::with_phase(i as f32 * 0.71 % 1.0),
            c: Osc::with_phase(i as f32 * 0.47 % 1.0),
            d: Osc::with_phase(i as f32 * 0.89 % 1.0),
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
            ctrl: 0,
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
        let update = self.ctrl == 0;
        self.ctrl = (self.ctrl + 1) & 15;
        let (mut l, mut r) = (0.0, 0.0);
        for v in self.voices.iter_mut() {
            if !v.env.is_active() {
                continue;
            }
            let e = v.env.next();
            // Slow LFO: coefficients every 16 samples are plenty.
            if update {
                v.filt_l.set(cutoff, 0.1, sr);
                v.filt_r.set(cutoff, 0.1, sr);
            }
            let (fl, fr) = (v.freq * self.detune, v.freq / self.detune);
            let (xl, xr) = match self.p.variant {
                1 => (v.a.square(fl, sr) * 0.57, v.b.square(fr, sr) * 0.57),
                2 => {
                    let vib = 1.0 + 0.003 * lfo;
                    (
                        (v.a.sine(fl * vib, sr) + 0.3 * v.c.sine(fl * 2.0, sr)) * 0.75,
                        (v.b.sine(fr * vib, sr) + 0.3 * v.d.sine(fr * 2.0, sr)) * 0.75,
                    )
                }
                3 => {
                    let d2 = self.detune * self.detune;
                    (
                        (v.a.saw(fl, sr) + v.c.saw(v.freq * d2, sr)) * 0.75,
                        (v.b.saw(fr, sr) + v.d.saw(v.freq / d2, sr)) * 0.75,
                    )
                }
                _ => (v.a.saw(fl, sr), v.b.saw(fr, sr)),
            };
            l += v.filt_l.lp(xl) * e;
            r += v.filt_r.lp(xr) * e;
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
    /// Which riser this build uses (set per bar by the deck).
    pub kind: RiserKind,
    swell_hp: Svf,
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
            kind: RiserKind::Noise,
            swell_hp: Svf::new(3000.0, 0.1, sr),
        }
    }

    /// `level` 0..1, `progress` 0..1 through the build, `beat_frac` 0..1 within the beat.
    #[inline]
    pub fn process(&mut self, level: f32, progress: f32, beat_frac: f32) -> f32 {
        if level <= 1e-4 || self.kind == RiserKind::None {
            return 0.0;
        }
        match self.kind {
            RiserKind::Gated => {
                // Gate speeds up: quarters, 8ths, 16ths, 32nds.
                let rate = [1.0, 2.0, 4.0, 8.0][((progress * 4.0) as usize).min(3)];
                let open = (beat_frac * rate).fract() < 0.5;
                let x = self.noise_sweep(progress);
                return if open { x * level } else { 0.0 };
            }
            RiserKind::Swell => {
                // Bright, airy swell that grows towards the end (reverse-cymbal feel).
                let n = self.noise.next();
                self.swell_hp.set(2500.0 + 5000.0 * progress, 0.15, self.sr);
                return self.swell_hp.hp(n) * level * level * 1.6;
            }
            RiserKind::Noise => return self.noise_sweep(progress) * level,
            _ => {}
        }
        let cutoff = 300.0 * (progress * 5.0 * core::f32::consts::LN_2).exp(); // 300 Hz .. 9.6 kHz
        self.bp.set(cutoff, 0.55, self.sr);
        let n = self.noise.next();
        let noise = self.bp.bp(n) * 1.5 + self.hp.hp(n) * 0.15 * progress;
        let mut saw = 0.0;
        if self.pitched > 0.0 {
            let f = self.base_hz * (1.0 + 0.5 * progress); // up a fifth over the build
            const DETUNE: [f32; 5] = [1.0, 1.006, 0.994, 1.013, 0.987];
            for (o, d) in self.saws.iter_mut().zip(DETUNE) {
                saw += o.saw(f * d, self.sr);
            }
            self.saw_lp
                .set(600.0 + 6000.0 * progress * progress, 0.2, self.sr);
            saw = self.saw_lp.lp(saw) * 0.25 * self.pitched;
        }
        (noise + saw * 0.7) * level
    }

    #[inline]
    fn noise_sweep(&mut self, progress: f32) -> f32 {
        let cutoff = 300.0 * (progress * 5.0 * core::f32::consts::LN_2).exp(); // 300 Hz .. 9.6 kHz
        self.bp.set(cutoff, 0.55, self.sr);
        let n = self.noise.next();
        self.bp.bp(n) * 1.5 + self.hp.hp(n) * 0.15 * progress
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
    detune_ratio: f32,
    ctrl: u32,
    noise: Noise,
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
            detune_ratio: 2f32.powf(p.detune / 12.0),
            ctrl: 0,
            noise: Noise::new(0xD20E),
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
        let d = self.detune_ratio;
        let f = self.freq;
        // Variants: 0 saws on root and fifth, 1 sub (sines, driven), 2 noise with a root tone.
        let x = match self.p.variant {
            1 => {
                (self.osc[0].sine(f, sr)
                    + 0.5 * self.osc[1].sine(f * 1.5, sr)
                    + 1.2 * self.osc[4].sine(f * 0.5, sr))
                    * 1.9
            }
            2 => {
                self.noise.next() * 2.9
                    + 1.2 * self.osc[0].sine(f, sr)
                    + 0.8 * self.osc[4].sine(f * 0.5, sr)
            }
            _ => {
                self.osc[0].saw(f * d, sr)
                    + self.osc[1].saw(f / d, sr)
                    + 0.7 * self.osc[2].saw(f * 1.5 * d, sr)
                    + 0.7 * self.osc[3].saw(f * 1.5 / d, sr)
                    + 0.8 * self.osc[4].sine(f * 0.5, sr)
            }
        };
        let lfo = self.lfo.sine(0.045, sr);
        // The cutoff moves slowly: update the coefficients every 16 samples.
        if self.ctrl == 0 {
            self.filt.set(self.p.cutoff_hz * (1.0 + 0.6 * lfo), 0.3, sr);
        }
        self.ctrl = (self.ctrl + 1) & 15;
        saturate(self.filt.lp(x * 0.3) * self.env.next(), self.p.drive) * 0.6
    }
}

// ----------------------------------------------------------------------------- piano

const PIANO_VOICES: usize = 12;
/// Partials per note; the lowest `PIANO_DOUBLED` have a second, slightly detuned string.
const PIANO_PARTIALS: usize = 7;
const PIANO_DOUBLED: usize = 3;
const PIANO_MODES: usize = PIANO_PARTIALS + PIANO_DOUBLED;

/// Damped sine resonator: y[n] = c1 y[n-1] - c2 y[n-2] rings at a fixed frequency with an
/// exponential decay. Ten of them per note make a piano tone (modal synthesis).
#[derive(Clone, Copy, Default)]
struct Mode {
    c1: f32,
    c2: f32,
    y1: f32,
    y2: f32,
    w: f32,
}

impl Mode {
    fn strike(&mut self, w: f32, tau: f32, amp: f32, sr: f32) {
        self.w = w;
        self.set_decay(tau, sr);
        let r = self.c2.sqrt().max(1e-6);
        self.y1 = 0.0;
        self.y2 = -amp * w.sin() / r;
    }

    fn set_decay(&mut self, tau: f32, sr: f32) {
        let r = (-1.0 / (tau.max(0.005) * sr)).exp();
        self.c1 = 2.0 * r * self.w.cos();
        self.c2 = r * r;
    }

    #[inline]
    fn next(&mut self) -> f32 {
        let y = self.c1 * self.y1 - self.c2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[derive(Clone, Copy, Default)]
struct PianoVoice {
    modes: [Mode; PIANO_MODES],
    /// Modes that are below Nyquist and audible.
    count: usize,
    /// Samples until the hammer hits (strummed / humanised chords).
    start_in: u32,
    gate: u32,
    /// Rough envelope of the slowest mode, for voice stealing and switching off.
    env: f32,
    env_mul: f32,
    hammer: f32,
    hammer_mul: f32,
    active: bool,
    pending: bool,
    freq: f32,
    vel: f32,
}

/// Piano without samples: every note is a bank of damped resonators at the (slightly
/// inharmonic) partials of a string, two detuned strings for the low partials (beating,
/// double decay), a short filtered hammer noise and a damper on note-off.
/// Variants: 0 grand, 1 felt (soft, dark), 2 electric (tine with bell partial),
/// 3 bright house piano (short, wide detune).
pub struct Piano {
    sr: f32,
    p: KeysParams,
    voices: [PianoVoice; PIANO_VOICES],
    next: usize,
    noise: Noise,
    hammer_bp: Svf,
    tone: OnePole,
    gain: f32,
    ctrl: u32,
    /// Brightness multiplier from the arrangement (slow filter movement).
    pub filter_mul: f32,
}

impl Piano {
    pub fn new(p: KeysParams, sr: f32) -> Piano {
        let gain = match p.variant {
            1 => 1.35,
            2 => 0.9,
            3 => 0.8,
            _ => 1.0,
        };
        Piano {
            sr,
            p,
            voices: [PianoVoice::default(); PIANO_VOICES],
            next: 0,
            noise: Noise::new(0x51A2_0F11),
            hammer_bp: Svf::new(2500.0, 0.4, sr),
            tone: OnePole::new(8000.0, sr),
            gain,
            ctrl: 0,
            filter_mul: 1.0,
        }
    }

    fn free_voice(&mut self) -> usize {
        // Prefer a silent voice, else the quietest one.
        let mut best = self.next;
        let mut best_env = f32::MAX;
        for k in 0..PIANO_VOICES {
            let i = (self.next + k) % PIANO_VOICES;
            let v = &self.voices[i];
            let e = if v.active || v.pending { v.env } else { -1.0 };
            if e < best_env {
                best_env = e;
                best = i;
            }
        }
        self.next = (best + 1) % PIANO_VOICES;
        best
    }

    /// Play a chord. `spread` = samples between the notes (0 = together).
    pub fn trigger_chord(&mut self, freqs: &[f32], vel: f32, gate_samples: u32, spread: u32) {
        for (k, &f) in freqs.iter().enumerate() {
            let i = self.free_voice();
            let v = &mut self.voices[i];
            v.freq = f;
            v.vel = vel;
            v.start_in = spread * k as u32;
            v.gate = gate_samples.max(1) + v.start_in;
            v.pending = true;
        }
    }

    fn strike(&mut self, i: usize) {
        let sr = self.sr;
        let p = self.p;
        let v = &mut self.voices[i];
        v.pending = false;
        v.active = true;
        let f = v.freq;
        let vel = v.vel;
        let bright = (p.brightness * if p.variant == 1 { 0.45 } else { 1.0 }).clamp(0.0, 1.0);
        // Steeper partial roll-off for soft notes and dark pianos.
        let rolloff = 2.0 - 0.9 * bright - 0.5 * vel;
        let (inharm, detune_cents, decay_mul) = match p.variant {
            1 => (0.0002, 0.6, 0.85),
            2 => (0.0, 0.3, 1.2),
            3 => (0.0003, 3.5, 0.55),
            _ => (0.00035, 0.9, 1.0),
        };
        // Higher notes ring shorter.
        let base_tau = p.decay * decay_mul * (261.6 / f).powf(0.45).clamp(0.25, 2.5);
        let nyq = 0.45 * sr;
        let mut n_modes = 0;
        for n in 0..PIANO_PARTIALS {
            let h = (n + 1) as f32;
            let (fn_, amp, tau) = if p.variant == 2 && n == PIANO_PARTIALS - 1 {
                // Tine: inharmonic bell partial with a fast decay.
                (f * 7.1, 0.35 * vel * vel, 0.06)
            } else if p.variant == 2 && n >= 3 {
                (0.0, 0.0, 0.0)
            } else {
                let hammer_pos = (core::f32::consts::PI * h / 7.3).sin().abs();
                (
                    f * h * (1.0 + inharm * h * h).sqrt(),
                    hammer_pos / h.powf(rolloff),
                    base_tau / (1.0 + 0.55 * (h - 1.0)),
                )
            };
            if fn_ <= 0.0 || fn_ >= nyq || amp < 1e-4 {
                continue;
            }
            let w = core::f32::consts::TAU * fn_ / sr;
            v.modes[n_modes].strike(w, tau, amp, sr);
            n_modes += 1;
            if n < PIANO_DOUBLED {
                // Second string: detuned and decaying faster (the "prompt" sound).
                let f2 = fn_ * (detune_cents / 1200.0 * core::f32::consts::LN_2).exp();
                if f2 < nyq {
                    let w2 = core::f32::consts::TAU * f2 / sr;
                    v.modes[n_modes].strike(w2, tau * 0.45, amp * 0.8, sr);
                    n_modes += 1;
                }
            }
        }
        v.count = n_modes;
        v.env = 1.0;
        v.env_mul = (-1.0 / (base_tau.max(0.01) * sr)).exp();
        v.hammer = 0.12 * vel * (0.4 + bright) * if p.variant == 2 { 0.3 } else { 1.0 };
        v.hammer_mul = (-1.0 / (0.004 * sr)).exp();
    }

    fn damp(v: &mut PianoVoice, sr: f32) {
        for m in v.modes[..v.count].iter_mut() {
            m.set_decay(0.07, sr);
        }
        v.env_mul = (-1.0 / (0.07 * sr)).exp();
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        if self.ctrl == 0 {
            let c = (1200.0 + 9000.0 * self.p.brightness * self.filter_mul).clamp(400.0, 16000.0);
            self.tone.set(c, self.sr);
        }
        self.ctrl = (self.ctrl + 1) & 31;
        let sr = self.sr;
        let mut out = 0.0;
        let mut hammer = 0.0;
        for i in 0..PIANO_VOICES {
            if self.voices[i].pending {
                if self.voices[i].start_in == 0 {
                    self.strike(i);
                } else {
                    self.voices[i].start_in -= 1;
                    continue;
                }
            }
            let v = &mut self.voices[i];
            if !v.active {
                continue;
            }
            if v.gate > 0 {
                v.gate -= 1;
                if v.gate == 0 {
                    Piano::damp(v, sr);
                }
            }
            let mut s = 0.0;
            for m in v.modes[..v.count].iter_mut() {
                s += m.next();
            }
            out += s * v.vel;
            if v.hammer > 1e-4 {
                hammer += v.hammer;
                v.hammer *= v.hammer_mul;
            }
            v.env *= v.env_mul;
            if v.env < 2e-4 {
                v.active = false;
            }
        }
        if hammer > 0.0 {
            out += self.hammer_bp.bp(self.noise.next() * hammer);
        }
        self.tone.lp(out) * 0.32 * self.gain
    }
}

// ----------------------------------------------------------------------------- guitar

const GTR_VOICES: usize = 6;
const GTR_BUF: usize = 1024;

#[derive(Clone, Copy)]
struct GtrVoice {
    buf: [f32; GTR_BUF],
    w: usize,
    delay: f32,
    loop_gain: f32,
    bright: f32,
    lp: f32,
    env: f32,
    env_mul: f32,
    start_in: u32,
    gate: u32,
    active: bool,
    pending: bool,
    freq: f32,
    vel: f32,
    pick: f32,
}

impl Default for GtrVoice {
    fn default() -> Self {
        GtrVoice {
            buf: [0.0; GTR_BUF],
            w: 0,
            delay: 100.0,
            loop_gain: 0.99,
            bright: 0.5,
            lp: 0.0,
            env: 0.0,
            env_mul: 0.0,
            start_in: 0,
            gate: 0,
            active: false,
            pending: false,
            freq: 220.0,
            vel: 0.0,
            pick: 0.5,
        }
    }
}

/// Plucked guitar without samples: Karplus-Strong strings (a delay line one period long
/// with a low-pass in the loop, excited by a filtered noise burst with a pick-position
/// notch) and a simple two-band body resonance.
/// Variants: 0 nylon (soft, warm body), 1 steel (bright), 2 clean electric (no body).
pub struct Guitar {
    sr: f32,
    p: GuitarParams,
    voices: [GtrVoice; GTR_VOICES],
    next: usize,
    noise: Noise,
    body1: Svf,
    body2: Svf,
    /// Brightness multiplier from the arrangement.
    pub filter_mul: f32,
}

impl Guitar {
    pub fn new(p: GuitarParams, sr: f32) -> Guitar {
        Guitar {
            sr,
            p,
            voices: [GtrVoice::default(); GTR_VOICES],
            next: 0,
            noise: Noise::new(0x6A17_A2C3),
            body1: Svf::new(105.0, 0.6, sr),
            body2: Svf::new(240.0, 0.5, sr),
            filter_mul: 1.0,
        }
    }

    /// Pluck notes. `muted`: palm-muted short notes. `spread`: samples between strings
    /// (strum); `down`: strum direction (low to high).
    pub fn trigger_chord(
        &mut self,
        freqs: &[f32],
        vel: f32,
        gate_samples: u32,
        spread: u32,
        muted: bool,
        down: bool,
    ) {
        let n = freqs.len();
        for k in 0..n {
            let f = if down { freqs[k] } else { freqs[n - 1 - k] };
            let i = self.next;
            self.next = (self.next + 1) % GTR_VOICES;
            let v = &mut self.voices[i];
            v.freq = f;
            v.vel = vel * if k == 0 { 1.0 } else { 0.85 };
            v.start_in = spread * k as u32;
            v.gate = gate_samples.max(1) + v.start_in;
            v.pending = true;
            v.pick = if muted { 1.0 } else { 0.0 };
        }
    }

    fn pluck(&mut self, i: usize) {
        let sr = self.sr;
        let p = self.p;
        let muted = self.voices[i].pick > 0.5;
        let mut f = self.voices[i].freq;
        // Keep the period inside the buffer.
        while sr / f > (GTR_BUF - 8) as f32 {
            f *= 2.0;
        }
        let variant_bright = match p.variant {
            0 => 0.6,
            1 => 1.0,
            _ => 0.85,
        };
        let bright = (p.brightness * variant_bright * self.filter_mul).clamp(0.05, 1.0);
        let sustain = if muted {
            0.25
        } else {
            p.sustain * if p.variant == 0 { 0.8 } else { 1.0 }
        };
        let noise_state = &mut self.noise;
        let v = &mut self.voices[i];
        v.pending = false;
        v.active = true;
        // Loop low-pass: brighter = less damping per pass.
        v.bright = if muted { 0.4 } else { 0.45 + 0.5 * bright };
        let lp_delay = (1.0 - v.bright) / v.bright;
        v.delay = (sr / f - lp_delay).max(2.0);
        let period = v.delay as usize;
        v.loop_gain = (-6.9 / (sustain * f)).exp().min(0.9995);
        // Excitation: low-passed noise, then a notch at the pick position.
        let mut z = 0.0;
        let a = 0.15 + 0.8 * bright * if muted { 0.4 } else { 1.0 };
        // Raised-sine window: a softer pick attack and fewer stray peaks.
        let win = core::f32::consts::PI / period as f32;
        for k in 0..period {
            z += (noise_state.next() - z) * a;
            let w = (k as f32 * win).sin();
            v.buf[(v.w + GTR_BUF - period + k) % GTR_BUF] = z * v.vel * w * 1.5;
        }
        let pick = ((period as f32) * 0.18) as usize;
        if pick > 0 {
            for k in (pick..period).rev() {
                let i0 = (v.w + GTR_BUF - period + k) % GTR_BUF;
                let i1 = (v.w + GTR_BUF - period + k - pick) % GTR_BUF;
                v.buf[i0] -= v.buf[i1];
            }
        }
        v.lp = 0.0;
        v.env = 1.0;
        v.env_mul = (-6.9 / (sustain * sr)).exp();
    }

    #[inline]
    pub fn process(&mut self) -> f32 {
        let mut out = 0.0;
        let sr = self.sr;
        for i in 0..GTR_VOICES {
            if self.voices[i].pending {
                if self.voices[i].start_in == 0 {
                    self.pluck(i);
                } else {
                    self.voices[i].start_in -= 1;
                    continue;
                }
            }
            let v = &mut self.voices[i];
            if !v.active {
                continue;
            }
            if v.gate > 0 {
                v.gate -= 1;
                if v.gate == 0 {
                    // Finger lifts: the string is damped within ~60 ms.
                    v.loop_gain = (-6.9 / (0.06 * v.freq)).exp();
                    v.env_mul = (-6.9 / (0.06 * sr)).exp();
                }
            }
            // Fractional delay read (linear interpolation).
            let rp = v.w as f32 + GTR_BUF as f32 - v.delay;
            let i0 = rp as usize;
            let frac = rp - i0 as f32;
            let a = v.buf[i0 % GTR_BUF];
            let b = v.buf[(i0 + 1) % GTR_BUF];
            let y = a + (b - a) * frac;
            v.lp += (y - v.lp) * v.bright;
            let s = v.lp * v.loop_gain + ANTI_DENORMAL;
            v.buf[v.w] = s;
            v.w = (v.w + 1) % GTR_BUF;
            out += y;
            v.env *= v.env_mul;
            if v.env < 1e-4 {
                v.active = false;
            }
        }
        let body = match self.p.variant {
            2 => 0.0,
            0 => 0.5,
            _ => 0.3,
        };
        let y = if body > 0.0 {
            out + body * (self.body1.bp(out) + 0.6 * self.body2.bp(out))
        } else {
            out
        };
        // Soft limit like a clip-on compressor: unity gain for normal levels, pick
        // transients are rounded off instead of hitting the master limiter.
        fast_tanh(y * 0.55) * 2.0
    }
}

#[cfg(test)]
mod keys_tests {
    use super::*;
    use techno_core::profile::{ChordShape, GuitarStyle, KeysStyle};

    /// Frequency of the strongest spectral peak (naive DFT around the expected pitch).
    fn pitch(x: &[f32], sr: f32, lo: f32, hi: f32) -> f32 {
        let mut best = (0.0, 0.0);
        let mut f = lo;
        while f <= hi {
            let w = core::f32::consts::TAU * f / sr;
            let (mut re, mut im) = (0.0f32, 0.0f32);
            for (n, v) in x.iter().enumerate() {
                re += v * (w * n as f32).cos();
                im += v * (w * n as f32).sin();
            }
            let m = re * re + im * im;
            if m > best.1 {
                best = (f, m);
            }
            f += 0.25;
        }
        best.0
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    #[test]
    fn piano_variants_in_tune_and_finite() {
        let sr = 48000.0;
        for variant in 0..4 {
            let mut p = Piano::new(
                KeysParams {
                    variant,
                    style: KeysStyle::Stabs,
                    chord: ChordShape::Seventh,
                    brightness: 0.7,
                    decay: 1.8,
                },
                sr,
            );
            p.trigger_chord(&[220.0], 0.9, 24000, 0);
            let x: Vec<f32> = (0..24000).map(|_| p.process()).collect();
            assert!(
                x.iter().all(|v| v.is_finite() && v.abs() < 2.0),
                "variant {variant}"
            );
            let f = pitch(&x[2000..10000], sr, 200.0, 240.0);
            assert!(cents(f, 220.0).abs() < 15.0, "variant {variant}: {f} Hz");
            // Damper: after note-off the tone dies quickly.
            let tail: Vec<f32> = (0..24000).map(|_| p.process()).collect();
            let peak = tail[20000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak < 1e-3, "variant {variant}: still ringing {peak}");
        }
    }

    #[test]
    fn guitar_strings_in_tune_and_decay() {
        let sr = 48000.0;
        for variant in 0..3 {
            let mut g = Guitar::new(
                GuitarParams {
                    variant,
                    style: GuitarStyle::Picked,
                    chord: ChordShape::Triad,
                    brightness: 0.6,
                    sustain: 1.5,
                },
                sr,
            );
            g.trigger_chord(&[196.0], 0.9, 30000, 0, false, true);
            let x: Vec<f32> = (0..30000).map(|_| g.process()).collect();
            assert!(x.iter().all(|v| v.is_finite()), "variant {variant}");
            let f = pitch(&x[3000..15000], sr, 180.0, 215.0);
            assert!(cents(f, 196.0).abs() < 20.0, "variant {variant}: {f} Hz");
            let late: Vec<f32> = (0..48000).map(|_| g.process()).collect();
            let peak = late[24000..].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak < 1e-3, "variant {variant}: string never stops {peak}");
        }
    }
}
