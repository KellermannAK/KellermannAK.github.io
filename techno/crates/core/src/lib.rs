//! techno-core: deterministic, audio-free music generation.
//!
//! Time base, seeded PRNG, music theory, euclidean rhythms, Markov melodies,
//! style profiles and the song DNA. Nothing here touches audio or the system clock.

pub mod dna;
pub mod euclid;
pub mod markov;
pub mod naming;
pub mod profile;
pub mod rng;
pub mod structure;
pub mod theory;
pub mod time;
pub mod tracks;

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub use dna::SongDna;
pub use profile::Profile;
pub use rng::Rng;
pub use tracks::{Track, TrackSet};
