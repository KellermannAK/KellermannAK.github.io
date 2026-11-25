//! Song titles and artist names.
//!
//! Titles: words from a library (`profiles/words.toml`) combined by simple patterns,
//! with a suffix that fits the song ("(Dub)" for sparse drone tracks, "(Tool)" without
//! melody).
//!
//! Artists: every song has a style fingerprint (kick style, main synth and lead kind, long
//! layer). Each fingerprint owns a small, fixed pool of artists; songs with the same
//! fingerprint usually get the pool's regular artist, sometimes another member. With
//! `new_artist_prob` a one-off artist is invented for the song instead, and occasionally
//! two artists collaborate. No memory between songs is needed: a seed always gets the same
//! name, in a live set, a mix or on its own.
//!
//! Naming never affects the music: it only reads the DNA and draws from its own streams.

use serde::{Deserialize, Serialize};

use crate::dna::SongDna;
use crate::profile::{InsertKind, LeadStyle};
use crate::rng::{label_hash, Rng};
use crate::tracks::Track;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamingRules {
    /// Probability that a song gets a brand-new, one-off artist.
    pub new_artist_prob: f32,
    /// Weights of the artists in a fingerprint's pool (first = the regular).
    pub pool_weights: Vec<f32>,
    /// Probability of a collaboration ("A & B").
    pub collab_prob: f32,
    /// Probability of a fitting suffix such as "(Dub)" or "(Tool)".
    pub suffix_prob: f32,
}

