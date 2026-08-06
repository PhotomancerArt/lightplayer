//! Read-only code figure: a source listing with hand-authored highlight
//! ranges — the docs primitive for "look, *this* line is the knob".
//!
//! Display only. No editing, no caret, no focus trap; the editable surface
//! is [`crate::base::code_editor::CodeEditor`]. Text stays selectable,
//! because copying a snippet out of an article is friendly.
//!
//! **Tone convention.** [`CodeHighlightTone::Bound`] is violet
//! (`status-bound-*`), the same color Studio wears everywhere for "this
//! value comes from a bus". A figure is often where a reader meets that
//! color for the first time, so it must never drift toward another status
//! hue. [`CodeHighlightTone::Note`] is the neutral tone for annotation that
//! makes no status claim.
//!
//! Ranges are hand-authored 1-based line numbers (deriving them from
//! compiler spans is a later idea). A range that runs past the end of the
//! code simply covers fewer rows — never a panic.
//!
//! No syntax highlighting: the GLSL tokenizer lives inside the vendored
//! CodeMirror bundle and cannot be lent to a static render, and adding a
//! highlighting dependency is out of scope. Monochrome code plus the
//! highlight wash carries the beat.

use std::ops::RangeInclusive;

use dioxus::prelude::*;

/// How one highlighted range reads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CodeHighlightTone {
    /// Bound to a bus or control: violet, Studio's binding color.
    #[default]
    Bound,
    /// Plain annotation: neutral wash, no status claim.
    Note,
}

/// One highlighted range of a [`CodeFigure`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeHighlight {
    /// 1-based and inclusive — the numbers the reader sees in the gutter.
    pub lines: RangeInclusive<usize>,
    pub tone: CodeHighlightTone,
    /// Small chip on the range's first line, right-aligned (e.g. "the Scale
    /// knob"). Ranges without a label just get the wash.
    pub label: Option<String>,
}

