//! Weighted interval Markov chain over scale degrees.

use crate::profile::Weighted;
use crate::rng::Rng;

/// Pick the next degree from `current` using interval weights, staying within [lo, hi].
/// `home_pull` (0..1) adds extra weight to moves that head back towards degree 0.
pub fn next_degree(
    rng: &mut Rng,
    current: i32,
    steps: &Weighted<i32>,
    lo: i32,
    hi: i32,
    home_pull: f32,
) -> i32 {
    let mut cand = [0i32; 32];
    let mut w = [0f32; 32];
    let mut n = 0;
    for (iv, weight) in steps.values.iter().zip(steps.weights.iter()) {
        if n == cand.len() {
            break;
        }
        let d = current + iv;
        if d < lo || d > hi || *weight <= 0.0 {
            continue;
        }
        let towards_home = (d.abs() < current.abs()) as i32 as f32;
        cand[n] = d;
        w[n] = weight * (1.0 + home_pull * 2.0 * towards_home);
        n += 1;
    }
    if n == 0 {
        return current.clamp(lo, hi);
    }
    cand[rng.weighted_index(&w[..n])]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_in_range() {
        let steps = Weighted {
            values: vec![0, 1, -1, 2, -2, 7, -7],
            weights: vec![1.0; 7],
        };
        let mut rng = Rng::new(3);
        let mut d = 0;
        for _ in 0..10_000 {
            d = next_degree(&mut rng, d, &steps, -3, 9, 0.2);
            assert!((-3..=9).contains(&d));
        }
    }
}
