//! Registered code figures: the sources an article may show with
//! [`crate::base::code_figure::CodeFigure`].
//!
//! An article fence can't carry a code body comfortably, so it names a
//! figure id instead and the body lives here — `include_str!`'d from the
//! example it belongs to, so the listing a reader sees is byte-identical to
//! the file the running sim compiles. One source of truth, no transcribed
//! shader drifting out of date.
//!
//! ⚠️ **Highlight ranges are hand-authored line numbers and do not track
//! edits to the included file.** Change `examples/*/shader.glsl` and the
//! ranges below must be re-checked — the tests at the bottom pin what each
//! range is supposed to be pointing at, so an edit that moves a line fails
//! here rather than silently washing the wrong row violet.

use crate::base::code_figure::{CodeHighlight, CodeHighlightTone};

/// One figure an article can name by id.
pub(crate) struct DocsCodeFigure {
    /// Fence argument: `src=<id>`.
    pub(crate) id: &'static str,
    /// Caption — the file's name as the reader would find it.
    pub(crate) title: &'static str,
    /// The listing, compiled in from the example.
    pub(crate) code: &'static str,
    pub(crate) highlights: &'static [DocsCodeHighlight],
}

/// A highlight in table form: `RangeInclusive` isn't const-constructible, so
/// the endpoints are stored plainly and converted on read.
pub(crate) struct DocsCodeHighlight {
    /// 1-based, inclusive — same numbering the gutter shows.
    pub(crate) first_line: usize,
    pub(crate) last_line: usize,
    pub(crate) tone: CodeHighlightTone,
    pub(crate) label: Option<&'static str>,
}

impl DocsCodeFigure {
    /// The figure's ranges in the shape [`CodeFigure`] takes.
    ///
    /// [`CodeFigure`]: crate::base::code_figure::CodeFigure
    pub(crate) fn highlights(&self) -> Vec<CodeHighlight> {
        self.highlights
            .iter()
            .map(|highlight| CodeHighlight {
                lines: highlight.first_line..=highlight.last_line,
                tone: highlight.tone,
                label: highlight.label.map(str::to_string),
            })
            .collect()
    }
}

/// Every registered figure. Ids are stable — an article's fence references
/// them.
pub(crate) const FIGURES: &[DocsCodeFigure] = &[
    DocsCodeFigure {
        id: "plasma-shader",
        title: "plasma / shader.glsl",
        code: include_str!("../../../../../examples/plasma/shader.glsl"),
        // Line 3 declares `scale`, the uniform the example binds to `bus:scale`
        // (see `examples/plasma/shader.json`) and therefore the one the Scale
        // knob drives — the beat the article is making: turn the knob, this
        // line's value changes.
        highlights: &[DocsCodeHighlight {
            first_line: 3,
            last_line: 3,
            tone: CodeHighlightTone::Bound,
            label: Some("the Scale knob"),
        }],
    },
    DocsCodeFigure {
        id: "peach-body-patch",
        title: "peach-1d / peach_body.patch.json",
        code: include_str!("../../../../../examples/peach-1d/peach_body.patch.json"),
        // Line 5 is the second half of the body: the range that lands after
        // the leaves and arrives at the wire from its far end. `reversed` is
        // the word the article spends a section on, so the eye should find
        // the row before the prose does.
        highlights: &[DocsCodeHighlight {
            first_line: 5,
            last_line: 5,
            tone: CodeHighlightTone::Note,
            label: Some("plugged in backwards"),
        }],
    },
];

/// Resolve a fence's `src=<id>`. Unknown ids are author error surfaced by
/// the caller (a placeholder box), not a panic.
pub(crate) fn code_figure(id: &str) -> Option<&'static DocsCodeFigure> {
    FIGURES.iter().find(|figure| figure.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn figures_resolve_by_id() {
        assert_eq!(
            code_figure("plasma-shader").map(|figure| figure.id),
            Some("plasma-shader")
        );
        assert!(code_figure("no-such-figure").is_none());
    }

    #[test]
    fn every_range_lands_inside_its_figure() {
        for figure in FIGURES {
            let line_count = figure.code.lines().count();
            assert!(
                !figure.code.trim().is_empty(),
                "{} compiled in empty",
                figure.id
            );
            for highlight in figure.highlights {
                assert!(
                    highlight.first_line >= 1 && highlight.last_line <= line_count,
                    "{}: range {}..={} is outside the file's {line_count} lines \
                     — the included file moved, re-author the range",
                    figure.id,
                    highlight.first_line,
                    highlight.last_line
                );
            }
        }
    }

    /// The range's *meaning* is the thing that rots: line 3 is only the
    /// right row while it still declares the bound uniform.
    #[test]
    fn the_plasma_range_still_points_at_the_bound_uniform() {
        let figure = code_figure("plasma-shader").expect("plasma figure");
        let line = figure
            .code
            .lines()
            .nth(2)
            .expect("plasma shader has at least three lines");
        assert!(
            line.contains("uniform") && line.contains("scale"),
            "plasma-shader line 3 no longer declares the `scale` uniform \
             (found {line:?}) — re-author the highlight range"
        );
    }

    /// Same rot, other figure: the peach's highlighted row is only the right
    /// one while it is still the reversed entry the article talks about.
    #[test]
    fn the_peach_range_still_points_at_the_reversed_entry() {
        let figure = code_figure("peach-body-patch").expect("peach patch figure");
        let line = figure
            .code
            .lines()
            .nth(4)
            .expect("the peach body patch has at least five lines");
        assert!(
            line.contains("reversed"),
            "peach-body-patch line 5 is no longer the reversed entry \
             (found {line:?}) — re-author the highlight range"
        );
    }
}
