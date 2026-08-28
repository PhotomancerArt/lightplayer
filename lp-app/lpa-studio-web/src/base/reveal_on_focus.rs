//! [`use_reveal_on_focus`]: scroll an element into view when it BECOMES
//! focused — the sidebar-click half of selection (click a tree row, the
//! node card is off-screen, nothing appears to happen).
//!
//! Two deliberate restraints:
//!
//! - Edge-triggered, not level-triggered: only a false→true flip of
//!   `focused` after mount scrolls. A pane that mounts already focused
//!   (the load-time default focus) stays put — scrolling on mount would
//!   yank the page around before the user has done anything.
//! - Only when needed: an element that is already MOSTLY visible (at
//!   least half of it, or half of its scroller for elements taller than
//!   the scroller) is left alone. Clicking the card itself also focuses
//!   it; without this check that click would nudge the very thing under
//!   the pointer.

use dioxus::prelude::*;

/// Watch `focused` and scroll the element this hook is mounted on into
/// view (smooth, nearest) when it flips false→true and the element is not
/// already mostly visible in its scroll container.
///
/// Returns the `onmounted` handler to attach to the element.
pub fn use_reveal_on_focus(focused: bool) -> Callback<Event<MountedData>> {
    let mut element = use_signal(|| None::<web_sys::Element>);
    // Seeded with the CURRENT value so the first effect run sees no edge:
    // mounting focused is the programmatic default-focus case, not a user
    // gesture.
    let mut was_focused = use_signal(|| focused);
    use_effect(use_reactive!(|focused| {
        let previously = *was_focused.peek();
        was_focused.set(focused);
        if focused
            && !previously
            && let Some(target) = element.peek().clone()
        {
            reveal_if_mostly_hidden(&target);
        }
    }));
    use_callback(move |event: Event<MountedData>| {
        use dioxus::web::WebEventExt;
        element.set(event.data().try_as_web_event());
    })
}

/// Scroll `target` into view unless it is already mostly visible in its
/// nearest scrollable ancestor. No ancestor scrolls (everything fits) —
/// nothing to do.
fn reveal_if_mostly_hidden(target: &web_sys::Element) {
    let Some(scroller) = scroll_parent(target) else {
        return;
    };
    let rect = target.get_bounding_client_rect();
    let view = scroller.get_bounding_client_rect();
    if mostly_visible(
        (rect.top(), rect.bottom()),
        (view.top(), view.bottom()),
    ) {
        return;
    }
    let options = web_sys::ScrollIntoViewOptions::new();
    options.set_behavior(web_sys::ScrollBehavior::Smooth);
    options.set_block(web_sys::ScrollLogicalPosition::Nearest);
    target.scroll_into_view_with_scroll_into_view_options(&options);
}

/// Nearest ancestor that actually scrolls vertically: overflowing content
/// AND an `overflow-y` that clips (`auto`/`scroll`).
fn scroll_parent(element: &web_sys::Element) -> Option<web_sys::Element> {
    let window = web_sys::window()?;
    let mut current = element.parent_element();
    while let Some(candidate) = current {
        if candidate.scroll_height() > candidate.client_height()
            && let Ok(Some(style)) = window.get_computed_style(&candidate)
            && let Ok(overflow) = style.get_property_value("overflow-y")
            && matches!(overflow.as_str(), "auto" | "scroll")
        {
            return Some(candidate);
        }
        current = candidate.parent_element();
    }
    None
}

/// "Mostly visible": the visible slice covers at least half of the
/// element — or half of the viewport, for elements taller than it (a tall
/// card filling the screen must not count as hidden just because its
/// edges are clipped). Spans are `(top, bottom)` in any shared coordinate
/// space.
fn mostly_visible(element: (f64, f64), viewport: (f64, f64)) -> bool {
    let visible = (element.1.min(viewport.1) - element.0.max(viewport.0)).max(0.0);
    let needed = (element.1 - element.0).min(viewport.1 - viewport.0) * 0.5;
    needed > 0.0 && visible >= needed
}

#[cfg(test)]
mod tests {
    use super::mostly_visible;

    #[test]
    fn fully_visible_element_needs_no_scroll() {
        assert!(mostly_visible((100.0, 300.0), (0.0, 800.0)));
    }

    #[test]
    fn element_clipped_but_over_half_visible_stays_put() {
        // 200-tall card, 120 of it visible above the fold.
        assert!(mostly_visible((680.0, 880.0), (0.0, 800.0)));
    }

    #[test]
    fn element_mostly_below_the_fold_scrolls() {
        // 200-tall card, only 40 visible.
        assert!(!mostly_visible((760.0, 960.0), (0.0, 800.0)));
    }

    #[test]
    fn element_entirely_off_screen_scrolls() {
        assert!(!mostly_visible((900.0, 1100.0), (0.0, 800.0)));
        assert!(!mostly_visible((-300.0, -100.0), (0.0, 800.0)));
    }

    #[test]
    fn tall_element_filling_the_viewport_counts_as_visible() {
        // Card taller than the viewport, clipped on both edges: judged
        // against half the VIEWPORT, not half of itself.
        assert!(mostly_visible((-200.0, 1000.0), (0.0, 800.0)));
    }

    #[test]
    fn tall_element_peeking_in_only_a_sliver_scrolls() {
        // 1000-tall card with 100 visible at the bottom of an 800 viewport.
        assert!(!mostly_visible((700.0, 1700.0), (0.0, 800.0)));
    }

    #[test]
    fn degenerate_spans_never_claim_visibility() {
        assert!(!mostly_visible((100.0, 100.0), (0.0, 800.0)));
        assert!(!mostly_visible((100.0, 300.0), (400.0, 400.0)));
    }
}
