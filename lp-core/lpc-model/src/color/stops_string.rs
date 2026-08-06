//! The stops literal: a [`Gradient`]'s payload as one compact string.
//!
//! `"#000 #f80@.5 (0.211,-0.017,-0.039)"` — each whitespace-separated token
//! is one stop, `color[@position]`. This is the storage, wire, and authored
//! form of a gradient's bulk (`docs/design/color.md` §5; ADR
//! 2026-08-05-gradient-stops-string-storage): metadata (`space`, `method`)
//! stays structural JSON, the part that scales with content is one literal.
//!
//! Colors are **raw coordinates in the gradient's own space** — never
//! converted here (space conversions live engine-side, `color.md` §7):
//!
//! - `(a,b,c)` — decimal triplet, the general form; the only way to spell
//!   negative axes (Oklab `a`/`b`) and full `f32` precision.
//! - `#rgb` / `#rrggbb` / `#rrrrggggbbbb` — `[0,1]`-fraction shorthand,
//!   component = `k / (2ⁿ−1)`. Notation, not "an sRGB color": it is only
//!   *printed* for the sRGB-shaped spaces, but parses anywhere.
//!
//! Positions are optional per stop, CSS-style: an unpositioned first stop
//! is `0`, an unpositioned last is `1`, and interior unpositioned runs
//! distribute linearly between their positioned neighbors. Explicit
//! positions must be non-decreasing and within `[0,1]` — errors, not
//! clamps.
//!
//! Printing is **canonical and lossless**: `parse(print(stops)) == stops`
//! bit-exact. Positions are omitted entirely iff the stops sit bit-exactly
//! on the even grid `i/(n-1)`; otherwise every stop prints one. A component
//! prints as `#rrggbb` hex only when the caller's space allows it and all
//! three components are bit-exactly `k/255`; otherwise it prints
//! shortest-round-trip decimals. The accept-only tiers (`#rgb`, 16-bit
//! hex) are never printed.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::fmt::Write as _;

use super::gradient::{Colorspace, GradientStop, MAX_GRADIENT_STOPS, MIN_GRADIENT_STOPS};

/// Why a stops literal failed to parse. `index` is the zero-based stop
/// (token) the failure was found at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StopsParseError {
    /// No stop tokens at all.
    Empty,
    /// Fewer than [`MIN_GRADIENT_STOPS`] stops.
    TooFewStops(usize),
    /// More than [`MAX_GRADIENT_STOPS`] stops.
    TooManyStops(usize),
    /// A token's color part is not a hex tier or a decimal triplet.
    InvalidColor { index: usize },
    /// A `@position` suffix is not a finite number.
    InvalidPosition { index: usize },
    /// An explicit position is outside `[0, 1]`.
    PositionOutOfRange { index: usize },
    /// An explicit position is smaller than an earlier one.
    PositionsNotSorted { index: usize },
}

impl fmt::Display for StopsParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("stops literal is empty"),
            Self::TooFewStops(n) => write!(
                f,
                "a gradient needs at least {MIN_GRADIENT_STOPS} stops, got {n}"
            ),
            Self::TooManyStops(n) => write!(
                f,
                "a gradient holds at most {MAX_GRADIENT_STOPS} stops, got {n}"
            ),
            Self::InvalidColor { index } => write!(
                f,
                "stop {index}: expected #rgb, #rrggbb, #rrrrggggbbbb, or (a,b,c)"
            ),
            Self::InvalidPosition { index } => {
                write!(
                    f,
                    "stop {index}: position after '@' must be a finite number"
                )
            }
            Self::PositionOutOfRange { index } => {
                write!(f, "stop {index}: position must be within 0..=1")
            }
            Self::PositionsNotSorted { index } => {
                write!(f, "stop {index}: positions must not decrease")
            }
        }
    }
}

impl core::error::Error for StopsParseError {}

