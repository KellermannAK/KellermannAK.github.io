//! Block-timing benchmark: `cargo run --release -p techno-web --example bench -- 14`
use std::time::Instant;
use techno_web::WebEngine;

fn main() {
    let minutes: f64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(10.0);
    let sr = 48000.0;
    let block = 128;
    let mut e = WebEngine::new(7, sr as f32);
    let blocks = (minutes * 60.0 * sr / block as f64) as usize;
    let budget = block as f64 / sr * 1000.0;
    let mut times = Vec::with_capacity(blocks);
    let mut spikes = Vec::new();
    for i in 0..blocks {
        let t0 = Instant::now();
        e.render(block);
        let dt = t0.elapsed().as_secs_f64() * 1000.0;
        if dt > budget * 0.5 {
            spikes.push((i * block) as f64 / sr);
        }
        times.push(dt);
    }
    let total: f64 = times.iter().sum();
    let worst = times.iter().cloned().fold(0.0, f64::max);
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!(
        "realtime {:.1}x, p99 {:.3} ms, worst {:.2} ms, over half budget: {}",
        minutes * 60000.0 / total,
        times[times.len() * 99 / 100],
        worst,
        spikes.len()
    );
    println!("{:?}", spikes.iter().map(|s| *s as u32).collect::<Vec<_>>());
}
