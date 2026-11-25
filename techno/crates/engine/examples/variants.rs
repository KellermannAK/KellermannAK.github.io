//! Level check for instrument sound variants: renders each variant soloed in the same
//! song section and prints its RMS relative to variant 0.
//!   cargo run --release -p techno-engine --example variants

use techno_core::dna::SongDna;
use techno_core::structure::SectionKind;
use techno_core::{Profile, Track, TrackSet};
use techno_engine::Deck;

fn rms_db(dna: SongDna, track: Track, start_bar: u32) -> f32 {
    let sr = 22050.0;
    let mut d = Deck::new(dna, sr);
    d.solo = TrackSet::of(&[track]);
    d.seek_bar(start_bar);
    let n = (sr * 12.0) as usize;
    let mut sum = 0.0f64;
    for _ in 0..n {
        let (l, r) = d.process();
        sum += (l as f64).powi(2) + (r as f64).powi(2);
    }
    (10.0 * (sum / (2 * n) as f64 + 1e-12).log10()) as f32
}

fn main() {
    let p = Profile::hard_techno_simple();
    type Setter = fn(&mut SongDna, u8);
    let cases: [(&str, Track, u8, Setter, SectionKind); 13] = [
        (
            "clap",
            Track::Clap,
            4,
            |d, v| d.sound.clap.variant = v,
            SectionKind::Drop,
        ),
        (
            "snare",
            Track::Snare,
            4,
            |d, v| d.sound.snare.variant = v,
            SectionKind::Build,
        ),
        (
            "hats",
            Track::HatClosed,
            4,
            |d, v| d.sound.hats.variant = v,
            SectionKind::Drop,
        ),
        (
            "ride",
            Track::Ride,
            3,
            |d, v| d.sound.ride.variant = v,
            SectionKind::Drop,
        ),
        (
            "crash",
            Track::Crash,
            3,
            |d, v| d.sound.crash_variant = v,
            SectionKind::Drop,
        ),
        (
            "rim",
            Track::Rim,
            3,
            |d, v| d.sound.rim.variant = v,
            SectionKind::Drop,
        ),
        (
            "tom",
            Track::Tom,
            3,
            |d, v| d.sound.tom.variant = v,
            SectionKind::Drop,
        ),
        (
            "bass",
            Track::Bass,
            4,
            |d, v| d.sound.bass.variant = v,
            SectionKind::Drop,
        ),
        (
            "rumble",
            Track::Rumble,
            3,
            |d, v| d.sound.rumble.variant = v,
            SectionKind::Drop,
        ),
        (
            "stab",
            Track::Stab,
            4,
            |d, v| d.sound.stab.variant = v,
            SectionKind::Drop,
        ),
        (
            "arp",
            Track::Arp,
            4,
            |d, v| d.sound.arp.variant = v,
            SectionKind::Breakdown,
        ),
        (
            "pad",
            Track::Pad,
            4,
            |d, v| d.sound.pad.variant = v,
            SectionKind::Breakdown,
        ),
        (
            "drone",
            Track::Drone,
            3,
            |d, v| d.sound.drone.variant = v,
            SectionKind::Breakdown,
        ),
    ];
    for (name, track, n, set, section) in cases {
        // Find a song that actually plays this track in the wanted section.
        let mut found = None;
        for seed in 0..400u64 {
            let dna = SongDna::generate(seed, &p);
            if let Some(s) = dna.plan.sections().iter().find(|s| s.kind == section) {
                let bar = s.start + s.bars / 2 - 2;
                if dna.plan.bar(bar).active.contains(track)
                    || matches!(track, Track::Crash | Track::Snare | Track::Tom)
                {
                    found = Some((dna, if track == Track::Crash { s.start } else { bar }));
                    break;
                }
            }
        }
        let Some((dna, bar)) = found else {
            println!("{name:<7} no song found");
            continue;
        };
        let mut levels = Vec::new();
        for v in 0..n {
            let mut d = dna;
            set(&mut d, v);
            levels.push(rms_db(d, track, bar));
        }
        let rel: Vec<String> = levels
            .iter()
            .map(|l| format!("{:+5.1}", l - levels[0]))
            .collect();
        println!(
            "{name:<7} v0 {:6.1} dB   relative: {}",
            levels[0],
            rel.join("  ")
        );
    }
}
