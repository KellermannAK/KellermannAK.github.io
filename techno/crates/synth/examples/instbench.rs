//! Rough per-instrument CPU cost: 60 s of audio with a typical pattern.
use std::time::Instant;
use techno_core::{Profile, SongDna};
use techno_synth::instruments::*;

fn main() {
    let sr = 48000.0;
    let p = Profile::house();
    let dna = SongDna::generate(1, &p);
    let n = (sr * 60.0) as usize;
    let step = (sr * 60.0 / 122.0 / 4.0) as usize;
    let chord = [220.0, 261.6, 329.6, 392.0];

    let mut piano = Piano::new(dna.sound.piano, sr);
    let t = Instant::now();
    let mut acc = 0.0;
    for i in 0..n {
        if i % (step * 3) == 0 {
            piano.trigger_chord(&chord, 0.8, (step * 2) as u32, 200);
        }
        acc += piano.process();
    }
    println!(
        "piano  {:.1} ms per 60 s ({acc:.1})",
        t.elapsed().as_secs_f64() * 1e3
    );

    let mut g = Guitar::new(dna.sound.guitar, sr);
    let t = Instant::now();
    for i in 0..n {
        if i % step == 0 {
            g.trigger_chord(&chord[..1], 0.8, step as u32, 0, true, true);
        }
        acc += g.process();
    }
    println!(
        "guitar {:.1} ms per 60 s ({acc:.1})",
        t.elapsed().as_secs_f64() * 1e3
    );

    let mut s = Stab::new(dna.sound.stab, sr);
    let t = Instant::now();
    for i in 0..n {
        if i % (step * 3) == 0 {
            s.trigger_chord(&chord, 0.8);
        }
        acc += s.process();
    }
    println!(
        "stab   {:.1} ms per 60 s ({acc:.1})",
        t.elapsed().as_secs_f64() * 1e3
    );
    let mut pad = Pad::new(dna.sound.pad, sr);
    pad.set_chord(&chord);
    let t = Instant::now();
    for _ in 0..n {
        let (l, r) = pad.process();
        acc += l + r;
    }
    println!(
        "pad    {:.1} ms per 60 s ({acc:.1})",
        t.elapsed().as_secs_f64() * 1e3
    );
}
