//! An endless DJ-style set: two decks on one clock, songs blended into each other.
//!
//! The running song dictates the tempo; the next song's key is a Camelot neighbour.
//! Transitions start and end on phrase boundaries, and the low end (kick, bass, rumble)
//! of the two decks is never mixed: it is swapped hard at one phrase boundary.

use techno_core::dna::Constraints;
use techno_core::structure::SectionKind;
use techno_core::time::seconds_per_bar;
use techno_core::{Profile, Rng, SongDna};
use techno_synth::dsp::{Smoother, Svf};
use techno_synth::mixer::MasterBus;

use crate::deck::Deck;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    /// Switch within one bar at a phrase boundary, straight into the new song's groove.
    Cut,
    /// 8-16 bars.
    FastBlend,
    /// 32-64 bars.
    SlowBlend,
    /// Old song high-passed out, new song low-passed in.
    FilterBlend,
}

impl TransitionKind {
    pub const fn name(self) -> &'static str {
        match self {
            TransitionKind::Cut => "cut",
            TransitionKind::FastBlend => "fast blend",
            TransitionKind::SlowBlend => "slow blend",
            TransitionKind::FilterBlend => "filter blend",
        }
    }
}

/// A planned transition out of the current song.
#[derive(Clone, Copy, Debug)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Bar of the outgoing song at which the incoming song starts.
    pub out_bar: u32,
    /// Bar of the incoming song it starts at (skips part of its intro, or all of it for cuts).
    pub in_bar: u32,
    /// Length of the overlap in bars.
    pub bars: u32,
    /// Bars after the start at which the low end is swapped.
    pub swap_after: u32,
}

/// One song as it appears in the set.
#[derive(Clone, Debug)]
pub struct SetEntry {
    pub index: u32,
    pub seed: u64,
    pub key: String,
    pub camelot: String,
    pub bpm: f32,
    /// Seconds into the set where the song becomes audible.
    pub start_seconds: f64,
    /// How the song was brought in (None for the first).
    pub transition: Option<TransitionKind>,
    pub structure: String,
}

/// DJ channel strip: fader, low-end kill and a high-/low-pass filter.
struct DeckStrip {
    gain: Smoother,
    lows: Smoother,
    hp: Smoother,
    lp: Smoother,
    split: [Svf; 2],
    hpf: [Svf; 2],
    lpf: [Svf; 2],
    sr: f32,
}

impl DeckStrip {
    fn new(sr: f32, gain: f32, lows: f32) -> DeckStrip {
        DeckStrip {
            gain: Smoother::new(gain, 0.05, sr),
            lows: Smoother::new(lows, 0.02, sr),
            hp: Smoother::new(0.0, 0.1, sr),
            lp: Smoother::new(0.0, 0.1, sr),
            split: [Svf::new(140.0, 0.0, sr), Svf::new(140.0, 0.0, sr)],
            hpf: [Svf::new(20.0, 0.2, sr), Svf::new(20.0, 0.2, sr)],
            lpf: [Svf::new(20000.0, 0.2, sr), Svf::new(20000.0, 0.2, sr)],
            sr,
        }
    }

    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let g = self.gain.next();
        let lows = self.lows.next();
        let hp = self.hp.next();
        let lp = self.lp.next();
        let mut x = [l, r];
        for (i, v) in x.iter_mut().enumerate() {
            // Low kill: crossfade between full signal and its high band.
            let high = self.split[i].hp(*v);
            *v = high + lows * (*v - high);
            if hp > 1e-3 {
                let fc = 20.0 * (hp * 75f32.ln()).exp(); // 20 Hz .. 1.5 kHz
                self.hpf[i].set(fc, 0.25, self.sr);
                *v = self.hpf[i].hp(*v);
            }
            if lp > 1e-3 {
                let fc = 20000.0 * (-lp * 100f32.ln()).exp(); // 20 kHz .. 200 Hz
                self.lpf[i].set(fc, 0.25, self.sr);
                *v = self.lpf[i].lp(*v);
            }
            *v *= g;
        }
        (x[0], x[1])
    }
}

