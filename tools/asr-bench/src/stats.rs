//! Word error rate with a bootstrap interval over clips (or recordings).

/// SplitMix64: a small seeded generator, so intervals are reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Errors and reference words for one unit.
#[derive(Debug, Clone, Copy)]
pub struct Unit {
    pub errors: f64,
    pub words: f64,
}

pub fn wer(units: &[Unit]) -> f64 {
    let (e, w) = units.iter().fold((0.0, 0.0), |a, u| (a.0 + u.errors, a.1 + u.words));
    if w == 0.0 {
        0.0
    } else {
        100.0 * e / w
    }
}

/// 2.5th and 97.5th percentiles of `stat` over `reps` resamples of unit indices.
pub fn bootstrap<F: Fn(&[usize]) -> f64>(n: usize, reps: usize, seed: u64, stat: F) -> (f64, f64) {
    if n == 0 {
        return (0.0, 0.0);
    }
    let mut rng = Rng::new(seed);
    let mut idx = vec![0usize; n];
    let mut vals: Vec<f64> = (0..reps)
        .map(|_| {
            for i in idx.iter_mut() {
                *i = rng.below(n);
            }
            stat(&idx)
        })
        .collect();
    vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| vals[((p * (reps - 1) as f64).round() as usize).min(reps - 1)];
    (q(0.025), q(0.975))
}

/// WER interval for `units`.
pub fn wer_interval(units: &[Unit], reps: usize, seed: u64) -> (f64, f64) {
    bootstrap(units.len(), reps, seed, |idx| {
        wer(&idx.iter().map(|&i| units[i]).collect::<Vec<_>>())
    })
}

/// Interval of `wer(system) - wer(baseline)` over the same resampled units
/// (`system[i]` and `baseline[i]` are the same clip).
pub fn paired_interval(system: &[Unit], baseline: &[Unit], reps: usize, seed: u64) -> (f64, f64) {
    assert_eq!(system.len(), baseline.len());
    bootstrap(system.len(), reps, seed, |idx| {
        let s: Vec<Unit> = idx.iter().map(|&i| system[i]).collect();
        let b: Vec<Unit> = idx.iter().map(|&i| baseline[i]).collect();
        wer(&s) - wer(&b)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Vec<Unit> {
        (0..50).map(|i| Unit { errors: (i % 7) as f64, words: 20.0 }).collect()
    }

    #[test]
    fn a_seeded_bootstrap_is_reproducible() {
        let u = units();
        assert_eq!(wer_interval(&u, 4000, 42), wer_interval(&u, 4000, 42));
    }

    #[test]
    fn identical_units_give_a_zero_width_interval() {
        let u = vec![Unit { errors: 2.0, words: 10.0 }; 30];
        assert_eq!(wer_interval(&u, 1000, 1), (20.0, 20.0));
    }

    #[test]
    fn a_system_against_itself_differs_by_exactly_zero() {
        let u = units();
        assert_eq!(paired_interval(&u, &u, 1000, 3), (0.0, 0.0));
    }

    #[test]
    fn the_interval_contains_the_point_estimate() {
        let u = units();
        let (lo, hi) = wer_interval(&u, 4000, 9);
        let w = wer(&u);
        assert!(lo <= w && w <= hi, "{lo} <= {w} <= {hi}");
    }
}
