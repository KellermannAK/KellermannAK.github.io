//! `techno analyze --input feedback.txt`: turn exported listening feedback into patterns.
//!
//! Every entry names a song seed (and, for the live set, the bar). The song is rebuilt
//! from its seed, and the features at that moment are collected: song-level choices
//! (sounds, synths, scale, structure) and bar-level state (section, its style, which tracks
//! were playing). Features are then counted for 👍 vs 👎. A 👎 weighs only half as much as
//! a 👍 (complaining is quicker than praising). 💡 ideas are neutral: listed, never counted.
//!
//! Seeds only mean the same song for the same engine build: run this at the build the
//! feedback was given with (`git checkout <build>`); entries from other builds are flagged.

use std::collections::BTreeMap;

use techno_core::structure::{SectionKind, SHORT_SYNTHS};
use techno_core::{Profile, SongDna, Track, TrackSet};

/// Weight of a thumbs down relative to a thumbs up in the score.
pub const DOWN_WEIGHT: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rating {
    Up,
    Down,
    Idea,
}

impl Rating {
    fn icon(self) -> &'static str {
        match self {
            Rating::Up => "👍",
            Rating::Down => "👎",
            Rating::Idea => "💡",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub rating: Rating,
    pub song_seed: u64,
    /// 1-based bar in the song, if known.
    pub bar: Option<u32>,
    pub build: Option<String>,
    pub comment: String,
}

/// Parse the text produced by the site's "Kopieren" / "Teilen" button.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('„') || t.starts_with('"') {
            if let Some(last) = out.last_mut() {
                last.comment = t
                    .trim_matches(|c| c == '„' || c == '“' || c == '"')
                    .to_string();
            }
            continue;
        }
        if !t.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            continue;
        }
        let rating = if t.contains('👍') {
            Rating::Up
        } else if t.contains('👎') {
            Rating::Down
        } else if t.contains('💡') {
            Rating::Idea
        } else {
            continue;
        };
        let Some(song_seed) = number_after(t, "(Seed ") else {
            continue;
        };
        out.push(Entry {
            rating,
            song_seed,
            bar: number_after(t, "Song-Takt ").map(|b| b as u32),
            build: word_after(t, "Build "),
            comment: String::new(),
        });
    }
    out
}

