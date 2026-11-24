//! A deck plays one song: it turns the song DNA into note triggers on the musical clock
//! and runs the instruments through their channel strips and send effects.

use techno_core::dna::{bass_base_midi, Note, SongDna};
use techno_core::profile::{Backbeat, ChordShape, LeadKind};
use techno_core::structure::{Automation, BarPlan, Fill, SongPlan};
use techno_core::theory::midi_to_hz;
use techno_core::time::{samples_per_tick, seconds_per_bar, Tick, TICKS_PER_BAR, TICKS_PER_STEP};
use techno_core::{Track, TrackSet};
use techno_synth::dsp::{Duck, PingPongDelay, Reverb, Smoother, Svf};
use techno_synth::instruments::*;
use techno_synth::mixer::{Channel, ChannelOut, ChannelSettings};

fn settings(
    gain: f32,
    pan: f32,
    sidechain: f32,
    delay_send: f32,
    reverb_send: f32,
    hp_hz: f32,
) -> ChannelSettings {
    ChannelSettings {
        gain,
        pan,
        sidechain,
        delay_send,
        reverb_send,
        hp_hz,
    }
}

/// Default mix. Kept in one place so it is easy to tune by ear.
fn channel_settings(t: Track, dna: &SongDna) -> ChannelSettings {
    let sc = dna.sound.mix.sidechain_depth;
    match t {
        //                       gain   pan   sc    dly   rev   hp
        Track::Kick => settings(0.95, 0.0, 0.0, 0.0, 0.0, 0.0),
        Track::Rumble => settings(0.30 * dna.sound.rumble.amount, 0.0, 1.0, 0.0, 0.0, 0.0),
        Track::Bass => settings(0.40, 0.0, sc, 0.0, 0.0, 30.0),
        Track::HatClosed => settings(0.24, 0.15, 0.15, 0.0, 0.04, 0.0),
        Track::HatOpen => settings(0.20, -0.12, 0.3, 0.0, 0.06, 0.0),
        Track::Clap => settings(0.30, 0.0, 0.0, 0.04, 0.16, 180.0),
        Track::Snare => settings(0.30, 0.0, 0.0, 0.0, 0.14, 120.0),
        Track::Rim => settings(0.10, -0.35, 0.2, 0.14, 0.08, 250.0),
        Track::Tom => settings(0.22, 0.25, 0.3, 0.05, 0.08, 60.0),
        Track::Ride => settings(0.12, 0.3, 0.2, 0.0, 0.06, 0.0),
        Track::Crash => settings(0.16, -0.2, 0.2, 0.0, 0.08, 0.0),
        Track::Stab => settings(0.22, 0.0, sc * 0.8, 0.28, 0.22, 250.0),
        Track::Lead => settings(0.14, 0.0, sc * 0.7, 0.16, 0.10, 180.0),
        Track::Pad => settings(0.18, 0.0, sc * 0.9, 0.0, 0.30, 200.0),
        Track::Arp => settings(0.38, 0.2, sc * 0.8, 0.35, 0.15, 250.0),
        Track::Drone => settings(0.08, 0.0, sc, 0.05, 0.25, 90.0),
        Track::Riser => settings(0.20, 0.0, 0.0, 0.10, 0.25, 150.0),
        Track::Impact => settings(0.45, 0.0, 0.0, 0.0, 0.35, 0.0),
    }
}

/// Fold a progression root so the harmony stays in one register (VI and VII go below).
#[inline]
fn fold_root(root: i32) -> i32 {
    if root >= 4 {
        root - 7
    } else {
        root
    }
}

/// Small deterministic velocity variation (no RNG state in the audio path).
#[inline]
fn humanize(step: u64, salt: u64) -> f32 {
    let h = (step.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ salt).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    ((h >> 40) as f32 / (1u64 << 24) as f32) * 0.12 - 0.06
}

/// Tracks whose channel stays open regardless of the bar plan: they are one-shots
/// (fills, crashes, impacts) whose triggers are already gated by the plan.
const ONE_SHOTS: [Track; 4] = [Track::Crash, Track::Impact, Track::Tom, Track::Snare];

pub struct Deck {
    pub dna: SongDna,
    tick: u64,
    until_tick: f64,
    spt: f64,
    step_samples: f32,
    swing_ticks: u32,
    pub plan: BarPlan,
    bass_base: i32,
    tom_base: f32,

