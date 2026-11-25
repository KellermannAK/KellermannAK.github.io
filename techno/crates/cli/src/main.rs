//! `techno` command line: offline rendering of songs and DJ sets, song inspection.
//!
//!   techno mix    --seed 7 --songs 5 --out set.wav [--tracklist set.json]
//!   techno info   --seed 42
//!   techno render --seed 42 --out song.wav [--bars N] [--start-bar 0] [--sr 44100]
//!                 [--profile profiles/x.toml] [--solo bass,lead] [--mute rumble]

mod analyze;

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use techno_core::time::seconds_per_bar;
use techno_core::{Profile, SongDna, Track, TrackSet};
use techno_engine::{Engine, Set};

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
    songs: u32,
    tracklist: Option<PathBuf>,
    input: Option<PathBuf>,
    no_fx: bool,
}

const USAGE: &str = "usage:
  techno analyze --input FEEDBACK.txt  (patterns in exported listening feedback; - = stdin)
  techno version                       (engine version and profile id as JSON)
  techno mix    --seed N [--songs N] [--out FILE.wav] [--tracklist FILE.json] [--sr HZ]
  techno info   --seed N [--profile FILE]
  techno render --seed N [--out FILE.wav] [--bars N] [--start-bar N] [--sr HZ]
                [--profile FILE] [--solo t1,t2] [--mute t1,t2] [--no-fx]
