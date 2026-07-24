//! The status circle: the roster's shape × health indicator.
//!
//! Shape and motion carry meaning without color: solid = live link,
//! hollow = remembered only, pulsing = working. Color rides the existing
//! `bg-status-*` families (green good, amber attention, red broken, gray
//! neutral) — no parallel color vocabulary.
//!
//! Precedence rule (direction.md "Card grammar"): one circle per card,
//! showing the worst ACTIONABLE state; secondary facts (e.g. firmware
//! drift on a running device) demote to chips next to it.

use dioxus::prelude::*;

/// The circle's shape grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusCircleShape {
    /// A live link exists.
    Solid,
    /// Remembered only — no live link.
    Hollow,
    /// Work is in flight.
    Pulsing,
}

/// The circle's health family, mirroring the `bg-status-*` tokens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusCircleTone {
    Neutral,
    Working,
    Good,
    /// Unsaved/edit yellow (node vocabulary — not device health).
    Warning,
    /// Health-attention orange (the roster's amber family).
    Attention,
    Error,
}

/// One status circle. Sized for card headers (8px); purely presentational.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn StatusCircle(shape: StatusCircleShape, tone: StatusCircleTone) -> Element {
    rsx! {
        span { class: status_circle_class(shape, tone) }
    }
}

/// The class string for a shape × tone pairing (exposed so table-like
/// surfaces can reuse the exact treatment without the component).
pub fn status_circle_class(shape: StatusCircleShape, tone: StatusCircleTone) -> String {
    let base = "tw:inline-block tw:h-2 tw:w-2 tw:flex-none tw:rounded-full";
    let paint = match (shape, tone) {
        (StatusCircleShape::Hollow, tone) => hollow_paint(tone),
        (_, tone) => solid_paint(tone),
    };
    let motion = match shape {
        StatusCircleShape::Pulsing => " tw:animate-pulse",
        StatusCircleShape::Solid | StatusCircleShape::Hollow => "",
    };
    format!("{base} {paint}{motion}")
}

fn solid_paint(tone: StatusCircleTone) -> &'static str {
    match tone {
        StatusCircleTone::Neutral => "tw:bg-status-neutral-foreground",
        StatusCircleTone::Working => "tw:bg-status-working-foreground",
        StatusCircleTone::Good => "tw:bg-status-good-foreground",
        StatusCircleTone::Warning => "tw:bg-status-warning-foreground",
        StatusCircleTone::Attention => "tw:bg-status-attention-foreground",
        StatusCircleTone::Error => "tw:bg-status-error-foreground",
    }
}

fn hollow_paint(tone: StatusCircleTone) -> &'static str {
    match tone {
        StatusCircleTone::Neutral => {
            "tw:border tw:border-status-neutral-foreground tw:bg-transparent"
        }
        StatusCircleTone::Working => {
            "tw:border tw:border-status-working-foreground tw:bg-transparent"
        }
        StatusCircleTone::Good => "tw:border tw:border-status-good-foreground tw:bg-transparent",
        StatusCircleTone::Warning => {
            "tw:border tw:border-status-warning-foreground tw:bg-transparent"
        }
        StatusCircleTone::Attention => {
            "tw:border tw:border-status-attention-foreground tw:bg-transparent"
        }
        StatusCircleTone::Error => "tw:border tw:border-status-error-foreground tw:bg-transparent",
    }
}