    kick: Kick,
    rumble: Rumble,
    hats: Hats,
    clap: Clap,
    snare: Snare,
    rim: Rim,
    tom: Tom,
    ride: Cymbal,
    crash: Cymbal,
    bass: Bass,
    stab: Stab,
    lead: Lead,
    pad: Pad,
    riser: Riser,
    impact: Impact,
    arp: Stab,
    drone: Drone,

    delay: PingPongDelay,
    reverb: Reverb,
    duck: Duck,
    channels: [Channel; Track::COUNT],
    prev_lead_slide: bool,
    pad_root: Option<i32>,

    // Automation, smoothed per sample.
    lead_filter: Smoother,
    bass_filter: Smoother,
    highpass: Smoother,
    fx_send: Smoother,
    riser_level: Smoother,
    riser_progress: f32,
    hp_l: Svf,
    hp_r: Svf,
    sr: f32,

    /// User/debug overrides applied on top of the arrangement.
    pub mute: TrackSet,
    pub solo: TrackSet,
}

impl Deck {
    pub fn new(dna: SongDna, sample_rate: f32) -> Deck {
        let beat = 60.0 / dna.bpm;
        let rumble = Rumble::new(dna.sound.rumble, sample_rate);
        let delay = PingPongDelay::new(2.0, beat * 0.75, 0.38, sample_rate);
        let reverb = Reverb::new(2, 0.82, 0.45, 250.0, sample_rate);
        Deck::assemble(dna, sample_rate, rumble, delay, reverb)
    }

    /// A deck for a new song that reuses this deck's large buffers (rumble reverb, delay,
    /// reverb). Avoids allocating audio buffers, so it is safe on the audio thread.
    pub fn recycle(self, dna: SongDna) -> Deck {
        let Deck {
            mut rumble,
            mut delay,
            mut reverb,
            sr,
            ..
        } = self;
        rumble.reset(dna.sound.rumble);
        delay.reset(60.0 / dna.bpm * 0.75, sr);
        reverb.clear();
        Deck::assemble(dna, sr, rumble, delay, reverb)
    }

    fn assemble(
        dna: SongDna,
        sample_rate: f32,
        rumble: Rumble,
        delay: PingPongDelay,
        reverb: Reverb,
    ) -> Deck {
        let sr = sample_rate;
        let s = &dna.sound;
        let spt = samples_per_tick(dna.bpm as f64, sr as f64);
        let mut tom_p = s.tom;
        // Tune the tom to the key: tonic folded into the tom's range.
        let mut f = midi_to_hz(dna.key.root as f32 + 36.0);
        while f > tom_p.tone_hz * 1.41 {
            f *= 0.5;
        }
        while f < tom_p.tone_hz / 1.41 {
            f *= 2.0;
        }
        tom_p.tone_hz = f;
        let bass_base = bass_base_midi(dna.key);

        let channels =
            core::array::from_fn(|i| Channel::new(channel_settings(Track::ALL[i], &dna), sr));
        Deck {
            tick: 0,
            until_tick: 0.0,
            spt,
            step_samples: (spt * TICKS_PER_STEP as f64) as f32,
            swing_ticks: (dna.swing * 8.0).round() as u32,
            plan: dna.plan.bar(0),
            bass_base,
            tom_base: tom_p.tone_hz,
            kick: Kick::new(s.kick, sr),
            rumble,
            hats: Hats::new(s.hats, sr),
            clap: Clap::new(s.clap, sr),
            snare: Snare::new(s.snare, sr),
            rim: Rim::new(s.rim, sr),
            tom: Tom::new(tom_p, sr),
            ride: Cymbal::ride(s.ride, sr),
            crash: Cymbal::crash(sr),
            bass: Bass::new(s.bass, sr),
            stab: Stab::new(s.stab, sr),
            lead: Lead::new(s.lead, sr),
            pad: Pad::new(s.pad, sr),
            riser: Riser::new(midi_to_hz((bass_base + 24) as f32), 1.0, sr),
            arp: Stab::new(s.arp, sr),
            drone: Drone::new(s.drone, sr),
            impact: Impact::new(seconds_per_bar(dna.bpm as f64) as f32, sr),
            delay,
            reverb,
            duck: Duck::new(s.mix.sidechain_release, sr),
            channels,
            prev_lead_slide: false,
            pad_root: None,
            lead_filter: Smoother::new(0.0, 0.3, sr),
            bass_filter: Smoother::new(0.5, 0.3, sr),
            highpass: Smoother::new(0.0, 0.1, sr),
            fx_send: Smoother::new(1.0, 0.5, sr),
            riser_level: Smoother::new(0.0, 0.05, sr),
            riser_progress: 0.0,
            hp_l: Svf::new(20.0, 0.0, sr),
            hp_r: Svf::new(20.0, 0.0, sr),
            sr,
            mute: TrackSet::NONE,
            solo: TrackSet::NONE,
            dna,
        }
    }

