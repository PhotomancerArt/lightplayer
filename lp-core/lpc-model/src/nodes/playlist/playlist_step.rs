//! Next and previous through a playlist's entries: the one rule Studio's
//! next/prev offers and the engine's next/prev trigger ids both use
//! (playlists-on-the-bus vision D15).

/// The entry `steps` playable stops away from `from` (negative is
/// previous), wrapping, or `None` when nothing else can play.
///
/// `keys` are the playlist's authored entry keys in ascending order;
/// `playable` says which may be stepped onto (not skipped, not failed). A
/// `from` that is not in `keys` steps from where it would sort. The walk
/// never returns `from`: landing back on it (or finding nothing playable)
/// is `None`.
pub fn playlist_step(
    keys: &[u32],
    from: u32,
    steps: i32,
    playable: impl Fn(u32) -> bool,
) -> Option<u32> {
    let mut at = from;
    for _ in 0..steps.unsigned_abs() {
        at = if steps > 0 {
            next_after(keys, at, &playable)?
        } else {
            prev_before(keys, at, &playable)?
        };
    }
    (at != from).then_some(at)
}

/// The smallest playable key above `key`, else the smallest playable key
/// (wrapping); never `key` itself.
fn next_after(keys: &[u32], key: u32, playable: &impl Fn(u32) -> bool) -> Option<u32> {
    let candidates = || keys.iter().copied().filter(|k| *k != key && playable(*k));
    candidates()
        .filter(|k| *k > key)
        .min()
        .or_else(|| candidates().min())
}

/// The largest playable key below `key`, else the largest playable key
/// (wrapping); never `key` itself.
fn prev_before(keys: &[u32], key: u32, playable: &impl Fn(u32) -> bool) -> Option<u32> {
    let candidates = || keys.iter().copied().filter(|k| *k != key && playable(*k));
    candidates()
        .filter(|k| *k < key)
        .max()
        .or_else(|| candidates().max())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYS: [u32; 5] = [1, 2, 3, 4, 5];

    #[test]
    fn next_and_prev_wrap() {
        let all = |_| true;
        assert_eq!(playlist_step(&KEYS, 5, 1, all), Some(1));
        assert_eq!(playlist_step(&KEYS, 1, -1, all), Some(5));
        assert_eq!(playlist_step(&KEYS, 2, 1, all), Some(3));
        assert_eq!(playlist_step(&KEYS, 2, -1, all), Some(1));
    }

    #[test]
    fn unplayable_entries_are_passed_over() {
        let playable = |k| k != 3 && k != 1;
        assert_eq!(playlist_step(&KEYS, 2, 1, playable), Some(4));
        assert_eq!(playlist_step(&KEYS, 2, -1, playable), Some(5));
    }

    #[test]
    fn several_steps_net_and_landing_home_is_none() {
        let all = |_| true;
        assert_eq!(playlist_step(&KEYS, 1, 2, all), Some(3));
        assert_eq!(playlist_step(&KEYS, 1, -2, all), Some(4));
        assert_eq!(
            playlist_step(&KEYS, 1, 5, all),
            None,
            "a full loop lands home"
        );
        assert_eq!(playlist_step(&KEYS, 1, 0, all), None);
    }

    #[test]
    fn an_unknown_from_steps_from_where_it_would_sort() {
        let all = |_| true;
        assert_eq!(playlist_step(&[2, 4, 6], 3, 1, all), Some(4));
        assert_eq!(playlist_step(&[2, 4, 6], 3, -1, all), Some(2));
        assert_eq!(playlist_step(&[2, 4, 6], 9, 1, all), Some(2));
    }

    #[test]
    fn nothing_else_playable_is_none() {
        assert_eq!(playlist_step(&KEYS, 2, 1, |k| k == 2), None);
        assert_eq!(playlist_step(&[], 2, 1, |_| true), None);
    }
}