tracks: kick rumble bass hat_closed hat_open clap snare rim tom ride crash stab lead pad
        arp drone riser impact";

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
        songs: 4,
        tracklist: None,
        input: None,
        no_fx: false,
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
            "--songs" => a.songs = val()?.parse().map_err(|e| format!("--songs: {e}"))?,
            "--tracklist" => a.tracklist = Some(PathBuf::from(val()?)),
            "--input" => a.input = Some(PathBuf::from(val()?)),
            "--no-fx" => a.no_fx = true,
            "-h" | "--help" => return Err(String::new()),
            other => return Err(format!("unknown flag '{other}'")),
        }
    }
    if a.out.as_os_str().is_empty() {
        let prefix = if a.cmd == "mix" { "mix" } else { "seed" };
        a.out = PathBuf::from(format!("{prefix}-{}.wav", a.seed));
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

/// Streaming 16-bit PCM stereo WAV writer with deterministic TPDF dither.
struct WavWriter {
    w: BufWriter<File>,
    samples: u64,
    rng: techno_core::Rng,
}

impl WavWriter {
    fn create(path: &PathBuf, sr: u32) -> Result<WavWriter, String> {
        let open = || -> std::io::Result<BufWriter<File>> {
            let mut w = BufWriter::new(File::create(path)?);
            w.write_all(b"RIFF")?;
            w.write_all(&0u32.to_le_bytes())?; // patched in finish()
            w.write_all(b"WAVEfmt ")?;
            w.write_all(&16u32.to_le_bytes())?;
            w.write_all(&1u16.to_le_bytes())?; // PCM
            w.write_all(&2u16.to_le_bytes())?; // stereo
            w.write_all(&sr.to_le_bytes())?;
            w.write_all(&(sr * 4).to_le_bytes())?;
            w.write_all(&4u16.to_le_bytes())?;
            w.write_all(&16u16.to_le_bytes())?;
            w.write_all(b"data")?;
            w.write_all(&0u32.to_le_bytes())?; // patched in finish()
            Ok(w)
        };
        let w = open().map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(WavWriter {
            w,
            samples: 0,
            rng: techno_core::Rng::new(0xD17E),
        })
    }

    fn write(&mut self, interleaved: &[f32]) -> Result<(), String> {
        for &x in interleaved {
            let dither = (self.rng.next_f32() - self.rng.next_f32()) / 32768.0;
            let s = ((x + dither).clamp(-1.0, 1.0) * 32767.0).round() as i16;
            self.w
                .write_all(&s.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        self.samples += interleaved.len() as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        let data_len = (self.samples * 2) as u32;
        let mut patch = || -> std::io::Result<()> {
            self.w.seek(SeekFrom::Start(4))?;
            self.w.write_all(&(36 + data_len).to_le_bytes())?;
            self.w.seek(SeekFrom::Start(40))?;
            self.w.write_all(&data_len.to_le_bytes())?;
            self.w.flush()
        };
        patch().map_err(|e| e.to_string())
    }
}

/// Peak / RMS meter over everything written.
#[derive(Default)]
struct Meter {
    peak: f32,
    sum_sq: f64,
    n: u64,
}

impl Meter {
    fn add(&mut self, buf: &[f32]) {
        for &x in buf {
            self.peak = self.peak.max(x.abs());
            self.sum_sq += (x as f64) * (x as f64);
        }
        self.n += buf.len() as u64;
    }

    fn report(&self) -> String {
        let rms = (self.sum_sq / self.n.max(1) as f64).sqrt();
        format!(
            "peak {:.2} dBFS, rms {:.1} dBFS",
            20.0 * self.peak.max(1e-9).log10(),
            20.0 * rms.max(1e-9).log10()
        )
    }
}

fn mmss(secs: f64) -> String {
    format!("{}:{:02}", (secs / 60.0) as u32, secs as u32 % 60)
}

fn json_str(s: &str) -> String {
    let escaped = s
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn cmd_info(a: &Args, profile: &Profile) {
    let dna = SongDna::generate(a.seed, profile);
    println!("profile    {} ({})", profile.name, profile.id);
    println!("song       {}", dna.name(profile).display());
    println!("{}", dna.describe());
    println!("\nstructure:\n{}", dna.plan.describe(dna.bpm));
}

fn cmd_render(a: &Args, profile: &Profile) -> Result<(), String> {
    let dna = SongDna::generate(a.seed, profile);
    let mut e = Engine::new(dna, a.sr as f32);
    e.deck.solo = a.solo;
    e.deck.mute = a.mute;
    e.deck.bypass_fx = a.no_fx;
    if a.start_bar > 0 {
        e.deck.seek_bar(a.start_bar);
    }
    // Default: the whole song plus one bar of tails.
    let bars = if a.bars == 0 {
        dna.plan.total_bars.saturating_sub(a.start_bar) + 1
    } else {
        a.bars
    };
    let secs = seconds_per_bar(dna.bpm as f64) * bars as f64;
    let mut left = (secs * a.sr as f64) as usize;

    let mut wav = WavWriter::create(&a.out, a.sr)?;
    let mut meter = Meter::default();
    let mut buf = vec![0.0f32; 4096 * 2];
    let t0 = Instant::now();
    while left > 0 {
        let n = left.min(4096);
        e.render_interleaved(&mut buf[..n * 2]);
        meter.add(&buf[..n * 2]);
        wav.write(&buf[..n * 2])?;
        left -= n;
    }
    let took = t0.elapsed().as_secs_f64();
    wav.finish()?;

    println!(
        "seed {}  {}  {} BPM  {}",
        a.seed,
        dna.key.name(),
        dna.bpm,
        dna.key.camelot_code()
    );
    println!(
        "rendered {} bars ({}) in {:.2}s = {:.0}x realtime",
        bars,
        mmss(secs),
        took,
        secs / took
    );
    println!("{} -> {}", meter.report(), a.out.display());
    Ok(())
}

fn cmd_mix(a: &Args, profile: &Profile) -> Result<(), String> {
    let mut set = Set::new(a.seed, profile.clone(), a.songs, a.sr as f32);
    let mut wav = WavWriter::create(&a.out, a.sr)?;
    let mut meter = Meter::default();
    let mut buf = vec![0.0f32; 4096 * 2];
    let t0 = Instant::now();
    let mut frames = 0u64;
    while !set.finished() {
        let n = set.render_interleaved(&mut buf);
        meter.add(&buf[..n * 2]);
        wav.write(&buf[..n * 2])?;
        frames += n as u64;
    }
    wav.finish()?;
    let secs = frames as f64 / a.sr as f64;
    let took = t0.elapsed().as_secs_f64();

    println!(
        "set {}  {} songs  {} BPM  {}",
        a.seed,
        set.log.len(),
        set.bpm(),
        mmss(secs)
    );
    for e in &set.log {
        let how = e.transition.map(|t| t.name()).unwrap_or("start");
        println!(
            "  {:>6}  {:<12}  {} – {}  [seed {}, {} ({})]",
            mmss(e.start_seconds),
            how,
            e.artist,
            e.title,
            e.seed,
            e.key,
            e.camelot
        );
    }
    println!("rendered in {:.1}s = {:.0}x realtime", took, secs / took);
    println!("{} -> {}", meter.report(), a.out.display());

    if let Some(path) = &a.tracklist {
        let songs: Vec<String> = set
            .log
            .iter()
            .map(|e| {
                format!(
                    "{{\"index\":{},\"seed\":{},\"title\":{},\"artist\":{},\"key\":{},\"camelot\":{},\"bpm\":{},\"start\":{:.2},\"transition\":{},\"structure\":{}}}",
                    e.index,
                    e.seed,
                    json_str(&e.title),
                    json_str(&e.artist),
                    json_str(&e.key),
                    json_str(&e.camelot),
                    e.bpm,
                    e.start_seconds,
                    e.transition.map(|t| json_str(t.name())).unwrap_or_else(|| "null".into()),
                    json_str(&e.structure)
                )
            })
            .collect();
        let json = format!(
            "{{\"set_seed\":{},\"bpm\":{},\"duration\":{:.2},\"songs\":[{}]}}\n",
            a.seed,
            set.bpm(),
            secs,
            songs.join(",")
        );
        std::fs::write(path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn cmd_analyze(a: &Args, profile: &Profile) -> Result<(), String> {
    use std::io::Read;
    let text = match &a.input {
        Some(p) if p.as_os_str() != "-" => {
            std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?
        }
        _ => {
            let mut s = String::new();
            std::io::stdin()
                .read_to_string(&mut s)
                .map_err(|e| e.to_string())?;
            s
        }
    };
    let build = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    print!("{}", analyze::run(&text, profile, build.as_deref()));
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
        "version" => {
            println!(
                "{{\"engine\":\"{}\",\"profile\":\"{}\"}}",
                techno_core::ENGINE_VERSION,
                profile.id
            );
            Ok(())
        }
        "render" => cmd_render(&args, &profile),
        "mix" => cmd_mix(&args, &profile),
        "analyze" => cmd_analyze(&args, &profile),
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