    pub fn song_plan(&self) -> &SongPlan {
        &self.dna.plan
    }

    /// Current musical position.
    pub fn position(&self) -> Tick {
        Tick(self.tick)
    }

    pub fn total_bars(&self) -> u32 {
        self.dna.plan.total_bars
    }

    /// True once the song and its tails are over.
    pub fn finished(&self) -> bool {
        Tick(self.tick).bar() >= self.total_bars() + 2
    }

    /// `Some(bar)` if the next processed sample starts that bar. Used by the set to start
    /// an incoming deck exactly in phase with the running one.
    pub fn bar_starts_now(&self) -> Option<u32> {
        (self.until_tick <= 0.0 && self.tick % TICKS_PER_BAR as u64 == 0)
            .then(|| Tick(self.tick).bar())
    }

    /// Jump to the start of a bar (debugging / previews / cuts). Voices keep ringing out.
    pub fn seek_bar(&mut self, bar: u32) {
        self.tick = Tick::from_bar(bar).0;
        self.until_tick = 0.0;
        self.pad_root = None;
    }

    fn audible(&self, t: Track) -> bool {
        if !self.solo.is_empty() {
            return self.solo.contains(t);
        }
        !self.mute.contains(t)
    }

    fn apply_automation(&mut self, a: Automation) {
        self.lead_filter.target = a.lead_filter;
        self.bass_filter.target = a.bass_filter;
        self.highpass.target = a.highpass;
        self.fx_send.target = a.fx_send;
        self.riser_level.target = a.riser;
        self.riser_progress = a.riser_progress;
    }

    fn on_bar(&mut self, bar: u32) {
        self.plan = self.dna.plan.bar(bar);
        for t in Track::ALL {
            let on = (self.plan.active.contains(t) || ONE_SHOTS.contains(&t)) && self.audible(t);
            self.channels[t.index()].set_on(on);
        }

        // Pad follows the progression root, one chord per root change.
        if self.plan.active.contains(Track::Pad) {
            let root = fold_root(self.dna.progression.root_at_bar(bar));
            if self.pad_root != Some(root) {
                let mut buf = [0.0f32; 4];
                let n = self.chord_freqs(
                    root,
                    self.dna.sound.pad.chord,
                    self.bass_base + 24,
                    &mut buf,
                );
                self.pad.set_chord(&buf[..n]);
                self.pad_root = Some(root);
            }
        } else if self.pad_root.is_some() {
            self.pad.release_all();
            self.pad_root = None;
        }

        if self.plan.active.contains(Track::Drone) {
            let root = fold_root(self.dna.progression.root_at_bar(bar));
            let midi = self.bass_base + 12 + self.dna.key.degree_to_semitone(root);
            self.drone.play(midi_to_hz(midi as f32));
        } else {
            self.drone.release();
        }
    }

    fn chord_freqs(
        &self,
        root: i32,
        shape: ChordShape,
        base_midi: i32,
        out: &mut [f32; 4],
    ) -> usize {
        let degs = shape.degrees();
        for (i, d) in degs.iter().enumerate().take(4) {
            let midi = base_midi + self.dna.key.degree_to_semitone(root + d);
            out[i] = midi_to_hz(midi as f32);
        }
        degs.len().min(4)
    }

    fn note_freq(&self, base_midi: i32, root: i32, n: &Note) -> f32 {
        midi_to_hz((base_midi + self.dna.key.degree_to_semitone(root + n.degree as i32)) as f32)
    }

