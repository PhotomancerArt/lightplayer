//! What a selection does to a fixture's lamps: the `selection` Debug slot.
//!
//! The mapping lab's fixture overrides (plan
//! `lp2025/2026-10-17-0202-mapping-lab-into-studio`, M4b; the lab's
//! `domain/fixture_overrides.rs`), on the board: applied to the fixture's
//! lamp colours after it has sampled (and after gamma, brightness and the
//! power clamp) and before it publishes them, so every output the fixture
//! reaches shows the same, and so does every view that draws its lamps.
//!
//! The slot speaks the fixture's own lamp numbering (the order its objects
//! wire in), `;`-joined parts:
//!
//! ```text
//! value := part { ";" part }
//! part  := "dim" | "chase:" list | "breath:" list
//! list  := segment { "," segment }        ; as `highlight`'s
//! ```
//!
//! - **`dim`**: every lamp dims to a quarter (the output highlight's `>> 2`).
//! - **`chase:`**: each segment is one object, chasing in lp2014's language
//!   (`lpc_model::nodes::output::chase`: blue head, red tail, a white dot
//!   sweeping from lamp 0), so you see where its data enters and which way
//!   it runs.
//! - **`breath:`**: the lamps breathe white, as the output's highlight does.
//!
//! Any chase or breath dims every other lamp too: the selection is always
//! shown. Empty, or nothing that parses: the lamps are as sampled. A part
//! the engine doesn't know is skipped, so a newer Studio's text never
//! paints something wrong.

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::nodes::output::chase;

use crate::nodes::output::output_node::{highlight_level_16, parse_span_segment};

/// A fixture's selection, parsed once from its slot's text.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FixtureSelection {
    /// The text it was parsed from, to parse again only when it changes.
    text: alloc::string::String,
    dim: bool,
    /// `(first lamp, lamps)`: one object each.
    chases: Vec<(u32, u32)>,
    /// `(first lamp, lamps)`.
    breaths: Vec<(u32, u32)>,
}

impl FixtureSelection {
    /// Bring a fixture's held selection in step with its slot's `text`:
    /// `None` for an empty one (nothing held), parsed again only when the
    /// text changes.
    pub(crate) fn update_in(held: &mut Option<Box<FixtureSelection>>, text: &str) {
        if text.is_empty() {
            *held = None;
            return;
        }
        if held.as_ref().is_some_and(|s| s.text == text) {
            return;
        }
        *held = Some(Box::new(Self::parse(text)));
    }

    fn parse(text: &str) -> Self {
        let mut out = Self {
            text: text.into(),
            ..Self::default()
        };
        for part in text.split(';') {
            let part = part.trim();
            if part.eq_ignore_ascii_case("dim") {
                out.dim = true;
                continue;
            }
            let Some((kind, list)) = part.split_once(':') else {
                continue;
            };
            let spans = match kind.trim() {
                k if k.eq_ignore_ascii_case("chase") => &mut out.chases,
                k if k.eq_ignore_ascii_case("breath") => &mut out.breaths,
                _ => continue,
            };
            for segment in list.split(',') {
                if let Some((first, last)) = parse_span_segment(segment)
                    && first <= last
                {
                    spans.push((first, last - first + 1));
                }
            }
        }
        out.dim |= !out.chases.is_empty() || !out.breaths.is_empty();
        out
    }

    /// Whether it changes any lamp.
    pub(crate) fn is_active(&self) -> bool {
        self.dim
    }

    /// Lamp `lamp`'s colour (16-bit linear RGB, before colour order) under
    /// this selection, at `time_seconds`.
    pub(crate) fn apply(&self, lamp: u32, rgb: [u16; 3], time_seconds: f32) -> [u16; 3] {
        if !self.dim {
            return rgb;
        }
        if let Some(&(first, lamps)) = self.chases.iter().find(|s| within(**s, lamp)) {
            return chase::lamp_rgb_16(lamp - first, lamps, chase::phase_at(time_seconds));
        }
        if self.breaths.iter().any(|s| within(*s, lamp)) {
            let level = highlight_level_16(time_seconds);
            return [level, level, level];
        }
        rgb.map(|c| c >> 2)
    }
}

/// A selection at one frame's time, handed to the lamp writes: `None` when
/// there is nothing to apply (diagnostics, tests, an empty selection).
#[derive(Clone, Copy)]
pub(crate) struct SelectionAt<'a> {
    selection: Option<&'a FixtureSelection>,
    time_seconds: f32,
}

impl<'a> SelectionAt<'a> {
    #[cfg(test)]
    pub(crate) const NONE: SelectionAt<'static> = SelectionAt {
        selection: None,
        time_seconds: 0.0,
    };

    pub(crate) fn new(selection: Option<&'a FixtureSelection>, time_seconds: f32) -> Self {
        Self {
            selection: selection.filter(|s| s.is_active()),
            time_seconds,
        }
    }

    /// Lamp `lamp`'s colour under the selection.
    #[inline]
    pub(crate) fn apply(&self, lamp: u32, rgb: [u16; 3]) -> [u16; 3] {
        match self.selection {
            Some(s) => s.apply(lamp, rgb, self.time_seconds),
            None => rgb,
        }
    }
}

fn within((first, lamps): (u32, u32), lamp: u32) -> bool {
    lamp >= first && lamp - first < lamps
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIT: [u16; 3] = [40_000, 20_000, 8_000];

    #[test]
    fn empty_leaves_the_lamps_as_sampled() {
        let s = FixtureSelection::parse("");
        assert!(!s.is_active());
        assert_eq!(s.apply(3, LIT, 0.0), LIT);
    }

    #[test]
    fn dim_quarters_every_lamp() {
        let s = FixtureSelection::parse("dim");
        assert_eq!(s.apply(0, LIT, 0.0), [10_000, 5_000, 2_000]);
    }

    /// Each chase segment is its own object: its first lamps blue, its last
    /// red, and the rest of the fixture dims.
    #[test]
    fn a_chase_names_where_each_object_starts_and_ends() {
        let s = FixtureSelection::parse("chase:10-29,40-49");
        assert_eq!(s.apply(10, LIT, 0.0), chase::rgb8_to_16(chase::HEAD_RGB));
        assert_eq!(s.apply(29, LIT, 0.0), chase::rgb8_to_16(chase::TAIL_RGB));
        assert_eq!(s.apply(40, LIT, 0.0), chase::rgb8_to_16(chase::HEAD_RGB));
        assert_eq!(s.apply(49, LIT, 0.0), chase::rgb8_to_16(chase::TAIL_RGB));
        assert_eq!(s.apply(35, LIT, 0.0), [10_000, 5_000, 2_000]);
    }

    #[test]
    fn a_breath_breathes_white_and_dims_the_rest() {
        let s = FixtureSelection::parse("breath:5");
        let [r, g, b] = s.apply(5, LIT, 0.375);
        assert!(r == g && g == b && r > 0);
        assert_eq!(s.apply(6, LIT, 0.375), [10_000, 5_000, 2_000]);
    }

    /// A part it doesn't know is skipped, and the rest still applies.
    #[test]
    fn an_unknown_part_is_skipped() {
        let s = FixtureSelection::parse("sparkle:1-3; breath:2");
        assert!(s.is_active());
        assert_eq!(s.apply(1, LIT, 0.0), [10_000, 5_000, 2_000]);
    }
}
