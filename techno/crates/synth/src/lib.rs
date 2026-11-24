//! techno-synth: real-time synthesis. DSP primitives, instruments and mixer.
//!
//! Rule: nothing in a `process` method allocates, locks or does I/O.

pub mod dsp;
pub mod instruments;
pub mod mixer;