    fn on_step(&mut self, bar: u32, step: u32) {
        let plan = self.plan;
        let a = plan.active;
        let d = self.dna.drums;
        let abs = bar as u64 * 16 + step as u64;
        let abs32 = abs as u32;
        let root = fold_root(self.dna.progression.root_at_bar(bar));
        let in_fill = step >= plan.fill.starts_at();
        let rolling = plan.roll_every > 0 || (plan.fill == Fill::Roll && in_fill);

        // --- one-shot FX
        if step == 0 {
            if plan.crash {
                self.crash.trigger(0.9);
            }
            if plan.impact {
                self.impact.trigger();
            }
        }

        // --- drums
        if a.contains(Track::Kick) && d.kick.hit(step) {
            self.kick.trigger(1.0);
            self.duck.trigger();
        }
        if a.contains(Track::HatClosed)
            && d.hat_closed.hit(step)
            && !(in_fill && plan.fill == Fill::Roll)
        {
            let accent = d.hat_accent.hit(step);
            let v = if accent { 0.95 } else { 0.62 } + humanize(abs, 1);
            self.hats.trigger_closed(v);
        }
        if a.contains(Track::HatOpen) && d.hat_open.hit(step) {
            self.hats.trigger_open(0.8 + humanize(abs, 2));
        }
        if a.contains(Track::Clap) && !(in_fill && plan.fill == Fill::Roll) {
            if d.clap.hit(step) {
                match d.backbeat {
                    Backbeat::Clap => self.clap.trigger(1.0),
                    Backbeat::Snare => self.snare.trigger(0.95),
                    Backbeat::ClapSnare => {
                        self.clap.trigger(0.9);
                        self.snare.trigger(0.7);
                    }
                }
            } else if d.clap_ghost.hit(step) {
                self.clap.trigger(0.45);
            }
        }
        if a.contains(Track::Ride) && d.ride.hit(step) {
            self.ride.trigger(0.8 + humanize(abs, 3));
        }

        if rolling {
            let every = if plan.roll_every > 0 {
                plan.roll_every as u32
            } else {
                1
            };
            if step % every == 0 {
                let within = step as f32 / 16.0;
                let k = if plan.roll_every > 0 {
                    plan.build_progress + within / plan.section_bars.max(1) as f32
                } else {
                    (step - Fill::Roll.starts_at()) as f32 / 7.0
                };
                self.snare.trigger(0.3 + 0.7 * k.clamp(0.0, 1.0));
            }
        }

        if plan.fill == Fill::Small && in_fill {
            // Short tom run: fifth, third, root.
            let f = self.tom_base;
            match step {
                12 => self.tom.trigger(f * 1.5, 0.8),
                14 => self.tom.trigger(f * 1.2, 0.85),
                15 => self.tom.trigger(f, 0.95),
                _ => {}
            }
        } else {
            if a.contains(Track::Rim) && d.rim.hit(step) {
                self.rim.trigger(0.8 + humanize(abs, 4));
            }
            if a.contains(Track::Tom) && d.tom.hit(step) {
                let f = self.tom_base;
                let f = if step % 8 >= 4 { f * 1.5 } else { f };
                self.tom.trigger(f, 0.7 + humanize(abs, 5));
            }
        }

        // --- bass
        if a.contains(Track::Bass) {
            if let Some(n) = self.dna.bass.at(abs32) {
                let f = self.note_freq(self.bass_base, root, &n);
                let gate = (n.len as f32 * self.step_samples * 0.7) as u32;
                self.bass.trigger(f, n.velocity, n.accent, gate);
            }
        }

        // --- stab
        if a.contains(Track::Stab) {
            if let Some(n) = self.dna.stab.at(abs32) {
                let mut buf = [0.0f32; 4];
                let k = self.chord_freqs(
                    root,
                    self.dna.sound.stab.chord,
                    self.bass_base + 24,
                    &mut buf,
                );
                let k = k.min(self.dna.sound.stab.notes.max(1) as usize);
                self.stab.trigger_chord(&buf[..k], n.velocity);
            }
        }

        // --- lead
        if a.contains(Track::Lead) {
            let v = plan.variation;
            let note = self
                .dna
                .lead
                .at(abs32)
                .filter(|_| !(v.lead_thin && step % 2 == 1));
            if let Some(n) = note {
                let octave = if v.lead_up { 12 } else { 0 };
                let f = self.note_freq(self.bass_base + 24 + octave, root, &n);
                let len = n.len as f32 * self.step_samples;
                let hold = match self.dna.sound.lead.kind {
                    LeadKind::Acid => 0.55,
                    LeadKind::Pluck => 0.4,
                    LeadKind::Hoover | LeadKind::Supersaw => 0.85,
                };
                let gate = if n.slide { len * 1.1 } else { len * hold } as u32;
                self.lead
                    .trigger(f, n.velocity, n.accent, gate, self.prev_lead_slide);
                self.prev_lead_slide = n.slide;
            } else {
                self.prev_lead_slide = false;
            }
        }
    }

