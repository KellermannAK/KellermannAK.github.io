//! Cost of preparing the next song on the audio thread.
use std::time::Instant;
use techno_core::{Profile, SongDna};
use techno_engine::Deck;

fn main() {
    let p = Profile::hard_techno_simple();
    let mut deck = Deck::new(SongDna::generate(1, &p), 48000.0);
    let (mut gen, mut rec, mut fresh) = (0.0f64, 0.0f64, 0.0f64);
    let n = 200;
    let mut worst = 0.0f64;
    for s in 0..n {
        let t0 = Instant::now();
        let dna = SongDna::generate(s, &p);
        let t1 = Instant::now();
        deck = deck.recycle(dna);
        let t2 = Instant::now();
        let d2 = Deck::new(dna, 48000.0);
        let t3 = Instant::now();
        std::hint::black_box(&d2);
        gen += (t1 - t0).as_secs_f64();
        rec += (t2 - t1).as_secs_f64();
        fresh += (t3 - t2).as_secs_f64();
        worst = worst.max((t2 - t0).as_secs_f64());
    }
    let ms = |x: f64| x / n as f64 * 1000.0;
    println!(
        "dna {:.3} ms, recycle {:.3} ms, fresh deck {:.3} ms, worst dna+recycle {:.3} ms",
        ms(gen),
        ms(rec),
        ms(fresh),
        worst * 1000.0
    );
}
