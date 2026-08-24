//! THE chase light language, in one place.
//!
//! The chase is what a fixture-side patch selection looks like: blue head,
//! red tail, and a white dot sweeping the object's own order — the walk-up
//! answer to "which end is lamp 0, and which way does it run?" (D10, spike
//! `chaseRgb` in `spikes/patching-controls/index.html` §3).
//!
//! It is painted in TWO places, which is exactly why the numbers live here
//! rather than in either of them:
//!
//! - the ENGINE paints it into an output's published bytes when the
//!   `highlight` Debug slot names a `chase:` span list
//!   (`lpc-engine`'s `output_node::paint_chase`), so a MAPPED object chases
//!   on the strip, on the sprites and on the physical piece as ordinary
//!   frame data;
//! - the STUDIO CONTROLLER computes the same colors for the selected
//!   UNMAPPED object, which has no wire and therefore no published bytes
//!   (`lpa-studio-core`'s patch preview), so the panel strip and the canvas
//!   sprites paint one truth instead of each inventing a chase.
//!
//! Two copies of these constants would let those pictures drift; a lamp that
//! chases differently on the panel than on the wall is a lie about the rig.
//!
//! Levels are 16-bit linear unorm per channel — the space the engine's
//! sample buffer speaks — so a client renders them through the same
//! linear → sRGB transfer it decodes any frame with.

/// The chase's head and tail colors, 8-bit per channel.
///
/// Blue leads, red trails: the ends are named by HUE rather than by motion,
/// and the sweep between them says the rest.
pub const HEAD_RGB: [u8; 3] = [0, 0, 255];
pub const TAIL_RGB: [u8; 3] = [255, 0, 0];

/// Most lamps at each end that wear the head/tail color. One lamp is
/// unreadable across a room and eleven is a stripe, not an end marker.
pub const HEAD_MAX_LAMPS: u32 = 10;

/// Full period of the chase's white dot, in seconds — one pass across the
/// whole object per period (the spike's `(t / 2) % 1`).
pub const SWEEP_SECONDS: f32 = 2.0;

/// Falloff of the dot's raised window, in reciprocal object-lengths: the dot
/// fades to the floor `1/DOT_FALLOFF` of the object away from its centre, so
/// it reads as a runner rather than a wash at any lamp count.
pub const DOT_FALLOFF: f32 = 7.0;

/// The chase body's floor and crest, 16-bit unorm per channel (white).
///
/// The body sits near-dark (25/255) so the object reads as
/// dark-with-a-runner: unlike the breath, the chase's job is DIRECTION, and
/// a bright body would hide the dot's travel. The crest is full white for
/// the one dot; peak current stays bounded because the lit window is a
/// fraction of the object and everything else is at the floor.
pub const BODY_FLOOR_16: u16 = 25 * 257;
pub const BODY_CREST_16: u16 = 255 * 257;

/// Head/tail lamp count for an object of `n` lamps (D10): `round(n / 10)`
/// clamped to 1..=[`HEAD_MAX_LAMPS`], in integers (`(n + 5) / 10` is
/// round-half-up).
#[must_use]
pub fn head_lamps(n: u32) -> u32 {
    (n.saturating_add(5) / 10).clamp(1, HEAD_MAX_LAMPS)
}

/// Where the dot sits at `time_seconds`: the sweep folded into `0.0..1.0`.
///
/// Split out from [`body_level_16`] because the two painters keep time
/// differently — the engine has the frame's monotonic seconds, the studio
/// controller counts published frames — while the WINDOW they paint from a
/// phase must stay one function.
#[must_use]
pub fn phase_at(time_seconds: f32) -> f32 {
    let phase = libm::fmodf(time_seconds / SWEEP_SECONDS, 1.0);
    if phase < 0.0 { phase + 1.0 } else { phase }
}

/// The chase body's level for object-order lamp `j` of `n` at `phase`
/// (0..1): a raised window around the sweeping dot, floored at
/// [`BODY_FLOOR_16`] so a selected object never reads as dead lamps.
#[must_use]
pub fn body_level_16(j: u32, n: u32, phase: f32) -> u16 {
    let position = if n <= 1 {
        0.0
    } else {
        j as f32 / (n - 1) as f32
    };
    let window = (1.0 - libm::fabsf(position - phase) * DOT_FALLOFF).clamp(0.0, 1.0);
    let span = f32::from(BODY_CREST_16 - BODY_FLOOR_16);
    BODY_FLOOR_16 + libm::roundf(span * window) as u16
}