/// Parse a stops literal into stops with every position resolved.
pub fn parse_stops(input: &str) -> Result<Vec<GradientStop>, StopsParseError> {
    let mut colors: Vec<[f32; 3]> = Vec::new();
    let mut positions: Vec<Option<f32>> = Vec::new();

    for token in input.split_whitespace() {
        let index = colors.len();
        let (color_text, position_text) = split_stop_token(token);
        let Some(c) = parse_color(color_text) else {
            return Err(StopsParseError::InvalidColor { index });
        };
        let at = match position_text {
            None => None,
            Some(text) => {
                let at: f32 = text
                    .parse()
                    .ok()
                    .filter(|at: &f32| at.is_finite())
                    .ok_or(StopsParseError::InvalidPosition { index })?;
                if !(0.0..=1.0).contains(&at) {
                    return Err(StopsParseError::PositionOutOfRange { index });
                }
                Some(at)
            }
        };
        colors.push(c);
        positions.push(at);
    }

    if colors.is_empty() {
        return Err(StopsParseError::Empty);
    }
    if colors.len() < MIN_GRADIENT_STOPS as usize {
        return Err(StopsParseError::TooFewStops(colors.len()));
    }
    if colors.len() > MAX_GRADIENT_STOPS as usize {
        return Err(StopsParseError::TooManyStops(colors.len()));
    }

    // Explicit positions must already be non-decreasing (the fill below
    // preserves monotonicity between anchors, so this is the only ordering
    // check needed).
    let mut last_explicit: Option<f32> = None;
    for (index, at) in positions.iter().enumerate() {
        if let Some(at) = at {
            if let Some(previous) = last_explicit
                && *at < previous
            {
                return Err(StopsParseError::PositionsNotSorted { index });
            }
            last_explicit = Some(*at);
        }
    }

    let positions = fill_positions(positions);
    Ok(colors
        .into_iter()
        .zip(positions)
        .map(|(c, at)| GradientStop { at, c })
        .collect())
}

/// Print stops canonically. `space` decides hex eligibility: only the
/// sRGB-shaped spaces ever print hex, so a Lab gradient never *looks* like
/// it carries RGB bytes.
#[must_use]
pub fn print_stops(space: Colorspace, stops: &[GradientStop]) -> String {
    let hex_allowed = matches!(space, Colorspace::Srgb | Colorspace::LinearSrgb);
    let even = stops_are_evenly_spaced(stops);
    let mut out = String::new();
    for (index, stop) in stops.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        match (hex_allowed, hex8_components(&stop.c)) {
            (true, Some([r, g, b])) => {
                let _ = write!(out, "#{r:02x}{g:02x}{b:02x}");
            }
            _ => {
                let _ = write!(out, "({},{},{})", stop.c[0], stop.c[1], stop.c[2]);
            }
        }
        if !even {
            let _ = write!(out, "@{}", stop.at);
        }
    }
    out
}

/// Split one token into its color part and optional position part.
///
/// A triplet's own commas/negatives never contain `@`, so the first `@`
/// after the color body is the position separator.
fn split_stop_token(token: &str) -> (&str, Option<&str>) {
    match token.split_once('@') {
        Some((color, position)) => (color, Some(position)),
        None => (token, None),
    }
}

fn parse_color(text: &str) -> Option<[f32; 3]> {
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex);
    }
    let body = text.strip_prefix('(')?.strip_suffix(')')?;
    let mut parts = body.split(',');
    let mut c = [0.0f32; 3];
    for slot in &mut c {
        let part = parts.next()?.trim();
        let value: f32 = part.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        *slot = value;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(c)
}

/// Parse a hex body (no `#`) at one of the three accepted widths.
/// Component value is `k / (2ⁿ−1)`, so `#fff` and `#ffffff` both mean 1.0.
fn parse_hex(hex: &str) -> Option<[f32; 3]> {
    let per_channel = match hex.len() {
        3 => 1,
        6 => 2,
        12 => 4,
        _ => return None,
    };
    let max = ((1u32 << (4 * per_channel)) - 1) as f32;
    let mut c = [0.0f32; 3];
    for (slot, chunk) in c.iter_mut().zip(0..3) {
        let start = chunk * per_channel;
        let digits = hex.get(start..start + per_channel)?;
        let k = u32::from_str_radix(digits, 16).ok()?;
        *slot = k as f32 / max;
    }
    Some(c)
}

/// The `[r, g, b]` bytes when every component is bit-exactly `k/255`.
fn hex8_components(c: &[f32; 3]) -> Option<[u8; 3]> {
    let mut out = [0u8; 3];
    for (byte, component) in out.iter_mut().zip(c.iter()) {
        if !(0.0..=1.0).contains(component) {
            return None;
        }
        let k = (component * 255.0) as u32;
        // The candidate byte is either k or k+1 after truncation; accept
        // whichever reproduces the component exactly.
        let exact = [k, k.saturating_add(1)]
            .into_iter()
            .find(|k| *k <= 255 && (*k as f32 / 255.0).to_bits() == component.to_bits())?;
        *byte = exact as u8;
    }
    Some(out)
}

