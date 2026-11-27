//! techno-engine: decks, sequencing and the master bus.
//!
//! Platform-neutral: the same `Engine` is driven by the CLI (offline WAV), and later by
//! the WASM AudioWorklet and a native cpal host.

pub mod deck;
pub mod set;

pub use deck::Deck;
pub use set::{Blend, DeckView, PlaylistSong, Set, SetEntry, SetStatus, TransitionKind};
use techno_core::{Profile, SongDna};
use techno_synth::mixer::MasterBus;

pub struct Engine {
    pub sample_rate: f32,
    pub deck: Deck,
    master: MasterBus,
}

impl Engine {
    pub fn new(dna: SongDna, sample_rate: f32) -> Engine {
        Engine {
            sample_rate,
            deck: Deck::new(dna, sample_rate),
            master: MasterBus::new(sample_rate),
        }
    }

    pub fn from_seed(seed: u64, profile: &Profile, sample_rate: f32) -> Engine {
        Engine::new(SongDna::generate(seed, profile), sample_rate)
    }

    #[inline]
    pub fn process(&mut self) -> (f32, f32) {
        let (l, r) = self.deck.process();
        self.master.process(l, r)
    }

    /// Fill an interleaved stereo buffer.
    pub fn render_interleaved(&mut self, out: &mut [f32]) {
        for frame in out.chunks_exact_mut(2) {
            let (l, r) = self.process();
            frame[0] = l;
            frame[1] = r;
        }
    }

    /// Fill separate channel buffers (AudioWorklet layout).
    pub fn render_planar(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (a, b) = self.process();
            *l = a;
            *r = b;
        }
    }

    pub fn master_compression_db(&self) -> f32 {
        self.master.compression_db()
    }

    pub fn master_limiter_db(&self) -> f32 {
        self.master.limiter_db()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use techno_core::time::seconds_per_bar;

    fn render(seed: u64, bars: u32, sr: f32) -> Vec<f32> {
        let p = Profile::hard_techno_simple();
        let mut e = Engine::from_seed(seed, &p, sr);
        let n = (seconds_per_bar(e.deck.dna.bpm as f64) * bars as f64 * sr as f64) as usize;
        let mut buf = vec![0.0; n * 2];
        e.render_interleaved(&mut buf);
        buf
    }

    fn hash(buf: &[f32]) -> u64 {
        buf.iter().fold(0xCBF2_9CE4_8422_2325u64, |h, x| {
            (h ^ x.to_bits() as u64).wrapping_mul(0x100_0000_01B3)
        })
    }

    #[test]
    fn bit_identical_render() {
        let a = render(77, 4, 22050.0);
        let b = render(77, 4, 22050.0);
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn recycled_deck_is_bit_identical() {
        let p = Profile::hard_techno_simple();
        let sr = 22050.0;
        let dna = SongDna::generate(42, &p);
        let render = |mut d: Deck| {
            d.seek_bar(30);
            (0..(sr as usize * 8))
                .map(|_| d.process())
                .fold(0u64, |h, (l, r)| {
                    (h ^ l.to_bits() as u64 ^ ((r.to_bits() as u64) << 32))
                        .wrapping_mul(0x100_0000_01B3)
                })
        };
        // A used deck: play a different song for a while, then recycle it.
        let mut used = Deck::new(SongDna::generate(7, &p), sr);
        used.seek_bar(60);
        for _ in 0..(sr as usize * 4) {
            used.process();
        }
        assert_eq!(render(Deck::new(dna, sr)), render(used.recycle(dna)));
    }

    #[test]
    fn no_clipping_and_not_silent() {
        let mut e = Engine::from_seed(5, &Profile::hard_techno_simple(), 22050.0);
        e.deck.seek_bar(80); // drop: everything playing
        let mut buf = vec![0.0; 22050 * 2 * 6];
        e.render_interleaved(&mut buf);
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak <= 1.0, "peak {peak}");
        assert!(peak > 0.3, "too quiet: {peak}");
        assert!(buf.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn faster_than_realtime() {
        // Release-mode target is >= 10x; tests run with opt-level 2, so be lenient here.
        let sr = 44100.0;
        let mut e = Engine::from_seed(9, &Profile::hard_techno_simple(), sr);
        e.deck.seek_bar(80);
        let secs = 4.0;
        let mut buf = vec![0.0; (sr * secs) as usize * 2];
        let t0 = std::time::Instant::now();
        e.render_interleaved(&mut buf);
        let factor = secs as f64 / t0.elapsed().as_secs_f64();
        assert!(factor > 3.0, "realtime factor {factor:.1}");
    }
}
