//! The device card's terminal: what the board actually said, typed and
//! pinned. Renders straight off [`lpa_studio_core::DeviceTerminalLine`]
//! (P1 of the device-card-v2 plan) — no view model in between.
//!
//! Three defects the old `TerminalPanel` (a flat `Vec<String>`, still in
//! `device_roster_card.rs` until the swap) carried, fixed here:
//!
//! 1. **Reversed DOM order.** `TerminalPanel` was `flex-col-reverse` over
//!    `.rev()`-iterated lines, so `Ctrl+F`, a screen reader, or a
//!    drag-select all walked the log bottom-up — a paste came out
//!    newest-first. This renders rows in natural (oldest-first) DOM order
//!    inside a plain `overflow-y-auto` box and pins to the bottom in script
//!    instead of in markup.
//! 2. **No contrast.** Every line wore the same `text-subtle-foreground`;
//!    a fault read exactly like a heartbeat. Lines are typed
//!    ([`DeviceTerminalKind`]) and coloured against the Aurora status
//!    tokens (`kind_class`).
//! 3. **Untyped, unfolded, unbounded.** A multi-hundred-character block-plan
//!    dump used to just sit there and a repeating heartbeat scrolled the
//!    panel forever. The model (P1) now caps at 200 lines, collapses
//!    consecutive repeats into `repeats`, and counts what fell off
//!    (`dropped`); this renderer folds anything still over 160 characters
//!    behind a click-to-expand and shows the drop count as the first row.
//!
//! # One box (D3)
//!
//! The terminal is the zone's content directly on the card's
//! `bg-terminal` ground — no inner rounded/bordered sub-panel. The old
//! `terminal_class()` in `device_roster_card.rs` drew its own box inside
//! the zone's box; that is exactly the nesting the card's "one box" rule
//! (AC1) forbids, so this component drops it.
//!
//! # Pinning
//!
//! `pinned` tracks whether the user was already at the bottom
//! (`scrollTop + clientHeight >= scrollHeight - PIN_THRESHOLD_PX`) the last
//! time they scrolled — read on every `onscroll`. Two triggers keep the
//! scroll position honest, both funnelling through `scroll_to_bottom`:
//!
//! - `onmounted` fires the scroll immediately once the element exists —
//!   this is what actually satisfies "pinned on load" for a screen that
//!   never re-renders after its first paint (a story, a card that opens
//!   once and sits idle).
//! - `use_after_render` re-checks `pinned` on every later render (a render
//!   triggered by new lines arriving, most commonly) and repeats the
//!   scroll — this is `core::log_list::LogList`'s own autoscroll primitive,
//!   reused rather than adding a second pinning mechanism to the crate.
//!
//! `use_after_render` alone is not enough: its callback runs as part of the
//! render pass, before `onmounted` has necessarily fired for a
//! freshly-mounted element, so relying on it exclusively left the very
//! first paint unpinned until some second render happened to occur — caught
//! by the P5 CDP check (`scrollTop` was 0 on load) rather than by the unit
//! tests, which cover the pure `is_pinned_to_bottom` math, not the mount
//! timing. A reader who has scrolled up to read history is left alone
//! either way, since both triggers no-op once `pinned` goes false. (The
//! plan describes this as "an effect on the line count"; the render-time
//! half above is that effect, plus the mount-time half needed to cover a
//! screen with no second render.)
//!
//! # No `raw` button (follow-up)
//!
//! The spike's terminal bar carried a second `raw` button toggling wire
//! rows to the frame bytes. [`DeviceTerminalLine`] only ever carries the
//! decoded summary — the model has no raw-frame field (P1) — so inventing
//! one here would mean showing a lie. Dropped; a follow-up would add a raw
//! frame capture to the model first.

use std::collections::HashSet;
use std::rc::Rc;

use dioxus::prelude::*;
use dioxus::{html::geometry::PixelsVector2D, prelude::dioxus_core::use_after_render};
use lpa_studio_core::{DeviceTerminalKind, DeviceTerminalLine};

/// How close to the bottom still counts as pinned.
const PIN_THRESHOLD_PX: f64 = 4.0;

