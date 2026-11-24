//! Instrument tracks of a deck.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Track {
    Kick,
    Rumble,
    Bass,
    HatClosed,
    HatOpen,
    Clap,
    Snare,
    Rim,
    Tom,
    Ride,
    Crash,
    Stab,
    Lead,
    Pad,
}

impl Track {
    pub const COUNT: usize = 14;
    pub const ALL: [Track; Track::COUNT] = [
        Track::Kick,
        Track::Rumble,
        Track::Bass,
        Track::HatClosed,
        Track::HatOpen,
        Track::Clap,
        Track::Snare,
        Track::Rim,
        Track::Tom,
        Track::Ride,
        Track::Crash,
        Track::Stab,
        Track::Lead,
        Track::Pad,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn bit(self) -> u16 {
        1 << (self as u16)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Track::Kick => "kick",
            Track::Rumble => "rumble",
            Track::Bass => "bass",
            Track::HatClosed => "hat_closed",
            Track::HatOpen => "hat_open",
            Track::Clap => "clap",
            Track::Snare => "snare",
            Track::Rim => "rim",
            Track::Tom => "tom",
            Track::Ride => "ride",
            Track::Crash => "crash",
            Track::Stab => "stab",
            Track::Lead => "lead",
            Track::Pad => "pad",
        }
    }

    pub fn from_name(name: &str) -> Option<Track> {
        Track::ALL.iter().copied().find(|t| t.name() == name)
    }
}

/// Set of tracks as a bitmask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrackSet(pub u16);

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
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}