struct Slot {
    deck: Deck,
    strip: DeckStrip,
}

pub struct Set {
    sr: f32,
    profile: Profile,
    rng: Rng,
    slots: [Option<Slot>; 2],
    /// Slot of the song that owns the set (the outgoing one during a transition).
    current: usize,
    plan: Option<Transition>,
    /// Pre-generated next song (created well before it is needed).
    next: Option<Deck>,
    next_index: u32,
    max_songs: u32,
    /// Bar of the outgoing song when the transition started, and the frames per bar.
    frame: u64,
    transition_start_frame: Option<u64>,
    frames_per_bar: f64,
    master: MasterBus,
    pub log: Vec<SetEntry>,
    done: bool,
}

impl Set {
    pub fn new(set_seed: u64, profile: Profile, max_songs: u32, sample_rate: f32) -> Set {
        let mut rng = Rng::new(set_seed);
        let first_seed = rng.next_u64() % 1_000_000;
        let dna = SongDna::generate(first_seed, &profile);
        let frames_per_bar = seconds_per_bar(dna.bpm as f64) * sample_rate as f64;
        let mut set = Set {
            sr: sample_rate,
            profile,
            rng,
            slots: [None, None],
            current: 0,
            plan: None,
            next: None,
            next_index: 1,
            max_songs: max_songs.max(1),
            frame: 0,
            transition_start_frame: None,
            frames_per_bar,
            master: MasterBus::new(sample_rate),
            log: Vec::new(),
            done: false,
        };
        set.log.push(entry(0, &dna, 0.0, None));
        set.slots[0] = Some(Slot {
            deck: Deck::new(dna, sample_rate),
            strip: DeckStrip::new(sample_rate, 1.0, 1.0),
        });
        set.plan_transition();
        set
    }

    pub fn finished(&self) -> bool {
        self.done
    }

    pub fn bpm(&self) -> f32 {
        self.slots[self.current]
            .as_ref()
            .map(|s| s.deck.dna.bpm)
            .unwrap_or(0.0)
    }

    fn plan_transition(&mut self) {
        if self.next_index >= self.max_songs {
            self.plan = None;
            return;
        }
        let deck = &self.slots[self.current].as_ref().unwrap().deck;
        let plan = deck.song_plan();
        let end = plan.total_bars;
        let outro = plan.outro_start();
        let kinds = [
            TransitionKind::Cut,
            TransitionKind::FastBlend,
            TransitionKind::SlowBlend,
            TransitionKind::FilterBlend,
        ];
        let kind = kinds[self.rng.weighted_index(&[1.0, 2.0, 2.5, 2.0])];
        // Intro length of the incoming song is not known yet; profiles use 32-bar intros.
        let intro = self.profile.structure.intro_bars;
        let t = match kind {
            TransitionKind::Cut => Transition {
                kind,
                out_bar: outro,
                in_bar: intro,
                bars: 1,
                swap_after: 0,
            },
            TransitionKind::FastBlend => {
                let bars = if self.rng.chance(0.5) { 8 } else { 16 };
                Transition {
                    kind,
                    out_bar: end - bars,
                    in_bar: intro - bars,
                    bars,
                    swap_after: bars / 2,
                }
            }
            TransitionKind::SlowBlend => {
                let bars = if self.rng.chance(0.6) { 32 } else { 64 };
                let out_bar = end.saturating_sub(bars).max(outro.saturating_sub(32));
                let bars = end - out_bar;
                Transition {
                    kind,
                    out_bar,
                    in_bar: 0,
                    bars,
                    swap_after: (bars / 2 / 8) * 8,
                }
            }
            TransitionKind::FilterBlend => {
                let bars = if self.rng.chance(0.5) { 16 } else { 32 };
                Transition {
                    kind,
                    out_bar: end - bars,
                    in_bar: intro - bars.min(intro),
                    bars,
                    swap_after: bars / 2,
                }
            }
        };
        self.plan = Some(t);
    }