#[derive(Debug, Deserialize)]
struct TitleWords {
    nouns: Vec<String>,
    adjectives: Vec<String>,
    gerunds: Vec<String>,
    plurals: Vec<String>,
    german: Vec<String>,
    numbers: Vec<String>,
    roman: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ArtistWords {
    starts: Vec<String>,
    middles: Vec<String>,
    ends: Vec<String>,
    suffixes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Words {
    title: TitleWords,
    artist: ArtistWords,
}

const WORDS_TOML: &str = include_str!("../../../profiles/words.toml");

fn words() -> &'static Words {
    use std::sync::OnceLock;
    static W: OnceLock<Words> = OnceLock::new();
    W.get_or_init(|| toml::from_str(WORDS_TOML).expect("embedded word library must parse"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SongName {
    pub title: String,
    pub artist: String,
}

impl SongName {
    pub fn display(&self) -> String {
        format!("{} – {}", self.artist, self.title)
    }
}

fn pick<'a>(rng: &mut Rng, list: &'a [String]) -> &'a str {
    &list[rng.range_i32(0, list.len() as i32 - 1) as usize]
}

/// Style fingerprint: the choices that shape a song's character most.
pub fn fingerprint(dna: &SongDna) -> u64 {
    let p = &dna.plan;
    let lead = if p.main_synth == Track::Lead {
        format!("{:?}", dna.sound.lead.kind)
    } else {
        String::new()
    };
    let layer = [Track::Pad, Track::Drone]
        .into_iter()
        .find(|t| p.palette.contains(*t))
        .map(|t| t.name())
        .unwrap_or("none");
    let key = format!(
        "{:?}|{}|{}|{}",
        dna.sound.kick.style,
        p.main_synth.name(),
        lead,
        layer
    );
    label_hash(&key)
}

fn invented(rng: &mut Rng) -> String {
    let w = &words().artist;
    let mut s = String::new();
    s.push_str(pick(rng, &w.starts));
    s.push_str(pick(rng, &w.middles));
    s.push_str(pick(rng, &w.ends));
    // Very short names ("Ram") look like typos: lengthen them with a middle syllable.
    if s.chars().count() < 5 {
        let end = s.split_off(s.len() - 1);
        let mut mid = pick(rng, &w.middles);
        while mid.is_empty() {
            mid = pick(rng, &w.middles);
        }
        s.push_str(mid);
        s.push_str(&end);
    }
    s
}

fn artist_name(rng: &mut Rng) -> String {
    let t = &words().title;
    let w = &words().artist;
    match rng.weighted_index(&[4.0, 2.0, 1.5, 1.0, 1.5]) {
        0 => invented(rng),
        1 => format!("{} {}", pick(rng, &t.adjectives), pick(rng, &t.nouns)),
        2 => format!("{} {}", invented(rng), pick(rng, &w.suffixes)),
        3 => {
            let n = rng.range_i32(2, 3);
            (0..n)
                .map(|_| (b'A' + rng.range_i32(0, 25) as u8) as char)
                .map(|c| format!("{c}."))
                .collect()
        }
        _ => format!("{} {}", invented(rng), invented(rng)),
    }
}

fn title(dna: &SongDna, rules: &NamingRules, rng: &mut Rng) -> String {
    let t = &words().title;
    let mut s = match rng.weighted_index(&[4.0, 1.5, 1.0, 0.8, 0.8, 0.7, 0.5, 0.7]) {
        0 => format!("{} {}", pick(rng, &t.adjectives), pick(rng, &t.nouns)),
        1 => pick(rng, &t.nouns).to_string(),
        2 => format!("{} {}", pick(rng, &t.nouns), pick(rng, &t.numbers)),
        3 => {
            let a = pick(rng, &t.nouns);
            let mut b = pick(rng, &t.nouns);
            if a == b {
                b = pick(rng, &t.nouns);
            }
            format!("{a} of {b}")
        }
        4 => format!("{} {}", pick(rng, &t.gerunds), pick(rng, &t.plurals)),
        5 => pick(rng, &t.german).to_string(),
        6 => format!("{} / {}", pick(rng, &t.nouns), pick(rng, &t.nouns)),
        _ => format!(
            "{} {} {}",
            pick(rng, &t.adjectives),
            pick(rng, &t.nouns),
            pick(rng, &t.roman)
        ),
    };
    // A suffix only where it says something true about the song.
    if rng.chance(rules.suffix_prob) {
        let p = &dna.plan;
        let melodic =
            p.main_synth == Track::Lead && !matches!(dna.sound.lead.style, LeadStyle::Pulse);
        let drone = p.palette.contains(Track::Drone);
        let suffix = if drone && dna.sound.lead.style != LeadStyle::Riff {
            Some("(Dub)")
        } else if !melodic && !p.palette.contains(Track::Pad) {
            Some("(Tool)")
        } else if dna.sound.fx.synth.kind == InsertKind::Crusher {
            Some("(Raw Mix)")
        } else {
            None
        };
        if let Some(x) = suffix {
            s.push(' ');
            s.push_str(x);
        }
    }
    s
}

pub fn name(dna: &SongDna, rules: &NamingRules) -> SongName {
    let base = Rng::new(dna.seed);
    let mut r = base.fork("artist");
    let fp = fingerprint(dna);
    let pool_artist = |idx: usize| {
        artist_name(&mut Rng::new(
            fp ^ label_hash("pool").wrapping_mul(idx as u64 + 1),
        ))
    };
    let main = if r.chance(rules.new_artist_prob) {
        artist_name(&mut base.fork("new-artist"))
    } else {
        pool_artist(r.weighted_index(&rules.pool_weights))
    };
    let artist = if r.chance(rules.collab_prob) {
        // Collaborations come from the same pool: artists of the same style work together.
        let mut other = pool_artist(r.weighted_index(&rules.pool_weights));
        if other == main {
            other = pool_artist(rules.pool_weights.len());
        }
        format!("{main} & {other}")
    } else {
        main
    };
    SongName {
        title: title(dna, rules, &mut base.fork("title")),
        artist,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn deterministic_and_non_empty() {
        let p = Profile::hard_techno_simple();
        for seed in 0..200 {
            let dna = SongDna::generate(seed, &p);
            let a = name(&dna, &p.naming);
            assert_eq!(a, name(&dna, &p.naming));
            assert!(!a.title.trim().is_empty() && !a.artist.trim().is_empty());
        }
    }

    #[test]
    fn similar_songs_share_artists_and_new_ones_appear() {
        let p = Profile::hard_techno_simple();
        let mut by_fp: HashMap<u64, Vec<String>> = HashMap::new();
        let mut all = Vec::new();
        for seed in 0..600 {
            let dna = SongDna::generate(seed, &p);
            let n = name(&dna, &p.naming);
            by_fp
                .entry(fingerprint(&dna))
                .or_default()
                .push(n.artist.clone());
            all.push(n.artist);
        }
        // Within a fingerprint the regular artist recurs.
        let big = by_fp.values().filter(|v| v.len() >= 10).collect::<Vec<_>>();
        assert!(!big.is_empty());
        for v in big {
            let mut counts: HashMap<&String, usize> = HashMap::new();
            for a in v {
                *counts.entry(a).or_default() += 1;
            }
            let top = counts.values().max().unwrap();
            assert!(*top * 4 >= v.len(), "regular artist should be common");
        }
        // But there are many distinct artists overall (one-offs).
        let distinct: HashSet<_> = all.iter().collect();
        assert!(distinct.len() > 100, "{}", distinct.len());
    }
}
