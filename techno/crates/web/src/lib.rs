//! WebAssembly interface for the AudioWorklet.
//!
//! Plain C ABI so the worklet can call it without generated glue:
//!
//! ```text
//! e = techno_new(seed_lo, seed_hi, sampleRate)   // endless set from a 64-bit seed
//! n = techno_render(e, 128)                      // renders into internal L/R buffers
//! techno_left(e) / techno_right(e)               // -> pointers to n f32 samples each
//! techno_status(e)                               // -> pointer to STATUS_LEN f64 values
//! techno_set_gain(e, 0.8)
//! techno_free(e)
//! ```
//!
//! All allocation happens in `techno_new`; rendering only allocates briefly when the
//! next song's DNA is generated (small vectors, once per song). Audio buffers of decks
//! are recycled, never reallocated.

use techno_core::profile::Profile;
use techno_core::structure::SectionKind;
use techno_core::theory::ScaleKind;
use techno_engine::{Set, TransitionKind};

pub const MAX_FRAMES: usize = 4096;
pub const STATUS_LEN: usize = 16;

/// Status array layout (all f64):
/// 0 song index, 1 song seed, 2 key root (0 = C), 3 scale (0 minor, 1 phrygian,
/// 2 harmonic minor), 4 Camelot number, 5 BPM, 6 section (0 intro, 1 groove, 2 build,
/// 3 breakdown, 4 drop, 5 outro, -1 none), 7 bar, 8 bar in section, 9 section length,
/// 10 song length in bars, 11 energy 0..1, 12 planned transition (-1 none, 0 cut,
/// 1 fast blend, 2 slow blend, 3 filter blend), 13 bars until it starts,
/// 14 transition running (0/1), 15 seconds since start.
pub struct WebEngine {
    set: Set,
    left: Vec<f32>,
    right: Vec<f32>,
    status: [f64; STATUS_LEN],
    gain: f32,
}

impl WebEngine {
    pub fn new(seed: u64, sample_rate: f32) -> WebEngine {
        WebEngine {
            set: Set::new(seed, Profile::hard_techno_simple(), u32::MAX, sample_rate),
            left: vec![0.0; MAX_FRAMES],
            right: vec![0.0; MAX_FRAMES],
            status: [0.0; STATUS_LEN],
            gain: 1.0,
        }
    }

    pub fn render(&mut self, frames: usize) -> usize {
        let n = frames.min(MAX_FRAMES);
        for i in 0..n {
            let (l, r) = self.set.process();
            self.left[i] = l * self.gain;
            self.right[i] = r * self.gain;
        }
        n
    }

    pub fn update_status(&mut self) -> &[f64; STATUS_LEN] {
        let s = self.set.status();
        let key = s.key.unwrap_or(techno_core::theory::Key {
            root: 0,
            scale: ScaleKind::NaturalMinor,
        });
        let scale = match key.scale {
            ScaleKind::NaturalMinor => 0.0,
            ScaleKind::Phrygian => 1.0,
            ScaleKind::HarmonicMinor => 2.0,
        };
        let section = match s.section {
            Some(SectionKind::Intro) => 0.0,
            Some(SectionKind::Groove) => 1.0,
            Some(SectionKind::Build) => 2.0,
            Some(SectionKind::Breakdown) => 3.0,
            Some(SectionKind::Drop) => 4.0,
            Some(SectionKind::Outro) => 5.0,
            None => -1.0,
        };
        let transition = match s.transition {
            Some(TransitionKind::Cut) => 0.0,
            Some(TransitionKind::FastBlend) => 1.0,
            Some(TransitionKind::SlowBlend) => 2.0,
            Some(TransitionKind::FilterBlend) => 3.0,
            None => -1.0,
        };
        self.status = [
            s.song_index as f64,
            s.seed as f64,
            key.root as f64,
            scale,
            key.camelot().0 as f64,
            s.bpm as f64,
            section,
            s.bar as f64,
            s.section_bar as f64,
            s.section_bars as f64,
            s.total_bars as f64,
            s.energy as f64,
            transition,
            s.bars_to_transition as f64,
            s.in_transition as u8 as f64,
            s.seconds,
        ];
        &self.status
    }
}

/// # Safety
/// Returns an owned pointer; free it with `techno_free`.
#[no_mangle]
pub extern "C" fn techno_new(seed_lo: u32, seed_hi: u32, sample_rate: f32) -> *mut WebEngine {
    let seed = (seed_hi as u64) << 32 | seed_lo as u64;
    Box::into_raw(Box::new(WebEngine::new(seed, sample_rate)))
}

/// # Safety
/// `e` must come from `techno_new` and not be freed.
#[no_mangle]
pub unsafe extern "C" fn techno_render(e: *mut WebEngine, frames: u32) -> u32 {
    (*e).render(frames as usize) as u32
}

/// # Safety
/// `e` must be valid.
#[no_mangle]
pub unsafe extern "C" fn techno_left(e: *const WebEngine) -> *const f32 {
    (*e).left.as_ptr()
}

/// # Safety
/// `e` must be valid.
#[no_mangle]
pub unsafe extern "C" fn techno_right(e: *const WebEngine) -> *const f32 {
    (*e).right.as_ptr()
}

/// # Safety
/// `e` must be valid.
#[no_mangle]
pub unsafe extern "C" fn techno_status(e: *mut WebEngine) -> *const f64 {
    (*e).update_status().as_ptr()
}

/// # Safety
/// `e` must be valid.
#[no_mangle]
pub unsafe extern "C" fn techno_set_gain(e: *mut WebEngine, gain: f32) {
    (*e).gain = gain.clamp(0.0, 1.0);
}

/// # Safety
/// `e` must come from `techno_new`; it is invalid afterwards.
#[no_mangle]
pub unsafe extern "C" fn techno_free(e: *mut WebEngine) {
    if !e.is_null() {
        drop(Box::from_raw(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_and_reports() {
        let mut e = WebEngine::new(5, 48000.0);
        for _ in 0..400 {
            e.render(128);
        }
        assert!(e.left[..128].iter().any(|x| *x != 0.0));
        let s = *e.update_status();
        assert_eq!(s[0], 0.0); // first song
        assert!(s[5] >= 120.0); // bpm
        assert!(s[6] >= 0.0); // has a section
    }
}
