//! Channel strips and the master bus.

use crate::dsp::*;

#[derive(Clone, Copy, Debug)]
pub struct ChannelSettings {
    pub gain: f32,
    pub pan: f32,
    /// How strongly the kick sidechain ducks this channel (0..1).
    pub sidechain: f32,
    pub delay_send: f32,
    pub reverb_send: f32,
    /// High-pass cutoff in Hz to keep the low end clean (0 = off).
    pub hp_hz: f32,
}

/// Mono-in channel strip with click-free on/off (smoothed level) and sends.
pub struct Channel {
    pub settings: ChannelSettings,
    /// Extra delay send on top of the setting (delay throws).
    pub delay_boost: f32,
    /// Extra gain factor on top of the setting (arrangement automation).
    pub gain_mul: f32,
    /// Extra reverb send on top of the setting (wandering effects).
    pub reverb_boost: f32,
    level: Smoother,
    hp: Option<Svf>,
    hp_r: Option<Svf>,
    pan_l: f32,
    pan_r: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ChannelOut {
    pub l: f32,
    pub r: f32,
    pub delay: f32,
    pub reverb: f32,
}

impl Channel {
    pub fn new(settings: ChannelSettings, sr: f32) -> Channel {
        let (pan_l, pan_r) = pan_gains(settings.pan);
        let hp = (settings.hp_hz > 0.0).then(|| Svf::new(settings.hp_hz, 0.1, sr));
        let hp_r = hp.clone();
        Channel {
            settings,
            delay_boost: 0.0,
            gain_mul: 1.0,
            reverb_boost: 0.0,
            level: Smoother::new(0.0, 0.008, sr),
            hp,
            hp_r,
            pan_l,
            pan_r,
        }
    }

    /// Fade the channel in (true) or out (false) without clicks.
    pub fn set_on(&mut self, on: bool) {
        self.level.target = if on { 1.0 } else { 0.0 };
    }

    #[inline]
    pub fn process(&mut self, x: f32, duck: f32) -> ChannelOut {
        let s = &self.settings;
        let lvl = self.level.next();
        let g = s.gain * self.gain_mul * lvl * (1.0 - s.sidechain * duck);
        let x = match self.hp.as_mut() {
            Some(f) => f.hp(x),
            None => x,
        };
        let y = x * g;
        ChannelOut {
            l: y * self.pan_l,
            r: y * self.pan_r,
            delay: y * (s.delay_send + self.delay_boost),
            reverb: y * (s.reverb_send + self.reverb_boost),
        }
    }

    /// Stereo-in variant (pan acts as balance).
    #[inline]
    pub fn process_stereo(&mut self, l: f32, r: f32, duck: f32) -> ChannelOut {
        let s = &self.settings;
        let lvl = self.level.next();
        let g = s.gain * self.gain_mul * lvl * (1.0 - s.sidechain * duck);
        let (l, r) = match (self.hp.as_mut(), self.hp_r.as_mut()) {
            (Some(fl), Some(fr)) => (fl.hp(l), fr.hp(r)),
            _ => (l, r),
        };
        let (yl, yr) = (l * g, r * g);
        let m = (yl + yr) * 0.5;
        ChannelOut {
            l: yl * self.pan_l * core::f32::consts::SQRT_2,
            r: yr * self.pan_r * core::f32::consts::SQRT_2,
            delay: m * (s.delay_send + self.delay_boost),
            reverb: m * (s.reverb_send + self.reverb_boost),
        }
    }

    pub fn is_silent(&self) -> bool {
        self.level.value < 1e-4 && self.level.target == 0.0
    }
}

/// Master: glue compressor, gentle saturation, brickwall limiter.
pub struct MasterBus {
    comp: Compressor,
    limiter: Limiter,
    hp_l: Svf,
    hp_r: Svf,
    pub gain: f32,
}

impl MasterBus {
    pub fn new(sr: f32) -> MasterBus {
        MasterBus {
            comp: Compressor::new(-12.0, 2.5, 0.010, 0.15, 2.0, sr),
            limiter: Limiter::new(-0.8, 0.004, 0.12, sr),
            hp_l: Svf::new(22.0, 0.0, sr),
            hp_r: Svf::new(22.0, 0.0, sr),
            gain: 1.0,
        }
    }

    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let l = self.hp_l.hp(l * self.gain);
        let r = self.hp_r.hp(r * self.gain);
        let (l, r) = self.comp.process(l, r);
        let (l, r) = (saturate(l, 1.2), saturate(r, 1.2));
        self.limiter.process(l, r)
    }

    pub fn compression_db(&self) -> f32 {
        self.comp.reduction_db()
    }

    pub fn limiter_db(&self) -> f32 {
        self.limiter.gain_db()
    }
}