    fn prepare_next(&mut self) {
        let cur = &self.slots[self.current].as_ref().unwrap().deck.dna;
        let step = [0, 1, -1][self.rng.weighted_index(&[2.0, 3.0, 3.0])];
        let key = cur.key.camelot_step(step);
        let seed = self.rng.next_u64() % 1_000_000;
        let c = Constraints {
            bpm: Some(cur.bpm),
            key_root: Some(key.root),
        };
        let dna = SongDna::generate_with(seed, &self.profile, c);
        self.next = Some(Deck::new(dna, self.sr));
    }

    fn start_transition(&mut self, t: Transition) {
        let mut deck = self.next.take().expect("next song prepared");
        // Cuts land on the incoming song's first non-intro section.
        let in_bar = if t.kind == TransitionKind::Cut {
            deck.song_plan()
                .sections()
                .iter()
                .find(|s| s.kind != SectionKind::Intro)
                .map(|s| s.start)
                .unwrap_or(t.in_bar)
        } else {
            t.in_bar
        };
        if in_bar > 0 {
            deck.seek_bar(in_bar);
        }
        let (gain, lows) = match t.kind {
            TransitionKind::Cut => (1.0, 1.0),
            _ => (0.0, 0.0),
        };
        let mut strip = DeckStrip::new(self.sr, gain, lows);
        if t.kind == TransitionKind::FilterBlend {
            strip.lp = Smoother::new(1.0, 0.1, self.sr);
        }
        let other = 1 - self.current;
        let seconds = self.frame as f64 / self.sr as f64;
        self.log
            .push(entry(self.next_index, &deck.dna, seconds, Some(t.kind)));
        self.next_index += 1;
        self.slots[other] = Some(Slot { deck, strip });
        self.transition_start_frame = Some(self.frame);
        if t.kind == TransitionKind::Cut {
            let out = self.slots[self.current].as_mut().unwrap();
            out.strip.lows.target = 0.0;
            out.strip.gain.target = 0.0;
            out.strip.hp.target = 0.6;
        }
    }

    /// Fader / filter automation during a transition. `x` = bars since its start.
    fn automate(&mut self, t: Transition, x: f64) {
        let half = (t.bars as f64 / 2.0).max(1.0);
        let swapped = x >= t.swap_after as f64;
        let fade_in = (x / half).clamp(0.0, 1.0) as f32;
        let fade_out = (1.0 - (x - half) / half).clamp(0.0, 1.0) as f32;
        let [s0, s1] = &mut self.slots;
        let (out_s, in_s) = if self.current == 0 {
            (s0, s1)
        } else {
            (s1, s0)
        };
        let (Some(out_s), Some(in_s)) = (out_s.as_mut(), in_s.as_mut()) else {
            return;
        };
        match t.kind {
            TransitionKind::Cut => {}
            TransitionKind::FastBlend | TransitionKind::SlowBlend => {
                // Equal-power-ish curves.
                in_s.strip.gain.target = (fade_in * core::f32::consts::FRAC_PI_2).sin();
                out_s.strip.gain.target = (fade_out * core::f32::consts::FRAC_PI_2).sin();
            }
            TransitionKind::FilterBlend => {
                in_s.strip.gain.target = (fade_in * 3.0).min(1.0);
                in_s.strip.lp.target = 1.0 - fade_in;
                let p = (x / t.bars as f64).clamp(0.0, 1.0) as f32;
                out_s.strip.hp.target = p * 0.95;
                out_s.strip.gain.target = (fade_out * 2.0).min(1.0);
            }
        }
        in_s.strip.lows.target = if swapped { 1.0 } else { 0.0 };
        out_s.strip.lows.target = if swapped { 0.0 } else { 1.0 };
    }

