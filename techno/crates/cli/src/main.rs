//! `techno` command line: offline rendering and song inspection.
//!
//!   techno info   --seed 42
//!   techno render --seed 42 --out song.wav [--bars 128] [--start-bar 0] [--sr 44100]
//!                 [--profile profiles/x.toml] [--solo bass,lead] [--mute rumble]

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use techno_core::time::seconds_per_bar;
use techno_core::{Profile, SongDna, Track, TrackSet};
use techno_engine::Engine;

struct Args {
    cmd: String,
    seed: u64,
    bars: u32,
    start_bar: u32,
    sr: u32,
    out: PathBuf,
    profile: Option<PathBuf>,
    solo: TrackSet,
    mute: TrackSet,
}

const USAGE: &str = "usage:
  techno info   --seed N [--profile FILE]
  techno render --seed N [--out FILE.wav] [--bars N] [--start-bar N] [--sr HZ]
                [--profile FILE] [--solo t1,t2] [--mute t1,t2]
tracks: kick rumble bass hat_closed hat_open clap snare rim tom ride crash stab lead pad";

fn parse_tracks(s: &str) -> Result<TrackSet, String> {
    let mut set = TrackSet::NONE;
    for name in s.split(',').filter(|n| !n.is_empty()) {
        set = set.with(Track::from_name(name).ok_or_else(|| format!("unknown track '{name}'"))?);
    }
    Ok(set)
}

fn parse() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or("missing command")?;
    let mut a = Args {
        cmd,
        seed: 1,
        bars: 0,
        start_bar: 0,
        sr: 44100,
        out: PathBuf::new(),
        profile: None,
        solo: TrackSet::NONE,
        mute: TrackSet::NONE,
    };
    while let Some(flag) = it.next() {
        let mut val = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--seed" => a.seed = val()?.parse().map_err(|e| format!("--seed: {e}"))?,
            "--bars" => a.bars = val()?.parse().map_err(|e| format!("--bars: {e}"))?,
            "--start-bar" => {
                a.start_bar = val()?.parse().map_err(|e| format!("--start-bar: {e}"))?
            }
            "--sr" => a.sr = val()?.parse().map_err(|e| format!("--sr: {e}"))?,
            "--out" => a.out = PathBuf::from(val()?),
            "--profile" => a.profile = Some(PathBuf::from(val()?)),
            "--solo" => a.solo = parse_tracks(&val()?)?,
            "--mute" => a.mute = parse_tracks(&val()?)?,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    if a.out.as_os_str().is_empty() {
        a.out = PathBuf::from(format!("seed-{}.wav", a.seed));
    }
    Ok(a)
}

fn load_profile(path: &Option<PathBuf>) -> Result<Profile, String> {
    match path {
        None => Ok(Profile::hard_techno_simple()),
        Some(p) => {
            let src = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
            Profile::from_toml(&src).map_err(|e| format!("{}: {e}", p.display()))
        }
    }
}

/// 16-bit PCM WAV with deterministic TPDF dither.
fn write_wav(path: &PathBuf, sr: u32, interleaved: &[f32]) -> std::io::Result<()> {
    let mut w = BufWriter::new(File::create(path)?);
    let data_len = (interleaved.len() * 2) as u32;
    w.write_all(b"RIFF")?;
    w.write_all(&(36 + data_len).to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&1u16.to_le_bytes())?; // PCM
    w.write_all(&2u16.to_le_bytes())?; // stereo
    w.write_all(&sr.to_le_bytes())?;
    w.write_all(&(sr * 4).to_le_bytes())?;
    w.write_all(&4u16.to_le_bytes())?;
    w.write_all(&16u16.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_len.to_le_bytes())?;
    let mut rng = techno_core::Rng::new(0xD17E);
    for &x in interleaved {
        let dither = (rng.next_f32() - rng.next_f32()) / 32768.0;
        let s = ((x + dither).clamp(-1.0, 1.0) * 32767.0).round() as i16;
        w.write_all(&s.to_le_bytes())?;
    }
    w.flush()
}

fn cmd_info(a: &Args, profile: &Profile) {
    let dna = SongDna::generate(a.seed, profile);
    println!("profile    {} ({})", profile.name, profile.id);
    println!("{}", dna.describe());
    println!("\nstructure:\n{}", dna.plan.describe(dna.bpm));
}

fn cmd_render(a: &Args, profile: &Profile) -> Result<(), String> {
    let dna = SongDna::generate(a.seed, profile);
    let mut e = Engine::new(dna, a.sr as f32);
    e.deck.solo = a.solo;
    e.deck.mute = a.mute;
    if a.start_bar > 0 {
        e.deck.seek_bar(a.start_bar);
    }
    let bars = if a.bars == 0 {
        dna.plan.total_bars.saturating_sub(a.start_bar) + 1
    } else {
        a.bars
    };
    let secs = seconds_per_bar(dna.bpm as f64) * bars as f64;
    let frames = (secs * a.sr as f64) as usize;
    let mut buf = vec![0.0f32; frames * 2];

    let t0 = Instant::now();
    e.render_interleaved(&mut buf);
    let took = t0.elapsed().as_secs_f64();

    let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    let rms =
        (buf.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / buf.len().max(1) as f64).sqrt();
    write_wav(&a.out, a.sr, &buf).map_err(|e| format!("{}: {e}", a.out.display()))?;

    println!(
        "seed {}  {}  {} BPM  {}",
        a.seed,
        dna.key.name(),
        dna.bpm,
        dna.key.camelot_code()
    );
    println!(
        "rendered {} bars ({:.0}:{:04.1}) in {:.2}s = {:.0}x realtime",
        bars,
        (secs / 60.0).floor(),
        secs % 60.0,
        took,
        secs / took
    );
    println!(
        "peak {:.2} dBFS, rms {:.1} dBFS -> {}",
        20.0 * peak.max(1e-9).log10(),
        20.0 * rms.max(1e-9).log10(),
        a.out.display()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse() {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("error: {e}\n");
            }
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let profile = match load_profile(&args.profile) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let res = match args.cmd.as_str() {
        "info" => {
            cmd_info(&args, &profile);
            Ok(())
        }
        "render" => cmd_render(&args, &profile),
        other => Err(format!("unknown command '{other}'\n\n{USAGE}")),
    };
    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
