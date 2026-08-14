//! Live lamp colors as direct DOM writes — a 60Hz feed must cost zero
//! VDOM work.

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