/// CSS-style position fill: unpositioned first → 0, unpositioned last → 1,
/// interior runs distribute linearly between their anchors.
fn fill_positions(mut positions: Vec<Option<f32>>) -> Vec<f32> {
    let last = positions.len() - 1;
    if positions[0].is_none() {
        positions[0] = Some(0.0);
    }
    if positions[last].is_none() {
        positions[last] = Some(1.0);
    }
    let mut out: Vec<f32> = Vec::with_capacity(positions.len());
    let mut anchor_index = 0usize;
    let mut anchor_at = positions[0].expect("first position anchored above");
    out.push(anchor_at);
    let mut index = 1;
    while index <= last {
        if let Some(at) = positions[index] {
            let run = (index - anchor_index) as f32;
            for step in 1..(index - anchor_index) {
                out.push(anchor_at + (at - anchor_at) * step as f32 / run);
            }
            out.push(at);
            anchor_index = index;
            anchor_at = at;
        }
        index += 1;
    }
    out
}

/// Whether the stops sit bit-exactly on the even grid `i/(n-1)`.
fn stops_are_evenly_spaced(stops: &[GradientStop]) -> bool {
    if stops.len() < 2 {
        return false;
    }
    let denominator = (stops.len() - 1) as f32;
    stops
        .iter()
        .enumerate()
        .all(|(index, stop)| (index as f32 / denominator).to_bits() == stop.at.to_bits())
}

#[cfg(test)]
mod tests {
    use alloc::format;
    use alloc::string::ToString;
    use alloc::vec;

    use super::*;

    fn stop(at: f32, c: [f32; 3]) -> GradientStop {
        GradientStop { at, c }
    }

    #[test]
    fn parses_every_color_notation() {
        let stops = parse_stops("#fff #ff8800 #ffff88000000 (0.5,-0.25,1)").unwrap();
        assert_eq!(stops[0].c, [1.0, 1.0, 1.0]);
        assert_eq!(stops[1].c, [1.0, 136.0 / 255.0, 0.0]);
        assert_eq!(stops[2].c, [1.0, 34816.0 / 65535.0, 0.0]);
        assert_eq!(stops[3].c, [0.5, -0.25, 1.0]);
    }

    #[test]
    fn omitted_positions_distribute_evenly() {
        let stops = parse_stops("#000 #888 #fff").unwrap();
        assert_eq!(stops[0].at, 0.0);
        assert_eq!(stops[1].at, 0.5);
        assert_eq!(stops[2].at, 1.0);
    }

    #[test]
    fn mixed_positions_fill_css_style() {
        // Unpositioned first → 0, last → 1; the run between anchors
        // distributes linearly.
        let stops = parse_stops("#000 #111@.4 #222 #333 #444@.7 #fff").unwrap();
        let ats: Vec<f32> = stops.iter().map(|stop| stop.at).collect();
        assert_eq!(ats, vec![0.0, 0.4, 0.5, 0.6, 0.7, 1.0]);
    }

    #[test]
    fn leading_dot_and_negative_numbers_parse() {
        let stops = parse_stops("(0.211,-0.017,-0.039)@.25 (1,1,1)").unwrap();
        assert_eq!(stops[0].at, 0.25);
        assert_eq!(stops[0].c[1], -0.017);
    }

    #[test]
    fn rejects_malformed_input() {
        // Stop counts.
        assert_eq!(parse_stops(""), Err(StopsParseError::Empty));
        assert_eq!(parse_stops("   "), Err(StopsParseError::Empty));
        assert_eq!(parse_stops("#fff"), Err(StopsParseError::TooFewStops(1)));
        let over = "#fff ".repeat(MAX_GRADIENT_STOPS as usize + 1);
        assert_eq!(
            parse_stops(&over),
            Err(StopsParseError::TooManyStops(
                MAX_GRADIENT_STOPS as usize + 1
            ))
        );
        // Colors.
        for bad in ["#ff", "#gggggg", "(1,2)", "(1,2,3,4)", "(1,2,x)", "fff"] {
            assert_eq!(
                parse_stops(&format!("{bad} #fff")),
                Err(StopsParseError::InvalidColor { index: 0 }),
                "{bad}"
            );
        }
        // Positions.
        assert_eq!(
            parse_stops("#000@x #fff"),
            Err(StopsParseError::InvalidPosition { index: 0 })
        );
        assert_eq!(
            parse_stops("#000@1.5 #fff"),
            Err(StopsParseError::PositionOutOfRange { index: 0 })
        );
        assert_eq!(
            parse_stops("#000@.8 #fff@.2"),
            Err(StopsParseError::PositionsNotSorted { index: 1 })
        );
    }