/// Read-only code listing with a quiet line-number gutter and washed
/// highlight ranges. See the module docs for the tone convention.
///
/// Long or wide code scrolls *inside* the figure; the page never scrolls
/// sideways because an article embedded a big shader.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn CodeFigure(
    /// Source text, rendered verbatim.
    code: String,
    /// Caption above the listing — a filename, usually.
    #[props(default)]
    title: Option<String>,
    /// Hand-authored ranges. Overlaps are author error rather than a case
    /// to blend, so the first range covering a line wins.
    #[props(default)]
    highlights: Vec<CodeHighlight>,
) -> Element {
    let rows = figure_rows(&code, &highlights);
    // Width the gutter to the widest line number so the code column does
    // not shift between a 9-line and a 900-line figure.
    let gutter_ch = rows.len().to_string().len();

    rsx! {
        figure { class: "tw:m-0 tw:grid tw:min-w-0 tw:gap-0 tw:overflow-hidden tw:rounded-sm tw:border tw:border-border tw:bg-card",
            if let Some(title) = title {
                figcaption { class: "tw:border-b tw:border-border-muted tw:bg-card-muted tw:px-2 tw:py-1 tw:font-mono tw:text-xs tw:text-muted-foreground",
                    "{title}"
                }
            }
            div { class: "tw:max-h-96 tw:overflow-auto tw:py-1 tw:font-mono tw:text-xs tw:leading-relaxed",
                // `w-max` lets a row's wash and its right-aligned label chip
                // span the widest line rather than stopping at the viewport;
                // `min-w-full` keeps short listings flush with the frame.
                div { class: "tw:w-max tw:min-w-full",
                    for row in &rows {
                        div { class: row_class(row.tone),
                            span {
                                class: gutter_class(row.tone),
                                style: "min-width: {gutter_ch}ch",
                                "{row.number}"
                            }
                            span { class: "tw:whitespace-pre tw:pr-3 tw:text-foreground", "{row.text}" }
                            if let Some(label) = &row.label {
                                // Sticky so the chip stays readable while the
                                // reader scrolls a wide line horizontally.
                                span { class: label_class(row.tone), "{label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One rendered line: its number, its text, and the highlight (if any)
/// covering it.
struct FigureRow<'a> {
    number: usize,
    text: &'a str,
    tone: Option<CodeHighlightTone>,
    /// Present only on a range's first line — the chip labels the range,
    /// not every row of it.
    label: Option<&'a str>,
}

/// Resolve every line against the highlight list. A single trailing newline
/// is dropped so a file ending in `\n` does not render a phantom last row.
fn figure_rows<'a>(code: &'a str, highlights: &'a [CodeHighlight]) -> Vec<FigureRow<'a>> {
    code.strip_suffix('\n')
        .unwrap_or(code)
        .split('\n')
        .enumerate()
        .map(|(index, text)| {
            let number = index + 1;
            let highlight = highlights
                .iter()
                .find(|highlight| highlight.lines.contains(&number));
            FigureRow {
                number,
                text,
                tone: highlight.map(|highlight| highlight.tone),
                label: highlight
                    .filter(|highlight| *highlight.lines.start() == number)
                    .and_then(|highlight| highlight.label.as_deref()),
            }
        })
        .collect()
}

/// Row chrome: full-width wash plus the left accent bar. Unhighlighted rows
/// keep a transparent bar so the code column never shifts.
fn row_class(tone: Option<CodeHighlightTone>) -> &'static str {
    match tone {
        None => "tw:flex tw:items-center tw:border-l-2 tw:border-transparent",
        Some(CodeHighlightTone::Bound) => {
            "tw:flex tw:items-center tw:border-l-2 tw:border-status-bound-border tw:bg-status-bound-bg"
        }
        Some(CodeHighlightTone::Note) => {
            "tw:flex tw:items-center tw:border-l-2 tw:border-border-strong tw:bg-card-muted"
        }
    }
}

/// Gutter numbers: quiet by default, tinted on a highlighted row. Never
/// selectable — copying the figure should yield code, not line numbers.
fn gutter_class(tone: Option<CodeHighlightTone>) -> &'static str {
    match tone {
        None => "tw:shrink-0 tw:select-none tw:pr-3 tw:pl-2 tw:text-right tw:text-dim-foreground",
        Some(CodeHighlightTone::Bound) => {
            "tw:shrink-0 tw:select-none tw:pr-3 tw:pl-2 tw:text-right tw:text-status-bound-foreground"
        }
        Some(CodeHighlightTone::Note) => {
            "tw:shrink-0 tw:select-none tw:pr-3 tw:pl-2 tw:text-right tw:text-muted-foreground"
        }
    }
}

/// The range's label chip, pushed to the figure's right edge and stuck
/// there through horizontal scrolling.
fn label_class(tone: Option<CodeHighlightTone>) -> &'static str {
    match tone {
        Some(CodeHighlightTone::Note) | None => {
            "tw:sticky tw:right-1 tw:ml-auto tw:shrink-0 tw:select-none tw:rounded-xs tw:border tw:border-border tw:bg-card tw:px-1 tw:text-[0.7rem] tw:text-muted-foreground"
        }
        Some(CodeHighlightTone::Bound) => {
            "tw:sticky tw:right-1 tw:ml-auto tw:shrink-0 tw:select-none tw:rounded-xs tw:border tw:border-status-bound-border tw:bg-card tw:px-1 tw:text-[0.7rem] tw:text-status-bound-foreground"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bound(lines: RangeInclusive<usize>, label: Option<&str>) -> CodeHighlight {
        CodeHighlight {
            lines,
            tone: CodeHighlightTone::Bound,
            label: label.map(str::to_string),
        }
    }

    #[test]
    fn a_trailing_newline_does_not_add_a_row() {
        assert_eq!(figure_rows("a\nb\n", &[]).len(), 2);
        assert_eq!(figure_rows("a\nb", &[]).len(), 2);
        // A blank line in the middle is real content and keeps its number.
        assert_eq!(figure_rows("a\n\nb\n", &[]).len(), 3);
    }

    #[test]
    fn the_label_lands_on_the_ranges_first_line_only() {
        let highlights = [bound(2..=3, Some("the Scale knob"))];
        let rows = figure_rows("a\nb\nc\n", &highlights);
        assert_eq!(rows[0].tone, None);
        assert_eq!(rows[1].tone, Some(CodeHighlightTone::Bound));
        assert_eq!(rows[1].label, Some("the Scale knob"));
        assert_eq!(rows[2].tone, Some(CodeHighlightTone::Bound));
        assert_eq!(rows[2].label, None);
    }

    #[test]
    fn ranges_past_the_end_cover_nothing_extra() {
        let highlights = [bound(2..=99, None)];
        let rows = figure_rows("a\nb\n", &highlights);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].tone, Some(CodeHighlightTone::Bound));
    }

    #[test]
    fn bound_rows_wear_violet_and_never_green() {
        let bound = row_class(Some(CodeHighlightTone::Bound));
        assert!(bound.contains("status-bound-bg"));
        assert!(bound.contains("status-bound-border"));
        assert!(!bound.contains("status-good"));
        assert!(!gutter_class(Some(CodeHighlightTone::Bound)).contains("status-good"));
    }
}
