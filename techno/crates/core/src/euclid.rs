//! Euclidean rhythms: `k` hits spread as evenly as possible over `n` steps.

/// Bitmask pattern; bit `i` set = hit on step `i`. `n` <= 32.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pattern {
    pub mask: u32,
    pub len: u8,
}

impl Pattern {
    pub const fn empty(len: u8) -> Pattern {
        Pattern { mask: 0, len }
    }

    pub const fn from_mask(mask: u32, len: u8) -> Pattern {
        Pattern { mask, len }
    }

    /// Four-on-the-floor in 16ths.
    pub const fn four_on_the_floor() -> Pattern {
        Pattern {
            mask: 0x1111,
            len: 16,
        }
    }

    #[inline]
    pub fn hit(&self, step: u32) -> bool {
        let n = self.len.max(1) as u32;
        self.mask & (1 << (step % n)) != 0
    }

    pub fn count(&self) -> u32 {
        (self.mask & mask_for_len(self.len)).count_ones()
    }

    pub fn without(self, other: Pattern) -> Pattern {
        Pattern {
            mask: self.mask & !other.mask,
            len: self.len,
        }
    }

    pub fn union(self, other: Pattern) -> Pattern {
        Pattern {
            mask: self.mask | other.mask,
            len: self.len,
        }
    }

    pub fn to_string_grid(&self) -> String {
        (0..self.len as u32)
            .map(|i| if self.hit(i) { 'x' } else { '.' })
            .collect()
    }
}

fn mask_for_len(len: u8) -> u32 {
    if len >= 32 {
        u32::MAX
    } else {
        (1u32 << len) - 1
    }
}

/// Euclidean rhythm E(k, n), rotated so that step `i` plays what was step `i + rotation`.
pub fn euclid(k: u32, n: u32, rotation: i32) -> Pattern {
    assert!(n > 0 && n <= 32, "pattern length must be 1..=32");
    let k = k.min(n);
    let mut mask = 0u32;
    for i in 0..n {
        let src = (i as i32 + rotation).rem_euclid(n as i32) as u32;
        if (src * k) % n < k {
            mask |= 1 << i;
        }
    }
    Pattern { mask, len: n as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_patterns() {
        assert_eq!(euclid(3, 8, 0).to_string_grid(), "x..x..x.");
        assert_eq!(euclid(4, 16, 0).to_string_grid(), "x...x...x...x...");
        assert_eq!(euclid(5, 8, 0).count(), 5);
        assert_eq!(euclid(0, 16, 0).count(), 0);
        assert_eq!(euclid(16, 16, 0).count(), 16);
    }

    #[test]
    fn rotation_preserves_count() {
        for k in 0..=16 {
            for r in -16..16 {
                assert_eq!(euclid(k, 16, r).count(), k);
            }
        }
    }

    #[test]
    fn offbeat_hats() {
        // Offbeat 8ths = four on the floor shifted by two 16ths.
        assert_eq!(euclid(4, 16, -2).to_string_grid(), "..x...x...x...x.");
    }
}
