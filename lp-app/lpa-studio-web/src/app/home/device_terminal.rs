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
//! 3. **Untyped and unbounded.** A repeating heartbeat scrolled the panel
//!    forever. The model (P1) now caps at 200 lines, collapses consecutive
//!    repeats into `repeats`, and counts what fell off (`dropped`); this
//!    renderer shows the drop count as the first row.
//!
//! # No fold: every line whole, wrapped (2026-09-04)
//!
//! P5 shipped the spike's long-line fold — anything over 160 characters
//! rendered as a 120-character head plus a "… +N chars" click-to-expand —
//! and the 2026-09-04 bench retired it. The one line that mattered that
//! day, a flash that installed but could not stamp the board manifest, read
//! `…transport error: Transp… +77 chars`. Yona reads this panel by
//! selecting text and pressing Cmd+C, not by clicking rows or a copy
//! button, and the fold broke exactly that: a selection over a folded row
//! put "+77 chars" in the clipboard instead of the reason, and the per-row
//! click handler meant a drag-select that ended on a long row toggled it.
//!
//! So there is no fold and no per-line length bound here at all. Rows wrap
//! whole ([`ROW_BASE`]: `whitespace-pre-wrap` + `break-all`, hanging
//! indent) inside a fixed-height scroll box ([`TERMINAL_CLASS`] +
//! `height_class`), so a long line costs scroll distance and never card
//! height — the device card's "board events never move it" rule
//! (`docs/adr/2026-09-03-device-card-fixed-height-and-disconnect-disappears.md`)
//! holds without a fold — and a selection copies exactly what the board
//! said. The model bounds the COUNT (`TERMINAL_CAP` in `lpa-devices`), and
//! every summary it mints (`wire_summary`, an outcome's `summary()`) is
//! bounded by construction; a board printing a multi-kilobyte line with no
//! newline is the one input that could fill the box, and if it ever
//! happens the answer is a model-side bound on what the fold keeps, not a
//! render-side fold that lies to the clipboard.
//! See `docs/adr/2026-09-04-device-terminal-never-folds-a-line.md`.
//!
//! # One box, and flush inside it (D3; G1 2026-09-03)
//!
//! The terminal is content directly on the card's `bg-terminal` ground — no
//! inner rounded/bordered sub-panel. The old `terminal_class()` in
//! `device_roster_card.rs` drew its own box inside the zone's box; that is
//! exactly the nesting the card's "one box" rule (AC1) forbids, so this
//! component drops it.
//!
//! G1 took that one step further: the ground has to reach the card's own
//! edges. So this block carries neither zone padding nor a hairline — it is
//! the last block of the card's FIRMWARE zone, which owns the separator
//! above the pair (see [`ZONE_CLASS`]).
//!
//! # Pinning
//!
//! `pinned` tracks whether the user was already at the bottom
//! (`scrollTop + clientHeight >= scrollHeight - PIN_THRESHOLD_PX`) the last
//! time they scrolled — read on every `onscroll`. Two triggers keep the
//! scroll position honest, both funnelling through `scroll_to_bottom`:
//!
//! - `onmounted` fires the scroll once the element exists, and again on a
//!   short ladder of ticks (`PIN_RETRY_DELAYS_MS`) — the element has no
//!   height limit until the stylesheet applies, so the very first write is
//!   a no-op; this is what actually satisfies "pinned on load" for a
//!   screen that never re-renders after its first paint (a story, a card
//!   that opens once and sits idle).
//! - `use_after_render` re-checks `pinned` on every later render (a render
//!   triggered by new lines arriving, most commonly) and repeats the
//!   scroll — the same shape as `core::log_list::LogList`'s autoscroll,
//!   but writing `scrollTop` on the DOM element directly (see
//!   `scroll_to_bottom` for why).
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

use std::rc::Rc;