    #[test]
    fn canonical_print_omits_even_positions_and_prints_uneven_ones() {
        let even = vec![
            stop(0.0, [0.0, 0.0, 0.0]),
            stop(0.5, [1.0, 1.0, 1.0]),
            stop(1.0, [1.0, 0.0, 0.0]),
        ];
        assert_eq!(
            print_stops(Colorspace::Srgb, &even),
            "#000000 #ffffff #ff0000"
        );

        let uneven = vec![stop(0.0, [0.0, 0.0, 0.0]), stop(0.7, [1.0, 1.0, 1.0])];
        assert_eq!(
            print_stops(Colorspace::Srgb, &uneven),
            "#000000@0 #ffffff@0.7"
        );
    }

    #[test]
    fn hex_prints_only_for_srgb_shaped_spaces() {
        let stops = vec![stop(0.0, [0.0, 0.0, 0.0]), stop(1.0, [1.0, 1.0, 1.0])];
        assert_eq!(
            print_stops(Colorspace::LinearSrgb, &stops),
            "#000000 #ffffff"
        );
        // The same components in a Lab space print as decimals: a Lab
        // gradient must never look like RGB bytes.
        assert_eq!(print_stops(Colorspace::Oklab, &stops), "(0,0,0) (1,1,1)");
    }

    #[test]
    fn decimals_print_when_components_are_not_hex_exact() {
        let stops = vec![stop(0.0, [0.5, 0.25, 0.1]), stop(1.0, [1.0, 1.0, 1.0])];
        let printed = print_stops(Colorspace::Srgb, &stops);
        assert_eq!(printed, "(0.5,0.25,0.1) #ffffff");
    }

    #[test]
    fn round_trips_bit_exact() {
        for (space, literal) in [
            (Colorspace::Srgb, "#000000 #ff8800@0.5 #ffffff@1"),
            (
                Colorspace::Oklab,
                "(0.211231,-0.016952,-0.039276) (0.7418,-0.148559,0.051566)",
            ),
            (Colorspace::Srgb, "#00ff88 #883300"),
        ] {
            let stops = parse_stops(literal).unwrap();
            let printed = print_stops(space, &stops);
            let reparsed = parse_stops(&printed).unwrap();
            assert_eq!(reparsed, stops, "{literal} → {printed}");
        }
    }

    /// Deterministic pseudo-random round-trip sweep (no proptest dep in the
    /// workspace): many stop vectors across counts, spaces, hex-exact and
    /// arbitrary components, even and uneven spacings — `parse(print(s))`
    /// must reproduce `s` bit-exactly every time.
    #[test]
    fn round_trips_bit_exact_across_generated_stops() {
        let mut state = 0x243F_6A88_85A3_08D3u64; // seed; LCG below
        let mut next = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as u32
        };
        for case in 0..500 {
            let count = 2 + (next() % (MAX_GRADIENT_STOPS - 1)) as usize;
            let space = Colorspace::all()[(next() % 6) as usize];
            let even = next() % 2 == 0;
            let mut stops = Vec::with_capacity(count);
            let mut at = 0.0f32;
            for index in 0..count {
                at = if even {
                    index as f32 / (count - 1) as f32
                } else if index == 0 {
                    0.0
                } else {
                    // Non-decreasing, quantized so Display strings stay sane.
                    (at + (next() % 1000) as f32 / 10_000.0).min(1.0)
                };
                let c = if next() % 2 == 0 {
                    // Hex-exact components.
                    [
                        (next() % 256) as f32 / 255.0,
                        (next() % 256) as f32 / 255.0,
                        (next() % 256) as f32 / 255.0,
                    ]
                } else {
                    // Arbitrary (including negative) components.
                    [
                        (next() as f32 / u32::MAX as f32) * 2.0 - 0.5,
                        (next() as f32 / u32::MAX as f32) * 2.0 - 0.5,
                        (next() as f32 / u32::MAX as f32) * 2.0 - 0.5,
                    ]
                };
                stops.push(GradientStop { at, c });
            }
            let printed = print_stops(space, &stops);
            let reparsed = parse_stops(&printed)
                .unwrap_or_else(|error| panic!("case {case}: {printed:?}: {error}"));
            assert_eq!(reparsed, stops, "case {case}: {printed}");
        }
    }

    #[test]
    fn errors_display_readably() {
        assert!(
            StopsParseError::PositionsNotSorted { index: 3 }
                .to_string()
                .contains("stop 3")
        );
    }
}