/// A line renders folded once it is longer than this (spike parity).
const FOLD_AT_CHARS: usize = 160;
/// How much of a folded line shows ahead of the "… +N chars" control.
const FOLD_HEAD_CHARS: usize = 120;

/// The card's terminal zone. `height_class` is the panel's fixed height —
/// `tw:h-40` on a device card, the shorter `tw:h-24` on a pending link,
/// which has far less to say (`device_roster_card.rs`).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub(crate) fn DeviceTerminal(
    lines: Vec<DeviceTerminalLine>,
    dropped: u32,
    height_class: &'static str,
) -> Element {
    let mut expanded = use_signal(HashSet::<usize>::new);
    let mut terminal_element = use_signal(|| None::<Rc<MountedData>>);
    let mut pinned = use_signal(|| true);

    // `after_render` hooks fire once per render OF THIS SCOPE, but they run
    // as part of the render pass itself — before `onmounted` has necessarily
    // fired for a freshly-inserted element. On the very first render there
    // is nothing yet in `terminal_element`, and a story (or any screen that
    // never gets a second render) would then never scroll at all. So the
    // mount handler below ALSO kicks off the same scroll directly, which is
    // what actually satisfies "pinned on load"; this effect covers every
    // later render (new lines arriving while pinned).
    use_after_render(move || {
        if !pinned() {
            return;
        }
        let Some(element) = terminal_element.read().as_ref().cloned() else {
            return;
        };
        scroll_to_bottom(element);
    });

    let tail = copy_tail(&lines);
    let is_empty = lines.is_empty() && dropped == 0;

    rsx! {
        section { class: ZONE_CLASS,
            div { class: "ux-armed-dim tw:relative tw:min-w-0 tw:grid",
                button {
                    class: COPY_BUTTON_CLASS,
                    r#type: "button",
                    title: "Copy the whole tail to the clipboard.",
                    onclick: move |event| {
                        event.stop_propagation();
                        crate::clipboard::write_text(&tail);
                    },
                    "copy"
                }
                div {
                    class: "{TERMINAL_CLASS} {height_class}",
                    onmounted: move |event| {
                        let element = event.data();
                        terminal_element.set(Some(element.clone()));
                        if pinned() {
                            scroll_to_bottom(element);
                        }
                    },
                    onscroll: move |event| {
                        pinned
                            .set(
                                is_pinned_to_bottom(
                                    event.scroll_top(),
                                    event.scroll_height(),
                                    event.client_height(),
                                ),
                            );
                    },
                    if is_empty {
                        p { class: "tw:m-0 tw:opacity-60", "Nothing from this board yet." }
                    } else {
                        if dropped > 0 {
                            p { class: "{DROPPED_ROW_CLASS}",
                                "… {dropped} earlier lines not shown"
                            }
                        }
                        for (index , line) in lines.iter().enumerate() {
                            TerminalRow {
                                key: "{index}",
                                line: line.clone(),
                                is_expanded: expanded.read().contains(&index),
                                on_toggle: move |()| {
                                    expanded
                                        .with_mut(|set| {
                                            if !set.remove(&index) {
                                                set.insert(index);
                                            }
                                        });
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One terminal row: kind tag/marker, text (folded past
/// [`FOLD_AT_CHARS`]), and a `×N` repeat badge.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TerminalRow(
    line: DeviceTerminalLine,
    is_expanded: bool,
    on_toggle: EventHandler<()>,
) -> Element {
    let fold = fold_text(&line.text);
    let is_long = fold.is_some();
    let class = if is_long {
        format!("{ROW_BASE} tw:cursor-pointer {}", kind_class(line.kind))
    } else {
        format!("{ROW_BASE} {}", kind_class(line.kind))
    };

    rsx! {
        p {
            class,
            onclick: move |_| {
                if is_long {
                    on_toggle.call(());
                }
            },
            if matches!(line.kind, DeviceTerminalKind::Wire) {
                span { class: "tw:mr-1 tw:text-dim-foreground", "wire" }
            }
            if matches!(line.kind, DeviceTerminalKind::Studio) {
                span { class: "tw:opacity-70", "▸ " }
            }
            if let Some((head, remaining)) = fold {
                if is_expanded {
                    "{line.text} "
                    span { class: FOLD_CONTROL_CLASS, "less" }
                } else {
                    "{head}… "
                    span { class: FOLD_CONTROL_CLASS, "+{remaining} chars" }
                }
            } else {
                "{line.text}"
            }
            if line.repeats > 1 {
                span { class: REPEAT_BADGE_CLASS, "×{line.repeats}" }
            }
        }
    }
}

/// The zone treatment (full-bleed hairline + own padding), copied verbatim
/// from `device_roster_card.rs`'s `zone_class(false)` — that function is
/// private and this file must not edit its module, so the literal is
/// duplicated rather than imported. Keep the two in sync by hand.
const ZONE_CLASS: &str =
    "tw:grid tw:min-w-0 tw:gap-2 tw:border-t tw:border-border-strong tw:px-4 tw:py-3";

/// The terminal ground itself. Deliberately no border/rounded/background
/// sub-frame (see the module doc's "One box" section) — just the card's own
/// `bg-terminal` with block-flow children, which is what gives natural
/// (oldest-first) DOM order for free.
const TERMINAL_CLASS: &str = "tw:overflow-y-auto tw:overflow-x-hidden tw:bg-terminal tw:px-2 tw:py-1.5 tw:font-mono tw:text-[10.5px] tw:leading-[1.45] tw:text-muted-foreground";

const COPY_BUTTON_CLASS: &str = "tw:absolute tw:right-1 tw:top-1 tw:z-10 tw:h-5 tw:cursor-pointer tw:appearance-none tw:rounded tw:border tw:border-border-muted tw:bg-terminal/85 tw:px-1.5 tw:text-[10px] tw:leading-5 tw:text-subtle-foreground tw:transition-colors tw:hover:text-strong-foreground";

/// Hanging indent (`pl-2.5` + `-10px` text-indent, spike parity) so a
/// wrapped long line's continuation lines sit flush under the first
/// character rather than under the kind marker.
const ROW_BASE: &str =
    "tw:m-0 tw:whitespace-pre-wrap tw:break-all tw:pl-2.5 tw:[text-indent:-10px]";

const DROPPED_ROW_CLASS: &str = "tw:m-0 tw:mb-1 tw:pl-2.5 tw:[text-indent:-10px] tw:border-b tw:border-dashed tw:border-border-muted tw:pb-1 tw:italic tw:text-dim-foreground";

const FOLD_CONTROL_CLASS: &str = "tw:text-subtle-foreground tw:underline tw:decoration-dotted";

const REPEAT_BADGE_CLASS: &str = "tw:ml-1.5 tw:inline-block tw:rounded tw:border tw:border-border-muted tw:px-1 tw:align-baseline tw:text-[9.5px] tw:text-dim-foreground";

/// Kind → Aurora status token (D1 in the plan). Bound = violet, never
/// green, per the studio-wide convention — Studio's own narration reads as
/// "bound to this device" rather than as an outcome.
fn kind_class(kind: DeviceTerminalKind) -> &'static str {
    match kind {
        DeviceTerminalKind::Rom => "tw:text-dim-foreground",
        DeviceTerminalKind::Board => "tw:text-muted-foreground",
        DeviceTerminalKind::Wire => "tw:text-status-live-foreground",
        DeviceTerminalKind::Studio => "tw:text-status-bound-foreground",
        DeviceTerminalKind::Outcome => "tw:text-status-good-foreground",
        DeviceTerminalKind::Failure => "tw:text-status-error-foreground",
        DeviceTerminalKind::Recovery => "tw:text-status-warning-foreground",
    }
}

/// Split `text` into a fold head plus the remaining character count, when
/// it is long enough to fold. `None` means render it whole.
fn fold_text(text: &str) -> Option<(String, usize)> {
    let char_count = text.chars().count();
    if char_count <= FOLD_AT_CHARS {
        return None;
    }
    let head: String = text.chars().take(FOLD_HEAD_CHARS).collect();
    Some((head, char_count - FOLD_HEAD_CHARS))
}

/// The whole tail as plain text for the copy button: one line per row,
/// `×N` appended for a repeat, folded lines copied in full regardless of
/// their expand state. The dropped-count notice is UI chrome, not a line
/// the board or Studio said, so it is not included.
fn copy_tail(lines: &[DeviceTerminalLine]) -> String {
    lines
        .iter()
        .map(|line| {
            if line.repeats > 1 {
                format!("{} ×{}", line.text, line.repeats)
            } else {
                line.text.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_pinned_to_bottom(scroll_top: f64, scroll_height: i32, client_height: i32) -> bool {
    f64::from(scroll_height) - scroll_top - f64::from(client_height) <= PIN_THRESHOLD_PX
}

/// Scroll `element` to its full height. Fire-and-forget, like every
/// best-effort browser-edge call in this crate: a `get_scroll_size`/`scroll`
/// rejection just means one paint stays unpinned, not a hard failure.
fn scroll_to_bottom(element: Rc<MountedData>) {
    spawn(async move {
        let Ok(scroll_size) = element.get_scroll_size().await else {
            return;
        };
        let coordinates = PixelsVector2D::new(0.0, scroll_size.height);
        let _ = element.scroll(coordinates, ScrollBehavior::Instant).await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(kind: DeviceTerminalKind, text: &str, repeats: u32) -> DeviceTerminalLine {
        DeviceTerminalLine {
            kind,
            text: text.to_string(),
            repeats,
        }
    }

    #[test]
    fn fold_text_leaves_short_lines_alone() {
        assert_eq!(fold_text("hello"), None);
        assert_eq!(fold_text(&"x".repeat(160)), None);
    }

    #[test]
    fn fold_text_splits_long_lines_at_the_head_length() {
        let text = "x".repeat(161);
        let (head, remaining) = fold_text(&text).expect("161 chars folds");
        assert_eq!(head.chars().count(), FOLD_HEAD_CHARS);
        assert_eq!(remaining, 161 - FOLD_HEAD_CHARS);
    }

    #[test]
    fn fold_text_counts_chars_not_bytes() {
        // Multi-byte characters must not panic a byte-offset slice and must
        // count as one character each.
        let text = "é".repeat(200);
        let (head, remaining) = fold_text(&text).expect("200 chars folds");
        assert_eq!(head.chars().count(), FOLD_HEAD_CHARS);
        assert_eq!(remaining, 200 - FOLD_HEAD_CHARS);
    }

    #[test]
    fn copy_tail_joins_lines_and_appends_repeat_counts() {
        let lines = vec![
            line(DeviceTerminalKind::Rom, "ESP-ROM:esp32c6-20220919", 1),
            line(DeviceTerminalKind::Wire, "heartbeat · 43 fps", 6),
        ];
        assert_eq!(
            copy_tail(&lines),
            "ESP-ROM:esp32c6-20220919\nheartbeat · 43 fps ×6"
        );
    }

    #[test]
    fn copy_tail_of_empty_lines_is_empty() {
        assert_eq!(copy_tail(&[]), "");
    }

    #[test]
    fn is_pinned_to_bottom_matches_within_threshold() {
        // scrollTop + clientHeight == scrollHeight exactly.
        assert!(is_pinned_to_bottom(100.0, 260, 160));
        // 4px shy is still pinned.
        assert!(is_pinned_to_bottom(96.0, 260, 160));
        // 5px shy is not.
        assert!(!is_pinned_to_bottom(95.0, 260, 160));
    }

    #[test]
    fn kind_class_covers_every_kind() {
        for kind in [
            DeviceTerminalKind::Rom,
            DeviceTerminalKind::Board,
            DeviceTerminalKind::Wire,
            DeviceTerminalKind::Studio,
            DeviceTerminalKind::Outcome,
            DeviceTerminalKind::Failure,
            DeviceTerminalKind::Recovery,
        ] {
            assert!(!kind_class(kind).is_empty());
        }
    }
}