use dioxus::prelude::dioxus_core::use_after_render;
use dioxus::prelude::*;
use lpa_studio_core::{DeviceTerminalKind, DeviceTerminalLine};

/// How close to the bottom still counts as pinned.
const PIN_THRESHOLD_PX: f64 = 4.0;
/// The mount-time pin ladder — see [`scroll_to_bottom`] for why one write
/// at mount is not enough.
const PIN_RETRY_DELAYS_MS: [u32; 4] = [0, 50, 250, 1000];

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
                            TerminalRow { key: "{index}", line: line.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// One terminal row: kind tag/marker, the whole text (wrapped, never
/// folded — see the module doc), and a `×N` repeat badge. Deliberately no
/// click handler: a row that reacts to a click is a row a drag-select can
/// trip over.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn TerminalRow(line: DeviceTerminalLine) -> Element {
    let class = format!("{ROW_BASE} {}", kind_class(line.kind));

    rsx! {
        p { class,
            if matches!(line.kind, DeviceTerminalKind::Wire) {
                span { class: "tw:mr-1 tw:text-dim-foreground", "wire" }
            }
            if matches!(line.kind, DeviceTerminalKind::Studio) {
                span { class: "tw:opacity-70", "▸ " }
            }
            "{line.text}"
            if line.repeats > 1 {
                span { class: REPEAT_BADGE_CLASS, "×{line.repeats}" }
            }
        }
    }
}

/// The terminal's block: NO padding and NO hairline of its own (G1
/// 2026-09-03, P9).
///
/// The terminal is the last block of the card's FIRMWARE zone — the same
/// subject, said twice: what firmware is on this board, and what that
/// firmware is saying. So the zone above it owns the separator, and there is
/// deliberately none between the firmware's verb row and this ground.
///
/// No padding either, because the ground is meant to reach the card's own
/// edges: a dark band across the card rather than a dark panel sitting
/// inside a lighter one. The reading inset lives on the ground itself
/// ([`TERMINAL_CLASS`]'s `px-2 py-1.5`), so text never touches the edge.
const ZONE_CLASS: &str = "tw:grid tw:min-w-0";

/// The terminal ground itself. Deliberately no border/rounded/background
/// sub-frame (see the module doc's "One box" section) — just the card's own
/// `bg-terminal` with block-flow children, which is what gives natural
/// (oldest-first) DOM order for free.
const TERMINAL_CLASS: &str = "tw:overflow-y-auto tw:overflow-x-hidden tw:bg-terminal tw:px-2 tw:py-1.5 tw:font-mono tw:text-[10.5px] tw:leading-[1.45] tw:text-muted-foreground";

const COPY_BUTTON_CLASS: &str = "tw:absolute tw:right-1 tw:top-1 tw:z-10 tw:h-5 tw:cursor-pointer tw:appearance-none tw:rounded tw:border tw:border-border-muted tw:bg-terminal/85 tw:px-1.5 tw:text-[10px] tw:leading-5 tw:text-subtle-foreground tw:transition-colors tw:hover:text-strong-foreground";

/// Every row wraps whole: `whitespace-pre-wrap` + `break-all` is what lets
/// a 200-character reason (or a 400-character block-plan dump) read in
/// full and copy in full, and the hanging indent (`pl-2.5` + `-10px`
/// text-indent, spike parity) sits a wrapped line's continuation rows flush
/// under its first character rather than under the kind marker.
const ROW_BASE: &str =
    "tw:m-0 tw:whitespace-pre-wrap tw:break-all tw:pl-2.5 tw:[text-indent:-10px]";

const DROPPED_ROW_CLASS: &str = "tw:m-0 tw:mb-1 tw:pl-2.5 tw:[text-indent:-10px] tw:border-b tw:border-dashed tw:border-border-muted tw:pb-1 tw:italic tw:text-dim-foreground";

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

