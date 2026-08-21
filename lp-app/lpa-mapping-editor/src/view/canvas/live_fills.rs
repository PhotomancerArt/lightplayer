//! Live lamp colors as direct DOM writes — a 60Hz feed must cost zero
//! VDOM work.

/// Write (or clear) per-lamp live colors on the fixture-layer SPRITES:
/// every `[data-sprite-lamp]` circle looks its color up by its sprite's
/// key (`data-sprite-fixture`) and its TRUE lamp index (sprites
/// display-subsample, so the attribute carries the stride-corrected
/// index). An absent feed entry — or an empty map — clears back to the
/// palette, same contract as [`apply_live_fills`].
pub(crate) fn apply_sprite_live_fills(
    canvas_dom_id: &str,
    feeds: &std::collections::BTreeMap<String, Vec<[u8; 3]>>,
) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(lamps) =
            document.query_selector_all(&format!("#{canvas_dom_id} [data-sprite-lamp]"))
        else {
            return;
        };
        use wasm_bindgen::JsCast;
        for slot in 0..lamps.length() {
            let Some(element) = lamps
                .item(slot)
                .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
            else {
                continue;
            };
            let color = element
                .get_attribute("data-sprite-fixture")
                .and_then(|fixture| feeds.get(&fixture))
                .and_then(|colors| {
                    element
                        .get_attribute("data-sprite-lamp")
                        .and_then(|index| index.parse::<usize>().ok())
                        .and_then(|index| colors.get(index))
                });
            match color {
                Some([r, g, b]) => {
                    let _ = element.set_attribute("style", &format!("fill: rgb({r} {g} {b})"));
                }
                None => {
                    let _ = element.remove_attribute("style");
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (canvas_dom_id, feeds);
    }
}

/// Write (or clear) per-lamp live-color overrides as inline styles on the
/// mounted canvas's `[data-lamp]` circles. Direct DOM only — the whole
/// point is that a 60Hz color feed costs zero VDOM work. Inline style beats
/// the `fill` attribute per SVG presentation rules; clearing the style
/// restores the palette without this code knowing what the palette was.
pub(crate) fn apply_live_fills(canvas_dom_id: &str, live_on: bool, colors: &[[u8; 3]]) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(lamps) = document.query_selector_all(&format!("#{canvas_dom_id} [data-lamp]"))
        else {
            return;
        };
        use wasm_bindgen::JsCast;
        for slot in 0..lamps.length() {
            let Some(element) = lamps
                .item(slot)
                .and_then(|node| node.dyn_into::<web_sys::Element>().ok())
            else {
                continue;
            };
            let color = (live_on && !colors.is_empty())
                .then(|| {
                    element
                        .get_attribute("data-lamp")
                        .and_then(|index| index.parse::<usize>().ok())
                        .and_then(|index| colors.get(index))
                })
                .flatten();
            match color {
                Some([r, g, b]) => {
                    let _ = element.set_attribute("style", &format!("fill: rgb({r} {g} {b})"));
                }
                None => {
                    let _ = element.remove_attribute("style");
                }
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (canvas_dom_id, live_on, colors);
    }
}