fn number_after(s: &str, pat: &str) -> Option<u64> {
    let i = s.find(pat)? + pat.len();
    let digits: String = s[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn word_after(s: &str, pat: &str) -> Option<String> {
    let i = s.find(pat)? + pat.len();
    let w: String = s[i..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect();
    (!w.is_empty()).then_some(w)
}

/// Feature name -> value for one entry.
fn features(dna: &SongDna, bar: Option<u32>) -> Vec<(String, String)> {
    let mut f: Vec<(String, String)> = Vec::new();
    let mut add = |k: &str, v: String| f.push((k.to_string(), v));
    let p = &dna.plan;
    add("scale", dna.key.scale.name().to_string());
    add("roots", format!("{} root(s)", dna.progression.count));
    add("kick", format!("{:?}", dna.sound.kick.style).to_lowercase());
    add(
        "backbeat",
        format!("{:?}", dna.drums.backbeat).to_lowercase(),
    );
    add("hats", format!("{:?}", dna.drums.hat_style).to_lowercase());
    add("open hat", (dna.drums.hat_open.count() > 0).to_string());
    add("main synth", p.main_synth.name().to_string());
    add(
        "breakdown synth",
        p.alt_synth.map(|t| t.name()).unwrap_or("same").to_string(),
    );
    let long = [Track::Pad, Track::Drone]
        .into_iter()
        .find(|t| p.palette.contains(*t));
    add(
        "long layer",
        long.map(|t| t.name()).unwrap_or("none").to_string(),
    );
    if p.main_synth == Track::Lead || p.alt_synth == Some(Track::Lead) {
        add(
            "lead kind",
            format!("{:?}", dna.sound.lead.kind).to_lowercase(),
        );
    }
    let kinds: Vec<&str> = p.sections().iter().map(|s| s.kind.name()).collect();
    add("has breakdown", kinds.contains(&"breakdown").to_string());
    add(
        "drops",
        kinds.iter().filter(|k| **k == "drop").count().to_string(),
    );
    add("structure", kinds.join(" > "));

    if let Some(b) = bar {
        let b0 = b.saturating_sub(1);
        let plan = p.bar(b0);
        let style = p.styles[p.section_index_at(b0)];
        add("section", plan.section.name().to_string());
        let pos = plan.section_bar as f32 / plan.section_bars.max(1) as f32;
        let part = if pos < 0.34 {
            "start"
        } else if pos < 0.67 {
            "middle"
        } else {
            "end"
        };
        add("section part", part.to_string());
        match plan.section {
            SectionKind::Build => {
                add("riser", format!("{:?}", style.riser).to_lowercase());
                add("roll", format!("{:?}", style.roll).to_lowercase());
            }
            SectionKind::Drop => add("drop style", format!("{:?}", style.drop).to_lowercase()),
            _ => {}
        }
        let melodic = plan
            .active
            .and(SHORT_SYNTHS.union(TrackSet::of(&[Track::Pad, Track::Drone])));
        add("melodic layers", melodic.0.count_ones().to_string());
        add("tracks playing", plan.active.0.count_ones().to_string());
        for t in plan.active.iter() {
            add("playing", t.name().to_string());
        }
    }
    f
}

pub fn run(text: &str, profile: &Profile, current_build: Option<&str>) -> String {
    let entries = parse(text);
    if entries.is_empty() {
        return "No feedback entries found (expected the text from the site's Kopieren/Teilen button).\n".into();
    }
    let mut out = String::new();
    let n = |r: Rating| entries.iter().filter(|e| e.rating == r).count();
    out.push_str(&format!(
        "# Feedback analysis: {} entries, {} 👍, {} 👎, {} 💡\n\n",
        entries.len(),
        n(Rating::Up),
        n(Rating::Down),
        n(Rating::Idea)
    ));

    let mismatched = entries
        .iter()
        .filter(|e| match (&e.build, current_build) {
            (Some(b), Some(c)) => !c.starts_with(b.as_str()) && !b.starts_with(c),
            _ => false,
        })
        .count();
    if mismatched > 0 {
        out.push_str(&format!(
            "⚠ {mismatched} entries were given with a different build than the current one; their seeds may sound different now. Run at that build (`git checkout <build>`) for exact results.\n\n"
        ));
    }

    // (feature, value) -> (up, down); ideas are not counted.
    let mut counts: BTreeMap<(String, String), (u32, u32)> = BTreeMap::new();
    out.push_str("## Entries\n\n");
    for (i, e) in entries.iter().enumerate() {
        let dna = SongDna::generate(e.song_seed, profile);
        let fs = features(&dna, e.bar);
        let get = |k: &str| {
            fs.iter()
                .filter(|(n, _)| n == k)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        out.push_str(&format!(
            "{}. {} seed {}{} - {}{}\n   playing: {}\n   sounds: kick {}, {} synth {}, layer {}; structure: {}\n",
            i + 1,
            e.rating.icon(),
            e.song_seed,
            e.bar.map(|b| format!(" bar {b}")).unwrap_or_default(),
            if e.bar.is_some() { format!("{} ({}), ", get("section"), get("section part")) } else { String::new() },
            if e.comment.is_empty() { "no comment".to_string() } else { format!("\"{}\"", e.comment) },
            if e.bar.is_some() { get("playing") } else { "(bar unknown)".into() },
            get("kick"),
            get("main synth"),
            get("lead kind"),
            get("long layer"),
            get("structure"),
        ));
        for (k, v) in fs {
            let c = counts.entry((k, v)).or_default();
            match e.rating {
                Rating::Up => c.0 += 1,
                Rating::Down => c.1 += 1,
                Rating::Idea => {}
            }
        }
    }

    let ideas: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.rating == Rating::Idea)
        .collect();
    if !ideas.is_empty() {
        out.push_str("\n## Ideas (neutral, not counted)\n\n");
        for e in ideas {
            out.push_str(&format!(
                "- seed {}{}: {}\n",
                e.song_seed,
                e.bar.map(|b| format!(" bar {b}")).unwrap_or_default(),
                if e.comment.is_empty() {
                    "(no text)"
                } else {
                    e.comment.as_str()
                }
            ));
        }
    }

    out.push_str(&format!(
        "\n## Patterns (features rated at least twice; score = 👍 - {DOWN_WEIGHT} x 👎)\n\n"
    ));
    out.push_str("| feature | value | 👍 | 👎 | score |\n|---|---|---|---|---|\n");
    let score = |u: u32, d: u32| u as f32 - DOWN_WEIGHT * d as f32;
    let mut rows: Vec<_> = counts
        .into_iter()
        .filter(|(k, (u, d))| u + d >= 2 && k.0 != "structure")
        .collect();
    rows.sort_by(|(ka, (ua, da)), (kb, (ub, db))| {
        score(*ub, *db)
            .total_cmp(&score(*ua, *da))
            .then_with(|| ka.cmp(kb))
    });
    for ((k, v), (u, d)) in rows {
        out.push_str(&format!(
            "| {k} | {v} | {u} | {d} | {:+.1} |\n",
            score(u, d)
        ));
    }
    out.push_str(
        "\nFew entries give weak evidence: treat single differences as hints, not rules.\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Techno-Feedback (3) – Engine 0.2.0 · Profil hard_techno_simple · Build abcdef1

1. 👎 Live-Set 4242 @ 0:02 · Song 1 (Seed 99533) · Intro 2/32 · Song-Takt 2/192 · H-Moll (10A) · Build abcdef1
   „Pinch auf der Offbeat“
2. 👍 Mix 7 @ 3:10 · Song 1 (Seed 742138) · F-Moll (4A)
3. 💡 Live-Set 4242 @ 5:00 · Song 2 (Seed 11) · Drop 1/32 · Song-Takt 80/200
   „Ein Acid-Lead im zweiten Drop wäre cool“
";

    #[test]
    fn parses_export() {
        let e = parse(SAMPLE);
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].rating, Rating::Down);
        assert_eq!(e[1].rating, Rating::Up);
        assert_eq!(e[2].rating, Rating::Idea);
        assert_eq!(e[0].song_seed, 99533);
        assert_eq!(e[0].bar, Some(2));
        assert_eq!(e[0].build.as_deref(), Some("abcdef1"));
        assert_eq!(e[0].comment, "Pinch auf der Offbeat");
        assert_eq!(e[2].comment, "Ein Acid-Lead im zweiten Drop wäre cool");
        assert_eq!(e[1].bar, None);
    }

    #[test]
    fn ideas_are_listed_but_not_counted() {
        let r = run(SAMPLE, &Profile::hard_techno_simple(), Some("abcdef1"));
        assert!(r.contains("3 entries, 1 👍, 1 👎, 1 💡"));
        assert!(r.contains("## Ideas"));
        assert!(r.contains("Acid-Lead"));
        // One 👍 and one 👎 on the same feature: score +0.5, not 0.
        assert!(r.contains("| 1 | 1 | +0.5 |"));
    }
}