/// The whole tail as plain text for the copy button: one line per row,
/// every line whole, `×N` appended for a repeat. The dropped-count notice
/// is UI chrome, not a line the board or Studio said, so it is not
/// included. (Selecting the panel and pressing Cmd+C copies the same text,
/// which is the point of never folding a row.)
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

/// Scroll `element` to its full height, through the DOM element itself,
/// now and again on a short ladder of ticks.
///
/// The ladder is the whole fix for "unpinned on load" (P7's CDP check):
/// at `onmounted` — and still at a 0 ms tick — the box has NO height limit
/// yet (`clientHeight == scrollHeight`, thousands of px) because the
/// stylesheet carrying `h-40` has not applied to the fresh node, so
/// `scrollTop = scrollHeight` is a no-op. By ~50 ms the class lands, the
/// box is 160 px tall, and the same write pins it. Later ticks cover a
/// slow first paint. Writes past the first are idempotent while pinned:
/// `scrollTop` clamps to the maximum.
///
/// Synchronous `web_sys` writes rather than Dioxus's async
/// `get_scroll_size` + `scroll` round trip (`core::log_list::LogList`'s
/// shape): the round trip never landed on this panel in headless capture.
fn scroll_to_bottom(element: Rc<MountedData>) {
    let Some(element) = element.downcast::<web_sys::Element>().cloned() else {
        return;
    };
    element.set_scroll_top(element.scroll_height());
    spawn(async move {
        for delay in PIN_RETRY_DELAYS_MS {
            gloo_timers::future::TimeoutFuture::new(delay).await;
            element.set_scroll_top(element.scroll_height());
        }
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

    /// No fold (2026-09-04): a long line wraps whole inside the scroll box,
    /// so a selection copies what the board said and the card's height rule
    /// is carried by the box's fixed height, not by clipping rows. Guards
    /// the two classes that decision lives in: rows must wrap (and never
    /// clip, ellipsise, or line-clamp), and the box must scroll.
    #[test]
    fn rows_wrap_whole_inside_a_scrolling_box() {
        for wrap in ["tw:whitespace-pre-wrap", "tw:break-all"] {
            assert!(ROW_BASE.contains(wrap), "{ROW_BASE}");
        }
        for clip in [
            "truncate",
            "line-clamp",
            "overflow-hidden",
            "text-ellipsis",
            "nowrap",
        ] {
            assert!(!ROW_BASE.contains(clip), "{ROW_BASE} clips with {clip}");
        }
        assert!(
            TERMINAL_CLASS.contains("tw:overflow-y-auto"),
            "{TERMINAL_CLASS}"
        );
    }

    #[test]
    fn copy_tail_keeps_a_long_line_whole() {
        let long = "x".repeat(400);
        let lines = vec![line(DeviceTerminalKind::Failure, &long, 1)];
        assert_eq!(copy_tail(&lines), long);
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

    /// FLUSH (G1 2026-09-03): the ground reaches the card's edges, so the
    /// block carries no padding and no hairline of its own — the firmware
    /// zone above owns the separator — and the ground itself keeps only the
    /// small reading inset that stops text touching the edge.
    #[test]
    fn the_terminal_is_flush_with_the_cards_edges() {
        assert!(!ZONE_CLASS.contains("px-"), "{ZONE_CLASS}");
        assert!(!ZONE_CLASS.contains("py-"), "{ZONE_CLASS}");
        assert!(!ZONE_CLASS.contains("border"), "{ZONE_CLASS}");
        // No frame of its own either: the ground is the card's, undecorated.
        assert!(!TERMINAL_CLASS.contains("rounded"), "{TERMINAL_CLASS}");
        assert!(!TERMINAL_CLASS.contains("border"), "{TERMINAL_CLASS}");
        assert!(TERMINAL_CLASS.contains("tw:px-2"), "{TERMINAL_CLASS}");
        assert!(TERMINAL_CLASS.contains("tw:py-1.5"), "{TERMINAL_CLASS}");
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
