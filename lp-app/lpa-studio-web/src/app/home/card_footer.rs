//! The slim glass footer a gallery card wears over its art.
//!
//! The card-overlay redesign (spike `spikes/card-overlay/`, gate-ruled
//! 2026-08-25/26) makes the 4:3 preview the whole card face; this footer
//! is the only chrome on it — a FIXED, SHALLOW frosted bar: one title
//! row (with the status compressed to small glyphs) and at most one
//! context line. Everything deeper — the status in words, the actions —
//! lives behind the card's ⋯ popup, never on the face and never on
//! hover: hover's one job on a card is starting the live preview, and
//! nothing may cover what it just paid for.
//!
//! Anchored `bottom-0` to the card, which after the redesign IS the art
//! box — and structurally capped at title + one line, so it can never
//! grow over the preview the way a meta stack would (the spike's
//! round-2 lesson).

use dioxus::prelude::*;

use crate::base::{StudioIcon, StudioIconName};

/// One status glyph on the footer's title row: an icon chip whose
/// meaning-in-words rides the tooltip (and, in full, the card's popup).
#[derive(Clone, PartialEq)]
pub(crate) struct CardStatusGlyph {
    pub icon: StudioIconName,
    pub tone: GlyphTone,
    /// The words the glyph compresses — tooltip text.
    pub words: String,
}

/// Status-color vocabulary (the studio conventions: green = good,
/// blue `status-working` = live/working, amber = needs attention).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum GlyphTone {
    Good,
    Live,
    Attention,
}

/// The single context line under the title, colored by kind.
#[derive(Clone, PartialEq)]
pub(crate) struct CardContextLine {
    pub text: String,
    pub tone: ContextTone,
}

/// Only attention-grade tones exist: quiet facts never earn a context
/// line on the face (they live in the ⋯ popup), so there is no muted
/// variant to reach for.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ContextTone {
    /// In-flight ("Opening…").
    Working,
    /// Honest bad content (a blocked card's headline) — amber, matching
    /// the card's border.
    Attention,
}

/// The footer. Absolutely positioned over the card's bottom edge; the
/// card must be `position: relative` with `overflow: hidden` (both
/// cards already are).
///
/// Stacking/clicks: the bar's `backdrop-filter` forces a stacking
/// context, so a z-index INSIDE the bar can never rise past the card's
/// stretched open link on its own — the bar itself must sit above the
/// link (`z-[2]`). It stays click-transparent (`pointer-events-none`)
/// so the face remains a door everywhere except the controls, which
/// re-enable their own pointer events: the ⋯ menu takes real clicks,
/// and the glyph cluster takes hover for its tooltips (a click landing
/// on a 17px glyph deliberately does nothing rather than mis-opening
/// the card).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn CardGlassFooter(
    title: String,
    #[props(default)] context: Option<CardContextLine>,
    #[props(default)] glyphs: Vec<CardStatusGlyph>,
    /// The title row's trailing control — the ⋯ menu lives IN the bar
    /// (G1 feedback 2026-08-26: floating on the art took space from the
    /// picture). Rises above the stretched open link so it stays
    /// clickable; negative margins keep the 24px trigger from
    /// inflating the slim bar.
    #[props(default)]
    trailing: Option<Element>,
    /// Quiet facts revealed while the CARD is hovered (the card must
    /// carry `tw:group`): the bar slides up ~two lines to show them,
    /// after a short delay so sweeping the pointer across the grid
    /// doesn't pump every bar. Hover is also when the live preview
    /// plays — the reveal is capped at a couple of lines precisely so
    /// it never covers the motion the hover just paid for (the spike's
    /// concept-D lesson). Enhancement only: touch and keyboard never
    /// see it, and the ⋯ popup remains the canonical depth.
    #[props(default)]
    reveal: Option<Element>,
    /// Live content for the context slot (the example card's opening
    /// progress line) — rendered after the static line, if any.
    children: Element,
) -> Element {
    rsx! {
        div { class: "tw:pointer-events-none tw:absolute tw:inset-x-0 tw:bottom-0 tw:z-[2] tw:border-t tw:border-white/5 tw:bg-[rgba(13,17,21,0.68)] tw:px-2.5 tw:pt-1.5 tw:pb-2 tw:backdrop-blur-[10px] tw:backdrop-saturate-[1.15]",
            div { class: "tw:flex tw:items-center tw:gap-1.5",
                p { class: "tw:m-0 tw:min-w-0 tw:flex-1 tw:truncate tw:text-[13px]/[17px] tw:font-semibold tw:text-strong-foreground",
                    "{title}"
                }
                if !glyphs.is_empty() {
                    span { class: "tw:pointer-events-auto tw:flex tw:flex-none tw:gap-1",
                        for glyph in glyphs {
                            span {
                                class: "tw:inline-flex tw:h-[17px] tw:w-[17px] tw:items-center tw:justify-center tw:rounded tw:bg-black/40 {glyph_tone_class(glyph.tone)}",
                                title: "{glyph.words}",
                                StudioIcon { name: glyph.icon, size: 11 }
                            }
                        }
                    }
                }
                if let Some(trailing) = trailing {
                    span { class: "tw:pointer-events-auto tw:-my-1 tw:-mr-1 tw:flex-none",
                        {trailing}
                    }
                }
            }
            if let Some(line) = context {
                p { class: "tw:m-0 tw:mt-px tw:truncate tw:text-xs {context_tone_class(line.tone)}",
                    "{line.text}"
                }
            }
            if let Some(reveal) = reveal {
                // 0fr → 1fr grid-row animation: height animates without
                // max-height guesses; the bar is bottom-anchored, so
                // growing content slides its top edge UP over the art.
                div { class: "tw:grid tw:grid-rows-[0fr] tw:opacity-0 tw:transition-[grid-template-rows,opacity] tw:delay-150 tw:duration-200 tw:group-hover:grid-rows-[1fr] tw:group-hover:opacity-100",
                    div { class: "tw:min-h-0 tw:overflow-hidden",
                        div { class: "tw:grid tw:gap-0.5 tw:pt-1",
                            {reveal}
                        }
                    }
                }
            }
            {children}
        }
    }
}

fn glyph_tone_class(tone: GlyphTone) -> &'static str {
    match tone {
        GlyphTone::Good => "tw:text-status-good-foreground",
        GlyphTone::Live => "tw:text-status-working-foreground",
        GlyphTone::Attention => "tw:text-status-attention-foreground",
    }
}

fn context_tone_class(tone: ContextTone) -> &'static str {
    match tone {
        ContextTone::Working => "tw:text-status-working-foreground",
        ContextTone::Attention => "tw:font-semibold tw:text-status-attention-foreground",
    }
}