/// One lamp of the chase, in 16-bit linear unorm RGB: `ordinal` of `total`
/// in OBJECT order, at `phase`.
///
/// The whole language in one call, so a painter picks neither the head size
/// nor the body window by hand. The engine re-orders the triple into its
/// output's declared channel order afterwards; a client renders it straight.
#[must_use]
pub fn lamp_rgb_16(ordinal: u32, total: u32, phase: f32) -> [u16; 3] {
    let heads = head_lamps(total);
    if ordinal < heads {
        rgb8_to_16(HEAD_RGB)
    } else if ordinal + heads >= total {
        rgb8_to_16(TAIL_RGB)
    } else {
        [body_level_16(ordinal, total, phase); 3]
    }
}

/// 8-bit unorm to 16-bit unorm: 0..=255 maps onto 0..=65535 exactly.
#[must_use]
pub fn rgb8_to_16([r, g, b]: [u8; 3]) -> [u16; 3] {
    [u16::from(r) * 257, u16::from(g) * 257, u16::from(b) * 257]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D10's sizing, in the integer form both painters share — a strip that
    /// rounded differently would draw a different object than the wall does.
    #[test]
    fn head_and_tail_sizing_is_round_n_over_ten_clamped() {
        assert_eq!(head_lamps(0), 1);
        assert_eq!(head_lamps(1), 1, "a tiny object still marks an end");
        assert_eq!(head_lamps(9), 1);
        assert_eq!(head_lamps(15), 2, "round(n / 10), not floor");
        assert_eq!(head_lamps(100), 10);
        assert_eq!(head_lamps(30_000), 10, "clamped, not proportional");
    }

    /// The phase folds seconds into one sweep, forward and backward.
    #[test]
    fn the_phase_folds_seconds_into_one_sweep() {
        assert!((phase_at(0.0) - 0.0).abs() < 1e-6);
        assert!((phase_at(1.0) - 0.5).abs() < 1e-6);
        assert!((phase_at(2.0) - 0.0).abs() < 1e-6);
        assert!((phase_at(3.5) - 0.75).abs() < 1e-6);
        assert!(
            (0.0..1.0).contains(&phase_at(-0.5)),
            "a negative clock still lands inside the sweep"
        );
    }

    /// Blue leads, red trails, and everything between is grey — the walk-up
    /// question answered in the lamps.
    #[test]
    fn the_chase_paints_a_blue_head_and_a_red_tail() {
        let heads = head_lamps(40);
        assert_eq!(heads, 4);
        for ordinal in 0..heads {
            assert_eq!(lamp_rgb_16(ordinal, 40, 0.5), rgb8_to_16(HEAD_RGB));
        }
        for ordinal in 40 - heads..40 {
            assert_eq!(lamp_rgb_16(ordinal, 40, 0.5), rgb8_to_16(TAIL_RGB));
        }
        for ordinal in heads..40 - heads {
            let [r, g, b] = lamp_rgb_16(ordinal, 40, 0.5);
            assert_eq!(r, g, "the body is white/grey, never hued");
            assert_eq!(g, b);
        }
    }

    /// The dot is a RUNNER, not a wash: brightest where the phase says, and
    /// the far end of the object stays at the floor.
    #[test]
    fn the_dot_sweeps_the_object_in_order() {
        assert!(body_level_16(25, 100, 0.25) > body_level_16(75, 100, 0.25));
        assert!(body_level_16(75, 100, 0.75) > body_level_16(25, 100, 0.75));
        assert_eq!(
            body_level_16(75, 100, 0.25),
            BODY_FLOOR_16,
            "the far end sits at the floor"
        );
        assert_eq!(
            body_level_16(0, 1, 0.0),
            BODY_CREST_16,
            "a one-lamp object sits at position 0 — lit when the dot is there",
        );
    }

    /// A one-lamp object is all head rather than a divide by zero.
    #[test]
    fn a_single_lamp_object_is_all_head() {
        assert_eq!(lamp_rgb_16(0, 1, 0.4), rgb8_to_16(HEAD_RGB));
        assert_eq!(rgb8_to_16([0, 0, 255]), [0, 0, 65535]);
    }
}