    /// Render one stereo sample of the set.
    pub fn process(&mut self) -> (f32, f32) {
        if self.done {
            return (0.0, 0.0);
        }
        let cur = self.current;

        // Bar-synchronous decisions happen exactly on the outgoing deck's bar starts.
        if let Some(bar) = self.slots[cur].as_ref().unwrap().deck.bar_starts_now() {
            if let Some(t) = self.plan {
                if self.next.is_none()
                    && self.transition_start_frame.is_none()
                    && bar + 8 >= t.out_bar
                {
                    self.prepare_next();
                }
                if bar == t.out_bar && self.transition_start_frame.is_none() {
                    self.start_transition(t);
                }
                if let Some(start) = self.transition_start_frame {
                    let x = (self.frame - start) as f64 / self.frames_per_bar;
                    if x >= (t.bars + 1) as f64 || (t.kind == TransitionKind::Cut && x >= 2.0) {
                        self.finish_transition();
                    }
                }
            }
        }
        if let (Some(t), Some(start)) = (self.plan, self.transition_start_frame) {
            if self.frame % 64 == 0 {
                let x = (self.frame - start) as f64 / self.frames_per_bar;
                self.automate(t, x);
            }
        }

        let (mut l, mut r) = (0.0, 0.0);
        for slot in self.slots.iter_mut().flatten() {
            let (a, b) = slot.deck.process();
            let (a, b) = slot.strip.process(a, b);
            l += a;
            r += b;
        }
        self.frame += 1;

        // Last song: stop after its tails.
        if self.plan.is_none() && self.slots[self.current].as_ref().unwrap().deck.finished() {
            self.done = true;
        }
        self.master.process(l, r)
    }

    fn finish_transition(&mut self) {
        let old = self.current;
        self.slots[old] = None;
        self.current = 1 - old;
        self.transition_start_frame = None;
        if let Some(s) = self.slots[self.current].as_mut() {
            s.strip.gain.target = 1.0;
            s.strip.lows.target = 1.0;
            s.strip.lp.target = 0.0;
            s.strip.hp.target = 0.0;
        }
        self.plan_transition();
    }

    /// Render into an interleaved stereo buffer; returns frames written (less at the end).
    pub fn render_interleaved(&mut self, out: &mut [f32]) -> usize {
        let mut n = 0;
        for frame in out.chunks_exact_mut(2) {
            if self.done {
                break;
            }
            let (l, r) = self.process();
            frame[0] = l;
            frame[1] = r;
            n += 1;
        }
        n
    }
}

fn entry(
    index: u32,
    dna: &SongDna,
    start_seconds: f64,
    transition: Option<TransitionKind>,
) -> SetEntry {
    SetEntry {
        index,
        seed: dna.seed,
        key: dna.key.name(),
        camelot: dna.key.camelot_code(),
        bpm: dna.bpm,
        start_seconds,
        transition,
        structure: dna.plan.describe(dna.bpm),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_plays_songs_in_order_and_ends() {
        let sr = 8000.0; // low rate keeps the test fast
        let mut set = Set::new(3, Profile::hard_techno_simple(), 3, sr);
        let mut buf = vec![0.0f32; 8000 * 2];
        let mut frames = 0usize;
        let mut peak = 0.0f32;
        while !set.finished() {
            let n = set.render_interleaved(&mut buf);
            frames += n;
            peak = buf[..n * 2].iter().fold(peak, |m, x| m.max(x.abs()));
            assert!(frames < 8000 * 60 * 30, "set never ends");
        }
        assert_eq!(set.log.len(), 3);
        assert!(peak <= 1.0 && peak > 0.2);
        for w in set.log.windows(2) {
            assert!(w[1].start_seconds > w[0].start_seconds);
            assert_eq!(w[0].bpm, w[1].bpm, "tempo is global");
        }
    }

    #[test]
    fn keys_move_on_camelot_wheel() {
        let mut set = Set::new(11, Profile::hard_techno_simple(), 4, 8000.0);
        let mut buf = vec![0.0f32; 8000 * 2];
        while !set.finished() {
            set.render_interleaved(&mut buf);
        }
        let num = |c: &str| c.trim_end_matches('A').parse::<i32>().unwrap();
        for w in set.log.windows(2) {
            let d = (num(&w[1].camelot) - num(&w[0].camelot)).rem_euclid(12);
            assert!(
                d == 0 || d == 1 || d == 11,
                "{} -> {}",
                w[0].camelot,
                w[1].camelot
            );
        }
    }
}
