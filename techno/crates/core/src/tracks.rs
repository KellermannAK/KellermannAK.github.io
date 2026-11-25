//! Instrument tracks of a deck.

macro_rules! tracks {
    ($($variant:ident => $name:literal),* $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u8)]
        pub enum Track { $($variant),* }

        impl Track {
            pub const ALL: [Track; Track::COUNT] = [$(Track::$variant),*];

            pub const fn name(self) -> &'static str {
                match self { $(Track::$variant => $name),* }
            }
        }
    };
}

tracks! {
    Kick => "kick",
    Rumble => "rumble",
    Bass => "bass",
    HatClosed => "hat_closed",
    HatOpen => "hat_open",
    Clap => "clap",
    Snare => "snare",
    Rim => "rim",
    Tom => "tom",
    Ride => "ride",
    Crash => "crash",
    Stab => "stab",
    Lead => "lead",
    Pad => "pad",
    Arp => "arp",
    Drone => "drone",
    Riser => "riser",
    Impact => "impact",
    Piano => "piano",
    Guitar => "guitar",
}

impl Track {
    pub const COUNT: usize = 20;

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn bit(self) -> u32 {
        1 << (self as u32)
    }

    pub fn from_name(name: &str) -> Option<Track> {
        Track::ALL.iter().copied().find(|t| t.name() == name)
    }
}

/// Set of tracks as a bitmask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackSet(pub u32);

impl TrackSet {
    pub const NONE: TrackSet = TrackSet(0);
    pub const ALL: TrackSet = TrackSet((1 << Track::COUNT) - 1);

    pub fn of(tracks: &[Track]) -> TrackSet {
        TrackSet(tracks.iter().fold(0, |m, t| m | t.bit()))
    }
    pub fn contains(self, t: Track) -> bool {
        self.0 & t.bit() != 0
    }
    pub fn with(self, t: Track) -> TrackSet {
        TrackSet(self.0 | t.bit())
    }
    pub fn without(self, t: Track) -> TrackSet {
        TrackSet(self.0 & !t.bit())
    }
    pub fn union(self, o: TrackSet) -> TrackSet {
        TrackSet(self.0 | o.0)
    }
    pub fn and(self, o: TrackSet) -> TrackSet {
        TrackSet(self.0 & o.0)
    }
    pub fn minus(self, o: TrackSet) -> TrackSet {
        TrackSet(self.0 & !o.0)
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn iter(self) -> impl Iterator<Item = Track> {
        Track::ALL.into_iter().filter(move |t| self.contains(*t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_roundtrip() {
        assert_eq!(Track::ALL.len(), Track::COUNT);
        for t in Track::ALL {
            assert_eq!(Track::from_name(t.name()), Some(t));
            assert_eq!(Track::ALL[t.index()], t);
        }
    }
}