    fn on_arp(&mut self, bar: u32, step: u32) {
        if !self.plan.active.contains(Track::Arp) {
            return;
        }
        let abs = bar * 16 + step;
        if let Some(n) = self.dna.arp.at(abs) {
            let root = fold_root(self.dna.progression.root_at_bar(bar));
            let f = self.note_freq(self.bass_base + 36, root, &n);
            self.arp.trigger_chord(&[f], n.velocity);
        }
    }

    fn on_tick(&mut self) {
        let t = Tick(self.tick);
        let step = t.step_in_bar();
        let tis = t.tick_in_step();
        if step == 0 && tis == 0 {
            self.on_bar(t.bar());
        }
        if tis == 0 {
            let pos = self.tick as f32 / TICKS_PER_BAR as f32;
            let a = self.dna.plan.automation(pos);
            self.apply_automation(a);
        }
        let trigger_at = if step % 2 == 1 { self.swing_ticks } else { 0 };
        if tis == trigger_at {
            self.on_step(t.bar(), step);
            self.on_arp(t.bar(), step);
        }
    }

    /// Render one stereo sample.
    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        while self.until_tick <= 0.0 {
            self.on_tick();
            self.tick += 1;
            self.until_tick += self.spt;
        }
        self.until_tick -= 1.0;

        let duck = self.duck.next();
        self.lead.filter_open = self.lead_filter.next();
        self.bass.filter_mul = self.bass_filter.next();
        let fx_send = self.fx_send.next();
        let riser_level = self.riser_level.next();

        let kick = self.kick.process();
        let rumble = self.rumble.process(kick);
        let (hc, ho) = self.hats.process();
        let (pad_l, pad_r) = self.pad.process();

        let mono: [(Track, f32); 17] = [
            (Track::Arp, self.arp.process()),
            (Track::Drone, self.drone.process()),
            (Track::Kick, kick),
            (Track::Rumble, rumble),
            (Track::Bass, self.bass.process()),
            (Track::HatClosed, hc),
            (Track::HatOpen, ho),
            (Track::Clap, self.clap.process()),
            (Track::Snare, self.snare.process()),
            (Track::Rim, self.rim.process()),
            (Track::Tom, self.tom.process()),
            (Track::Ride, self.ride.process()),
            (Track::Crash, self.crash.process()),
            (Track::Stab, self.stab.process()),
            (Track::Lead, self.lead.process()),
            (
                Track::Riser,
                self.riser.process(riser_level, self.riser_progress),
            ),
            (Track::Impact, self.impact.process()),
        ];

        let mut sum = ChannelOut::default();
        let mut add = |o: ChannelOut| {
            sum.l += o.l;
            sum.r += o.r;
            sum.delay += o.delay;
            sum.reverb += o.reverb;
        };
        for (t, x) in mono {
            add(self.channels[t.index()].process(x, duck));
        }
        add(self.channels[Track::Pad.index()].process_stereo(pad_l, pad_r, duck));

        let (dl, dr) = self.delay.process(sum.delay * fx_send);
        let (rl, rr) = self.reverb.process(sum.reverb * fx_send);
        // Effect returns are ducked too, so the kick always cuts through.
        let fx_duck = 1.0 - 0.5 * duck;
        let mut l = sum.l + (dl + rl) * fx_duck;
        let mut r = sum.r + (dr + rr) * fx_duck;

        // Build high-pass sweep (bypassed when fully open).
        let hp = self.highpass.next();
        if hp > 1e-3 {
            let cutoff = 20.0 * (hp * 20f32.ln()).exp(); // 20 Hz .. 400 Hz
            self.hp_l.set(cutoff, 0.15, self.sr);
            self.hp_r.set(cutoff, 0.15, self.sr);
            l = self.hp_l.hp(l);
            r = self.hp_r.hp(r);
        }
        (l, r)
    }
}
