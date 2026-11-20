/// Eager per-key debounce: a key's first edge counts at once, then the key
/// ignores its switch for `lockout` ticks, so bounce is swallowed without
/// delaying any note. A reading that still differs once the lockout ends
/// counts then, so a tap shorter than the lockout still gets its release.
pub struct Debouncer<const N: usize> {
    pressed: [bool; N],
    ready_at: [u64; N],
}

impl<const N: usize> Debouncer<N> {
    pub const fn new() -> Self {
        Self {
            pressed: [false; N],
            ready_at: [0; N],
        }
    }

    /// Feeds key `idx`'s raw reading at tick `now`, returning its new state
    /// when that reading counts as a press or release.
    pub fn update(&mut self, idx: usize, pressed: bool, now: u64, lockout: u64) -> Option<bool> {
        if pressed == self.pressed[idx] || now < self.ready_at[idx] {
            return None;
        }
        self.pressed[idx] = pressed;
        self.ready_at[idx] = now + lockout;
        Some(pressed)
    }
}

impl<const N: usize> Default for Debouncer<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCKOUT: u64 = 5_000;

    /// Feeds `(tick, reading)` samples to key 0 and collects what counted.
    fn run(samples: &[(u64, bool)]) -> Vec<(u64, bool)> {
        let mut d = Debouncer::<2>::new();
        samples
            .iter()
            .filter_map(|&(t, p)| d.update(0, p, t, LOCKOUT).map(|s| (t, s)))
            .collect()
    }

    #[test]
    fn press_counts_on_its_first_edge() {
        assert_eq!(run(&[(0, false), (100, true)]), [(100, true)]);
    }

    #[test]
    fn bounce_within_the_lockout_is_swallowed() {
        let bouncy_press = [
            (1_000, true),
            (1_300, false),
            (1_600, true),
            (2_000, false),
            (2_400, true),
            (9_000, true),
        ];
        assert_eq!(run(&bouncy_press), [(1_000, true)]);
    }

    #[test]
    fn release_counts_after_the_lockout() {
        let samples = [(1_000, true), (20_000, false), (20_300, true), (20_600, false)];
        assert_eq!(run(&samples), [(1_000, true), (20_000, false)]);
    }

    #[test]
    fn tap_shorter_than_the_lockout_still_releases() {
        let samples = [(1_000, true), (3_000, false), (5_999, false), (6_000, false)];
        assert_eq!(run(&samples), [(1_000, true), (6_000, false)]);
    }

    #[test]
    fn keys_debounce_independently() {
        let mut d = Debouncer::<2>::new();
        assert_eq!(d.update(0, true, 1_000, LOCKOUT), Some(true));
        assert_eq!(d.update(1, true, 1_100, LOCKOUT), Some(true));
        assert_eq!(d.update(0, false, 1_200, LOCKOUT), None);
    }
}
