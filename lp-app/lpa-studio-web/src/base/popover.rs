use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::base::outline::{OutlineRect, merged_outline_path};
use crate::base::{StudioIcon, StudioIconName};

static NEXT_POPOVER_ID: AtomicUsize = AtomicUsize::new(1);

const POPOVER_MARGIN_PX: f64 = 12.0;
const POPOVER_BORDER_WIDTH_PX: f64 = 1.0;
const POPOVER_CORNER_RADIUS_PX: f64 = 8.0;
const FALLBACK_PANEL_WIDTH_PX: f64 = 280.0;
const FALLBACK_PANEL_HEIGHT_PX: f64 = 180.0;
const MEASURE_RETRY_LIMIT: u8 = 3;
const STABILIZE_MEASURE_DELAYS_MS: [i32; 2] = [50, 250];
const OPEN_ANIM_MS: f64 = 160.0;
const CLOSE_ANIM_MS: f64 = 120.0;
/// The outline swells this much around the trigger while open ("diving in").
const TRIGGER_INFLATE_PX: f64 = 3.0;
/// Paint-only override on the in-flow placeholder while attached. Also the
/// marker [`trigger_rect_by_id`] keys its un-pinning on.
const TRIGGER_PLACEHOLDER_CLASS: &str = "ux-popover-trigger-placeholder";
/// Panel content starts fading in after this fraction of the open timeline.
const CONTENT_FADE_DELAY: f64 = 0.10;

/// Context handle letting popover CONTENT close its enclosing popover —
/// menu-style consumers (the add-node picker) close on selection. Provided
/// by [`PopoverButton`] to its panel subtree; content that may render
/// outside a popover consumes it with `try_consume_context`.
#[derive(Clone, Copy)]
pub struct PopoverCloseHandle {
    open: Signal<bool>,
}

impl PopoverCloseHandle {
    /// Close the enclosing popover (the normal close animation runs).
    pub fn close(&mut self) {
        self.open.set(false);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverPlacement {
    TopStart,
    TopMiddle,
    TopEnd,
    BottomStart,
    BottomMiddle,
    BottomEnd,
}

/// A popover with an arbitrary trigger. The `trigger` element becomes the
/// content of the toggle button; `class`/`open_class` style that button. The
/// panel floats in the browser top layer, so it escapes any `overflow` on the
/// trigger's ancestors. Use [`IconPopoverButton`] when the trigger is just an
/// icon.
///
/// While open, trigger and panel share one contiguous border: a single SVG
/// path — the rounded union of their rects (see [`crate::base::outline`]) —
/// draws the merged fill, border, and shadow in the top layer. Because the
/// top layer paints above everything, the trigger's visual re-parents into
/// it while open; the in-flow button stays as an invisible placeholder
/// holding layout and keyboard focus. The placeholder keeps its open-state
/// classes AND its content (painted at `opacity: 0`, size additionally
/// pinned inline): an emptied button's baseline synthesizes at its bottom
/// edge instead of the trigger glyph's text baseline, which grew the
/// surrounding line box by the strut descent and reflowed the page a few
/// pixels every time a popover opened. Triggers must therefore stay
/// presentational (icons/text) — the subtree renders twice while open.
/// Opening animates by
/// interpolating the panel's input rect and re-unioning each frame
/// (`prefers-reduced-motion` jumps to the settled shape). Decision record:
/// `docs/adr/2026-07-15-popover-svg-merged-outline.md`.
///
/// **Anchored mode** (`anchor_id` + `anchor_visual`, node-card P2c item 3:
/// "the control IS the trigger"): the merged outline anchors on an EXTERNAL
/// in-flow element — `anchor_id` names it — instead of the trigger button's
/// own rect, and `anchor_visual` (a live copy of that element's content)
/// renders in the top layer over the anchor while open. The in-flow trigger
/// button (e.g. a corner ⓘ hint inside the anchor) still toggles, and hides
/// behind the existing placeholder treatment while open — without the size
/// pin, since the measured rect is the anchor's, not the button's. Clicks
/// inside the anchored visual do NOT dismiss (it hosts interactive
/// controls); the backdrop and the trigger still close.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PopoverButton(
    class: String,
    open_class: String,
    trigger: Element,
    label: String,
    title: String,
    popup_class: String,
    #[props(default = String::new())] chrome_class: String,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] initially_open: bool,
    /// Anchored mode: id of the in-flow element whose rect anchors the
    /// merged outline (instead of the trigger button's own rect).
    #[props(default = None)]
    anchor_id: Option<String>,
    /// Anchored mode: the top-layer visual painted over the anchor while
    /// open — a live copy of the anchored element's content.
    #[props(default = None)]
    anchor_visual: Option<Element>,
    /// The trigger's content has real internal layout (icon **plus** label,
    /// not a lone glyph): the top-layer copy then keeps the trigger's own
    /// box — padding, display, and track sizes — instead of the default
    /// centered-glyph treatment, so nothing shifts when the popover opens.
    #[props(default = false)]
    layer_keeps_layout: bool,
    /// Lock the panel's width to the anchor's VISIBLE width (the measured
    /// rect plus the open-state inflate), overriding any width in
    /// `popup_class`. For anchored popovers that read as the control's own
    /// body — a panel a-few-px narrower than its anchor reads as a mistake,
    /// and welding only one edge leaves a shelf on the other.
    #[props(default = false)]
    match_anchor_width: bool,
    /// Floor for the width lock, in CSS px: the panel welds to the anchor as
    /// usual but never renders narrower than this. For anchored controls
    /// whose CONTENT needs more room than the control itself has — a palette
    /// swatch is ~190px on a module panel, and its chooser has tabs, a search
    /// box, a two-line catalog and an editor to fit. Ignored unless
    /// `match_anchor_width` is set, and always clamped to the viewport.
    #[props(default = None)]
    min_panel_width_px: Option<f64>,
    children: Element,
) -> Element {
    let mut open = use_signal(|| initially_open);
    use_context_provider(|| PopoverCloseHandle { open });
    let trigger_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-trigger-{id}")
    });
    // Anchored mode measures the external anchor element; regular mode
    // measures the trigger button itself.
    let anchored = anchor_id.is_some();
    let measured_id = anchor_id.unwrap_or_else(|| trigger_id.clone());
    let panel_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-panel-{id}")
    });
    let layer_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-layer-{id}")
    });
    let mut trigger_rect = use_signal(|| None::<RectSnapshot>);
    let mut panel_size = use_signal(|| None::<SizeSnapshot>);
    let position = use_signal(|| PopoverPosition::hidden(placement));
    let auto_update = use_hook(|| Rc::new(RefCell::new(None::<PopoverAutoUpdate>)));
    let panel_resize = use_hook(|| Rc::new(RefCell::new(None::<PanelResizeObserver>)));
    let gradient_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-grad-{id}")
    });
    let progress = use_signal(|| if initially_open { 1.0f64 } else { 0.0f64 });
    let mut render_open = use_signal(|| initially_open);
    let stabilized = use_signal(|| false);
    let animation = use_hook(|| Rc::new(RefCell::new(None::<PopoverAnimation>)));
    let current_position = position();
    let t = progress().clamp(0.0, 1.0);
    let settled = t >= 1.0;
    // The merged-outline chrome activates once the first measurement lands;
    // until then the trigger keeps its normal open look so nothing flashes.
    let attached = render_open() && current_position.visible && t > 0.0;
    let button_class = popover_button_class(open(), attached, &class, &open_class);
    let trigger_placeholder = trigger_placeholder_style(attached, anchored, trigger_rect());
    let panel_class = popover_panel_class(&popup_class);
    let (outline, panel_clip) = if attached {
        trigger_rect()
            .map(|anchor| {
                let panel = panel_size().unwrap_or_else(SizeSnapshot::fallback);
                animated_outline(anchor, panel, current_position, t)
            })
            .unwrap_or_default()
    } else {
        (String::new(), String::new())
    };
    // The width lock measures the anchor's rect and adds the open-state
    // inflate so both panel edges weld with the swollen outline; the panel's
    // ResizeObserver reports the locked width back into `panel_size`, so the
    // centered alignment lands on exactly the anchor's footprint.
    let panel_width_style = if match_anchor_width {
        trigger_rect()
            .map(|rect| {
                let welded = rect.width + 2.0 * TRIGGER_INFLATE_PX;
                // A FLOOR, never a ceiling: the lock exists so the panel is
                // not NARROWER than its anchor (a few px short reads as a
                // mistake, and welding one edge leaves a shelf on the other).
                // Growing past the anchor breaks neither rule — the merged
                // outline unions the two rects — so a control whose content
                // needs more room than the control has may ask for it.
                let width = welded.max(min_panel_width_px.unwrap_or(0.0));
                // Never past the viewport: the same budget the popover cards
                // assume in their own `w-[min(…, calc(100vw-24px))]`.
                format!("width: min({width:.1}px, calc(100vw - 24px));")
            })
            .unwrap_or_default()
    } else {
        String::new()
    };
    let panel_style = format!(
        "{} {panel_clip} {panel_width_style}",
        current_position.style()
    );
    let content_style = panel_content_style(t);
    let (grad_stop_near, grad_stop_far) = gradient_stops(current_position.side);
    let trigger_visual_style = open_trigger_style(trigger_rect());
    // Viewport-clamped case: the panel slid back across its own trigger and
    // covers it entirely. The top-layer trigger copy is skipped then — the
    // panel is on top, and painting the copy over the panel's rows would
    // interleave two surfaces' text.
    let trigger_covered = attached
        && trigger_rect().is_some_and(|anchor| {
            let panel = panel_size().unwrap_or_else(SizeSnapshot::fallback);
            panel_covers_trigger(anchor, panel, current_position)
        });
    let layer_layout_class = if layer_keeps_layout {
        "ux-popover-open-trigger-boxed"
    } else {
        ""
    };
    // The visual re-parented into the top layer while open: the anchored
    // element's live copy in anchored mode, the trigger's clone otherwise.
    let trigger_for_layer = anchor_visual.unwrap_or_else(|| trigger.clone());

    let measured_id_for_click = measured_id.clone();
    let measured_id_for_effect = measured_id.clone();
    let panel_id_for_effect = panel_id.clone();
    let layer_id_for_effect = layer_id.clone();
    let auto_update_for_effect = auto_update.clone();
    let panel_resize_for_effect = panel_resize.clone();
    let panel_resize_for_panel_mount = panel_resize.clone();
    let measured_id_for_layer_mount = measured_id.clone();
    let panel_id_for_layer_mount = panel_id.clone();
    let measured_id_for_panel_mount = measured_id.clone();
    let panel_id_for_panel_mount = panel_id.clone();
    let layer_id_for_layer_mount = layer_id.clone();
    let layer_id_for_panel_mount = layer_id.clone();
    let layer_id_for_drop = layer_id.clone();
    let layer_id_for_visibility = layer_id.clone();
    use_effect(move || {
        if render_open() {
            show_popover_layer(&layer_id_for_visibility);
        } else {
            hide_popover_layer(&layer_id_for_visibility);
        }
    });
    let animation_for_effect = animation.clone();
    use_effect(move || {
        if open() {
            if !*render_open.peek() {
                render_open.set(true);
            }
            measure_trigger_with_stabilization(
                measured_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
                stabilized,
            );
            ensure_popover_auto_update(
                auto_update_for_effect.clone(),
                measured_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                layer_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
            );
            // Panel content can grow/shrink while open (e.g. a tab switch);
            // only a ResizeObserver on the panel itself sees that. On first
            // open the panel isn't in the DOM yet — the panel's `onmounted`
            // installs it then.
            ensure_panel_resize_observer(
                panel_resize_for_effect.clone(),
                measured_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                layer_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
            );
            remeasure_after_fonts_ready(
                measured_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
                stabilized,
            );
        } else {
            auto_update_for_effect.borrow_mut().take();
            panel_resize_for_effect.borrow_mut().take();
        }
        // The layer unmounts only when the close animation lands at 0.
        animate_progress(
            progress,
            render_open,
            if open() { 1.0 } else { 0.0 },
            &animation_for_effect,
        );
    });
    use_drop(move || {
        hide_popover_layer(&layer_id_for_drop);
        auto_update.borrow_mut().take();
        panel_resize.borrow_mut().take();
    });

    rsx! {
        span { class: "tw:relative tw:inline-grid tw:min-w-0 tw:place-items-center",
            button {
                id: "{trigger_id}",
                class: "{button_class}",
                style: "cursor: pointer; {trigger_placeholder}",
                r#type: "button",
                aria_label: "{label}",
                title: "{title}",
                aria_expanded: "{open()}",
                onclick: move |event| {
                    event.stop_propagation();
                    if !open() {
                        // Measure synchronously so the placeholder can pin the
                        // trigger's size on the very first attached frame (in
                        // anchored mode: so the anchor visual lands on the
                        // anchor's rect immediately).
                        if let Some(rect) = trigger_rect_by_id(&measured_id_for_click) {
                            if !rect.is_empty() {
                                trigger_rect.set(Some(rect));
                            }
                        }
                    }
                    open.toggle();
                },
                // While attached, the trigger's VISIBLE copy renders in the
                // top layer; this in-flow button keeps the same classes and
                // content at opacity 0, so its baseline — and therefore the
                // surrounding line box — cannot move when the popover opens.
                {trigger}
            }
            if render_open() {
                div {
                    id: "{layer_id}",
                    class: "ux-popover-layer {chrome_class}",
                    "popover": "manual",
                    onmounted: move |_| {
                        show_popover_layer(&layer_id_for_layer_mount);
                        let trigger_id_for_panel = measured_id_for_layer_mount.clone();
                        let panel_id_for_panel = panel_id_for_layer_mount.clone();
                        spawn(async move {
                            measure_trigger_once(
                                trigger_id_for_panel,
                                panel_id_for_panel,
                                panel_size,
                                trigger_rect,
                                position,
                                placement,
                            );
                        });
                    },
    div {
                        class: "tw:fixed tw:inset-0 tw:z-[70] tw:bg-transparent",
                        aria_hidden: "true",
                        onclick: move |event| {
                            event.stop_propagation();
                            open.set(false);
                        },
                    }
                    // One path draws the merged trigger+panel chrome: fill (a
                    // gradient flowing continuously across both), border, and
                    // shadow. See base/outline.rs.
                    svg {
                        class: "ux-popover-outline-svg",
                        "aria-hidden": "true",
                        defs {
                            linearGradient {
                                id: "{gradient_id}",
                                x1: "0",
                                y1: "0",
                                x2: "0",
                                y2: "1",
                                stop { offset: "0", style: "stop-color: {grad_stop_near};" }
                                stop { offset: "1", style: "stop-color: {grad_stop_far};" }
                            }
                        }
                        path {
                            class: "ux-popover-outline-path",
                            d: "{outline}",
                            fill: "url(#{gradient_id})",
                            fill_rule: "evenodd",
                        }
                    }
                    div {
                        id: "{panel_id}",
                        class: "{panel_class}",
                        style: "{panel_style}",
                        role: "dialog",
                        // Story captures wait for a REAL panel measurement
                        // (not the pre-layout fallback), a resolved position,
                        // a settled animation, and the last stabilization
                        // re-measure (so a late 1px correction can't race the
                        // screenshot).
                        "data-story-wait": if current_position.visible && settled && panel_size().is_some() && stabilized() { "0" } else { "1" },
                        onclick: move |event| event.stop_propagation(),
                        onmounted: move |event| {
                            show_popover_layer(&layer_id_for_panel_mount);
                            let trigger_id_for_panel = measured_id_for_panel_mount.clone();
                            let panel_id_for_panel = panel_id_for_panel_mount.clone();
                            let panel_element = event.data();
                            spawn(async move {
                                let Ok(rect) = panel_element.get_client_rect().await else {
                                    return;
                                };
                                let size = SizeSnapshot::from_pixels_rect(rect);
                                if size.is_empty() {
                                    // Measured before layout settled; the
                                    // stabilization re-measures pick it up.
                                    return;
                                }
                                panel_size.set(Some(size));
                                measure_trigger_once(
                                    trigger_id_for_panel,
                                    panel_id_for_panel,
                                    panel_size,
                                    trigger_rect,
                                    position,
                                    placement,
                                );
                            });
                            ensure_panel_resize_observer(
                                panel_resize_for_panel_mount.clone(),
                                measured_id_for_panel_mount.clone(),
                                panel_id_for_panel_mount.clone(),
                                layer_id_for_panel_mount.clone(),
                                panel_size,
                                trigger_rect,
                                position,
                                placement,
                            );
                        },
                        div { style: "{content_style}", {children} }
                    }
                    // The trigger's visual (anchored mode: the anchor's live
                    // copy), re-parented into the top layer so it paints
                    // above the outline fill (the top layer covers the
                    // in-flow original). Regular triggers are presentational:
                    // clicking closes, focus stays on the in-flow placeholder
                    // button. An anchored visual hosts interactive controls,
                    // so its clicks do NOT dismiss — the backdrop still does.
                    if attached && anchored && !trigger_covered {
                        div {
                            class: "ux-popover-open-trigger",
                            style: "{trigger_visual_style}",
                            onclick: move |event| {
                                event.stop_propagation();
                            },
                            {trigger_for_layer}
                        }
                    } else if attached && !trigger_covered {
                        div {
                            class: "ux-popover-open-trigger {layer_layout_class} {open_class}",
                            style: "{trigger_visual_style}",
                            aria_hidden: "true",
                            onclick: move |event| {
                                event.stop_propagation();
                                open.set(false);
                            },
                            {trigger_for_layer}
                        }
                    }
                }
            }
        }
    }
}

/// A [`PopoverButton`] whose trigger is a single [`StudioIcon`]. Thin wrapper
/// preserved so existing icon-only callers are unchanged.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn IconPopoverButton(
    class: String,
    open_class: String,
    icon: StudioIconName,
    icon_size: u32,
    label: String,
    title: String,
    popup_class: String,
    #[props(default = String::new())] chrome_class: String,
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] initially_open: bool,
    /// Anchored mode pass-through (see [`PopoverButton`]).
    #[props(default = None)]
    anchor_id: Option<String>,
    /// Anchored mode pass-through (see [`PopoverButton`]).
    #[props(default = None)]
    anchor_visual: Option<Element>,
    children: Element,
) -> Element {
    rsx! {
        PopoverButton {
            class,
            open_class,
            trigger: rsx! {
                StudioIcon { name: icon, size: icon_size }
            },
            label,
            title,
            popup_class,
            chrome_class,
            placement,
            initially_open,
            anchor_id,
            anchor_visual,
            {children}
        }
    }
}

/// Re-run the measurement pass once `document.fonts` finishes loading. A
/// popover that mounts before @font-face decoding freezes its position and
/// panel size on fallback text metrics: the later font reflow shifts the
/// anchor without resizing it, so the resize-based auto-update never fires.
/// `document.fonts.ready` resolves immediately when fonts are already loaded,
/// so steady-state opens re-measure at most one microtask late. (The
/// select-churner class from b85dce748, surfaced as run-to-run bistable
/// story captures of the anchored control popovers.)
#[allow(
    unused_variables,
    reason = "host builds have no font loading; the wasm body consumes the args"
)]
fn remeasure_after_fonts_ready(
    trigger_id: String,
    panel_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    stabilized: Signal<bool>,
) {
    #[cfg(not(target_arch = "wasm32"))]
    return;
    #[cfg(target_arch = "wasm32")]
    {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let Ok(fonts) = js_sys::Reflect::get(document.as_ref(), &"fonts".into()) else {
            return;
        };
        let Ok(ready) = js_sys::Reflect::get(&fonts, &"ready".into()) else {
            return;
        };
        let Ok(promise) = ready.dyn_into::<js_sys::Promise>() else {
            return;
        };
        wasm_bindgen_futures::spawn_local(async move {
            if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
                measure_trigger_with_stabilization(
                    trigger_id,
                    panel_id,
                    panel_size,
                    trigger_rect,
                    position,
                    placement,
                    stabilized,
                );
            }
        });
    }
}

fn measure_trigger_with_stabilization(
    trigger_id: String,
    panel_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    mut stabilized: Signal<bool>,
) {
    if *stabilized.peek() {
        stabilized.set(false);
    }
    measure_trigger_once(
        trigger_id.clone(),
        panel_id.clone(),
        panel_size,
        trigger_rect,
        position,
        placement,
    );
    let last_delay = STABILIZE_MEASURE_DELAYS_MS[STABILIZE_MEASURE_DELAYS_MS.len() - 1];
    for delay_ms in STABILIZE_MEASURE_DELAYS_MS {
        schedule_delayed_measure_trigger(
            trigger_id.clone(),
            panel_id.clone(),
            panel_size,
            trigger_rect,
            position,
            placement,
            (delay_ms == last_delay).then_some(stabilized),
            delay_ms,
        );
    }
}

fn measure_trigger_once(
    trigger_id: String,
    panel_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    let current_panel_size = panel_size_by_id(&panel_id).or_else(|| panel_size());
    if let Some(size) = current_panel_size {
        let mut panel_size = panel_size;
        // Write only on real change: the panel ResizeObserver funnels back
        // into this path, and an unconditional set would re-render on every
        // observer fire (observe → measure → set → …) for no visual gain.
        let previous = *panel_size.peek();
        if !previous.is_some_and(|prev| prev.approx_eq(size)) {
            panel_size.set(Some(size));
        }
    }
    measure_trigger_element(
        trigger_id,
        panel_id,
        current_panel_size,
        trigger_rect,
        position,
        placement,
    );
}

#[allow(
    clippy::too_many_arguments,
    reason = "Small DOM timer callback factory"
)]
fn schedule_delayed_measure_trigger(
    trigger_id: String,
    panel_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    stabilized: Option<Signal<bool>>,
    delay_ms: i32,
) {
    let Some(window) = web_sys::window() else {
        // No timers available; don't leave story captures waiting forever.
        if let Some(mut stabilized) = stabilized {
            stabilized.set(true);
        }
        return;
    };

    let callback = Closure::once(move || {
        measure_trigger_once(
            trigger_id,
            panel_id,
            panel_size,
            trigger_rect,
            position,
            placement,
        );
        // The final stabilization pass has run: measurements are trustworthy
        // now, so story captures may proceed.
        if let Some(mut stabilized) = stabilized {
            stabilized.set(true);
        }
    });
    if window
        .set_timeout_with_callback_and_timeout_and_arguments_0(
            callback.as_ref().unchecked_ref(),
            delay_ms,
        )
        .is_ok()
    {
        callback.forget();
    }
}

fn measure_trigger_element(
    trigger_id: String,
    panel_id: String,
    current_panel_size: Option<SizeSnapshot>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    // The first attempt runs synchronously: getBoundingClientRect forces
    // layout when needed, and waiting for an animation frame here left the
    // popover unpositioned in environments that throttle rAF (occluded
    // pages). Retries — only needed when the element isn't in the DOM yet —
    // still go through rAF.
    spawn_measure_trigger_element(
        trigger_id,
        panel_id,
        current_panel_size,
        trigger_rect,
        position,
        placement,
        0,
    );
}

fn schedule_measure_trigger_element(
    trigger_id: String,
    panel_id: String,
    current_panel_size: Option<SizeSnapshot>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    attempt: u8,
) {
    let Some(window) = web_sys::window() else {
        spawn_measure_trigger_element(
            trigger_id,
            panel_id,
            current_panel_size,
            trigger_rect,
            position,
            placement,
            attempt,
        );
        return;
    };

    let fallback_trigger_id = trigger_id.clone();
    let fallback_panel_id = panel_id.clone();
    let fallback_trigger_rect = trigger_rect;
    let fallback_position = position;
    let callback = Closure::once(move || {
        spawn_measure_trigger_element(
            trigger_id,
            panel_id,
            current_panel_size,
            trigger_rect,
            position,
            placement,
            attempt,
        );
    });
    if window
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .is_ok()
    {
        callback.forget();
    } else {
        spawn_measure_trigger_element(
            fallback_trigger_id,
            fallback_panel_id,
            current_panel_size,
            fallback_trigger_rect,
            fallback_position,
            placement,
            attempt,
        );
    }
}

fn spawn_measure_trigger_element(
    trigger_id: String,
    panel_id: String,
    current_panel_size: Option<SizeSnapshot>,
    mut trigger_rect: Signal<Option<RectSnapshot>>,
    mut position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    attempt: u8,
) {
    let Some(anchor) = trigger_rect_by_id(&trigger_id) else {
        if attempt < MEASURE_RETRY_LIMIT {
            schedule_measure_trigger_element(
                trigger_id,
                panel_id,
                current_panel_size,
                trigger_rect,
                position,
                placement,
                attempt + 1,
            );
        }
        return;
    };
    if anchor.is_empty() && attempt < MEASURE_RETRY_LIMIT {
        schedule_measure_trigger_element(
            trigger_id,
            panel_id,
            current_panel_size,
            trigger_rect,
            position,
            placement,
            attempt + 1,
        );
        return;
    }
    if anchor.is_empty() {
        return;
    }

    let size = current_panel_size.unwrap_or_else(SizeSnapshot::fallback);
    trigger_rect.set(Some(anchor));
    position.set(PopoverPosition::from_anchor(anchor, size, placement));
}

/// The trigger's rect, measured free of its own placeholder pin.
///
/// While attached, the in-flow placeholder pins width/height to the LAST
/// measurement ([`trigger_placeholder_style`]) — so re-measuring it just
/// reads that pin back. A first measurement taken before the trigger's
/// layout settled (a `w-full` grid item that had not yet resolved against
/// its track, measuring as its shrink-to-fit content instead) then froze
/// permanently: the stabilization passes and the fonts-ready pass all
/// re-confirmed the wrong size, the top-layer copy painted at it — narrow
/// enough to wrap the trigger's label into a second line spilling over the
/// panel's first row — and the merged outline anchored on it.
///
/// The pin is dropped for the read and restored in the same task, so no
/// frame ever paints without it.
fn trigger_rect_by_id(trigger_id: &str) -> Option<RectSnapshot> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.get_element_by_id(trigger_id)?;
    let pinned = element.dyn_ref::<web_sys::HtmlElement>().filter(|element| {
        element.get_attribute("class").is_some_and(|class| {
            class
                .split_whitespace()
                .any(|c| c == TRIGGER_PLACEHOLDER_CLASS)
        })
    });
    let saved = pinned.map(|element| {
        let style = element.style();
        let width = style.get_property_value("width").unwrap_or_default();
        let height = style.get_property_value("height").unwrap_or_default();
        let _ = style.remove_property("width");
        let _ = style.remove_property("height");
        (width, height)
    });
    let rect = RectSnapshot::from_dom_rect(element.get_bounding_client_rect());
    if let (Some(element), Some((width, height))) = (pinned, saved) {
        let style = element.style();
        let _ = style.set_property("width", &width);
        let _ = style.set_property("height", &height);
    }
    Some(rect)
}

fn panel_size_by_id(panel_id: &str) -> Option<SizeSnapshot> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.get_element_by_id(panel_id)?;
    let size = SizeSnapshot::from_dom_rect(element.get_bounding_client_rect());
    (!size.is_empty()).then_some(size)
}

fn show_popover_layer(layer_id: &str) {
    if let Some(layer) = popover_layer_by_id(layer_id) {
        let _ = layer.show_popover();
    }
}

fn hide_popover_layer(layer_id: &str) {
    if let Some(layer) = popover_layer_by_id(layer_id) {
        let _ = layer.hide_popover();
    }
}

fn popover_layer_by_id(layer_id: &str) -> Option<web_sys::HtmlElement> {
    let window = web_sys::window()?;
    let document = window.document()?;
    document
        .get_element_by_id(layer_id)?
        .dyn_into::<web_sys::HtmlElement>()
        .ok()
}

fn ensure_popover_auto_update(
    auto_update: Rc<RefCell<Option<PopoverAutoUpdate>>>,
    trigger_id: String,
    panel_id: String,
    layer_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    if auto_update.borrow().is_some() {
        return;
    }

    let Some(update) = PopoverAutoUpdate::install(
        trigger_id,
        panel_id,
        layer_id,
        panel_size,
        trigger_rect,
        position,
        placement,
    ) else {
        return;
    };
    *auto_update.borrow_mut() = Some(update);
}

struct PopoverAutoUpdate {
    window: web_sys::Window,
    scroll_callback: Closure<dyn FnMut(web_sys::Event)>,
    resize_callback: Closure<dyn FnMut(web_sys::Event)>,
}

impl PopoverAutoUpdate {
    fn install(
        trigger_id: String,
        panel_id: String,
        layer_id: String,
        panel_size: Signal<Option<SizeSnapshot>>,
        trigger_rect: Signal<Option<RectSnapshot>>,
        position: Signal<PopoverPosition>,
        placement: PopoverPlacement,
    ) -> Option<Self> {
        let window = web_sys::window()?;
        let pending = Rc::new(Cell::new(false));
        let scroll_callback = make_update_callback(
            trigger_id.clone(),
            panel_id.clone(),
            layer_id.clone(),
            panel_size,
            trigger_rect,
            position,
            placement,
            pending.clone(),
        );
        let resize_callback = make_update_callback(
            trigger_id,
            panel_id,
            layer_id,
            panel_size,
            trigger_rect,
            position,
            placement,
            pending,
        );

        if window
            .add_event_listener_with_callback_and_bool(
                "scroll",
                scroll_callback.as_ref().unchecked_ref(),
                true,
            )
            .is_err()
        {
            return None;
        }
        if window
            .add_event_listener_with_callback("resize", resize_callback.as_ref().unchecked_ref())
            .is_err()
        {
            let _ = window.remove_event_listener_with_callback_and_bool(
                "scroll",
                scroll_callback.as_ref().unchecked_ref(),
                true,
            );
            return None;
        }

        Some(Self {
            window,
            scroll_callback,
            resize_callback,
        })
    }
}

impl Drop for PopoverAutoUpdate {
    fn drop(&mut self) {
        let _ = self.window.remove_event_listener_with_callback_and_bool(
            "scroll",
            self.scroll_callback.as_ref().unchecked_ref(),
            true,
        );
        let _ = self.window.remove_event_listener_with_callback(
            "resize",
            self.resize_callback.as_ref().unchecked_ref(),
        );
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Small DOM observer install wrapper"
)]
fn ensure_panel_resize_observer(
    observer: Rc<RefCell<Option<PanelResizeObserver>>>,
    trigger_id: String,
    panel_id: String,
    layer_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    if observer.borrow().is_some() {
        return;
    }

    let Some(installed) = PanelResizeObserver::install(
        trigger_id,
        panel_id,
        layer_id,
        panel_size,
        trigger_rect,
        position,
        placement,
    ) else {
        return;
    };
    *observer.borrow_mut() = Some(installed);
}

/// Watches the open panel element for size changes — content growth or
/// shrink while open (e.g. a tab switch inside the panel), which window
/// scroll/resize listeners never see — and funnels them into the same
/// rAF-coalesced re-measure as [`PopoverAutoUpdate`]. No feedback loop:
/// ResizeObserver only fires on actual border-box changes, and
/// `measure_trigger_once` writes `panel_size` only when the measurement
/// really changed.
struct PanelResizeObserver {
    observer: web_sys::ResizeObserver,
    _callback: Closure<dyn FnMut(web_sys::js_sys::Array)>,
}

impl PanelResizeObserver {
    fn install(
        trigger_id: String,
        panel_id: String,
        layer_id: String,
        panel_size: Signal<Option<SizeSnapshot>>,
        trigger_rect: Signal<Option<RectSnapshot>>,
        position: Signal<PopoverPosition>,
        placement: PopoverPlacement,
    ) -> Option<Self> {
        let element = web_sys::window()?
            .document()?
            .get_element_by_id(&panel_id)?;
        let pending = Rc::new(Cell::new(false));
        let callback = Closure::<dyn FnMut(web_sys::js_sys::Array)>::new(
            move |_entries: web_sys::js_sys::Array| {
                request_popover_update(
                    trigger_id.clone(),
                    panel_id.clone(),
                    layer_id.clone(),
                    panel_size,
                    trigger_rect,
                    position,
                    placement,
                    pending.clone(),
                );
            },
        );
        let observer = match web_sys::ResizeObserver::new(callback.as_ref().unchecked_ref()) {
            Ok(observer) => observer,
            Err(error) => {
                // Degrade to the pre-observer behavior (open-time measures
                // plus window scroll/resize) rather than failing the popover.
                log::warn!("popover: ResizeObserver unavailable: {error:?}");
                return None;
            }
        };
        observer.observe(&element);
        Some(Self {
            observer,
            _callback: callback,
        })
    }
}

impl Drop for PanelResizeObserver {
    fn drop(&mut self) {
        self.observer.disconnect();
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "Small DOM listener callback factory"
)]
fn make_update_callback(
    trigger_id: String,
    panel_id: String,
    layer_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    pending: Rc<Cell<bool>>,
) -> Closure<dyn FnMut(web_sys::Event)> {
    Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |_| {
        request_popover_update(
            trigger_id.clone(),
            panel_id.clone(),
            layer_id.clone(),
            panel_size,
            trigger_rect,
            position,
            placement,
            pending.clone(),
        );
    }))
}

#[allow(
    clippy::too_many_arguments,
    reason = "Small DOM listener callback body"
)]
fn request_popover_update(
    trigger_id: String,
    panel_id: String,
    layer_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    pending: Rc<Cell<bool>>,
) {
    if pending.replace(true) {
        return;
    }

    let Some(window) = web_sys::window() else {
        pending.set(false);
        show_popover_layer(&layer_id);
        measure_trigger_once(
            trigger_id,
            panel_id,
            panel_size,
            trigger_rect,
            position,
            placement,
        );
        return;
    };

    let callback = Closure::once(move || {
        pending.set(false);
        show_popover_layer(&layer_id);
        measure_trigger_once(
            trigger_id,
            panel_id,
            panel_size,
            trigger_rect,
            position,
            placement,
        );
    });
    if window
        .request_animation_frame(callback.as_ref().unchecked_ref())
        .is_ok()
    {
        callback.forget();
    }
}

fn popover_button_class(open: bool, attached: bool, class: &str, open_class: &str) -> String {
    if !open {
        class.to_string()
    } else if attached {
        // The visible copy lives in the top layer; the in-flow button keeps
        // its open-state layout classes and content so its size and baseline
        // stay EXACTLY as when it was measured, with the placeholder class
        // (unlayered, so it wins over the utility layer) making it paint
        // nothing.
        format!("{open_class} {TRIGGER_PLACEHOLDER_CLASS}")
    } else {
        open_class.to_string()
    }
}

fn popover_panel_class(popup_class: &str) -> String {
    // `ux-svg-popover-panel` strips the panel's own background/border/shadow;
    // the merged outline path draws all of that.
    format!("{popup_class} ux-popover-panel ux-svg-popover-panel")
}

/// Inline size pin for the in-flow placeholder button while attached. The
/// placeholder keeps its own classes and content (which already hold the
/// size); the pin is a belt-and-suspenders guard that keeps the footprint at
/// the measured rect the top-layer chrome was positioned against. Anchored
/// mode never pins: the measured rect is the ANCHOR's, not the button's —
/// pinning would blow the (hidden) trigger hint up to the anchor's size.
fn trigger_placeholder_style(attached: bool, anchored: bool, rect: Option<RectSnapshot>) -> String {
    match (attached, anchored, rect) {
        (true, false, Some(rect)) => {
            format!("width: {:.1}px; height: {:.1}px;", rect.width, rect.height)
        }
        _ => String::new(),
    }
}

/// The settled panel rect fully covers the trigger's visible (inflated)
/// footprint. Only the viewport-clamped case can get here: an unclamped
/// panel starts at the trigger's seam edge, which always leaves the trigger
/// body outside the panel.
fn panel_covers_trigger(
    anchor: RectSnapshot,
    panel: SizeSnapshot,
    position: PopoverPosition,
) -> bool {
    // Half-pixel slack: a welded panel edge lands EXACTLY on the inflated
    // trigger edge (`snap_to_trigger_edges` welds it), so strict comparisons
    // would flip coverage on sub-pixel measurement noise.
    const SLACK_PX: f64 = 0.5;
    let visible = anchor.inflate(TRIGGER_INFLATE_PX);
    position.left <= visible.x + SLACK_PX
        && position.top <= visible.y + SLACK_PX
        && position.left + panel.width >= visible.x + visible.width - SLACK_PX
        && position.top + panel.height >= visible.y + visible.height - SLACK_PX
}

/// Fixed-position style for the top-layer trigger visual.
fn open_trigger_style(rect: Option<RectSnapshot>) -> String {
    rect.map(|rect| {
        format!(
            "left: {:.1}px; top: {:.1}px; width: {:.1}px; height: {:.1}px;",
            rect.x, rect.y, rect.width, rect.height
        )
    })
    .unwrap_or_default()
}

/// The merged trigger+panel outline at animation time `t` (0 = closed,
/// 1 = settled), plus the `clip-path` that reveals the panel's content in step
/// with the growing shape.
///
/// The animation interpolates the panel's INPUT rect and re-unions every
/// frame — the path is never morphed directly, so corners appear and grow
/// naturally as segments become long enough to hold them.
fn animated_outline(
    anchor: RectSnapshot,
    panel: SizeSnapshot,
    position: PopoverPosition,
    t: f64,
) -> (String, String) {
    let inflate = TRIGGER_INFLATE_PX * ease_out_cubic((t / 0.5).clamp(0.0, 1.0));
    let anchor_rect = OutlineRect {
        x: anchor.x,
        y: anchor.y,
        w: anchor.width,
        h: anchor.height,
    }
    .inflate(inflate);
    let final_rect = OutlineRect {
        x: position.left,
        y: position.top,
        w: panel.width,
        h: panel.height,
    };
    let panel_rect = panel_rect_at(t, anchor_rect, final_rect, position.side);
    let path = merged_outline_path(
        &[anchor_rect, panel_rect],
        POPOVER_CORNER_RADIUS_PX,
        device_pixel_ratio(),
    );
    let clip = if t >= 1.0 {
        String::new()
    } else {
        let top = (panel_rect.y - final_rect.y).max(0.0);
        let right = ((final_rect.x + final_rect.w) - (panel_rect.x + panel_rect.w)).max(0.0);
        let bottom = ((final_rect.y + final_rect.h) - (panel_rect.y + panel_rect.h)).max(0.0);
        let left = (panel_rect.x - final_rect.x).max(0.0);
        format!(
            "clip-path: inset({top:.1}px {right:.1}px {bottom:.1}px {left:.1}px round {POPOVER_CORNER_RADIUS_PX}px);"
        )
    };
    (path, clip)
}

/// The panel's input rect at animation time `t`: a sliver at the trigger's
/// seam edge growing out to its final rect. The seam edge overlaps the
/// (inflated) trigger by the border width so the union always merges.
///
/// The seam edge lerps to the FINAL rect's edge, not the trigger's: when the
/// panel fits on its side the two are the same value (the lerp is the
/// identity), but a viewport-clamped position (`panel_top` slid the panel
/// back across its own trigger because it fits neither side) puts them
/// apart — the drawn box must land on the panel's actual rect, or the
/// chrome detaches from its content.
fn panel_rect_at(t: f64, anchor: OutlineRect, fin: OutlineRect, side: PopoverSide) -> OutlineRect {
    let eased = ease_out_cubic(t);
    let left = lerp(anchor.x, fin.x, eased);
    let right = lerp(anchor.x + anchor.w, fin.x + fin.w, eased);
    match side {
        PopoverSide::Below => {
            let seam = anchor.y + anchor.h - POPOVER_BORDER_WIDTH_PX;
            let top = lerp(seam, fin.y, eased);
            let bottom = lerp(anchor.y + anchor.h, fin.y + fin.h, eased);
            OutlineRect {
                x: left,
                y: top,
                w: right - left,
                h: (bottom - top).max(0.0),
            }
        }
        PopoverSide::Above => {
            let seam = anchor.y + POPOVER_BORDER_WIDTH_PX;
            let bottom = lerp(seam, fin.y + fin.h, eased);
            let top = lerp(anchor.y, fin.y, eased);
            OutlineRect {
                x: left,
                y: top,
                w: right - left,
                h: (bottom - top).max(0.0),
            }
        }
    }
}

/// Fade/slide for the panel's content, delayed slightly behind the shape.
fn panel_content_style(t: f64) -> String {
    let eased =
        ease_out_cubic(((t - CONTENT_FADE_DELAY) / (1.0 - CONTENT_FADE_DELAY)).clamp(0.0, 1.0));
    if eased >= 1.0 {
        return String::new();
    }
    format!(
        "opacity: {eased:.3}; transform: translateY({:.1}px);",
        -6.0 * (1.0 - eased)
    )
}

fn ease_out_cubic(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Drives `progress` toward `target` with a rAF loop that runs only while a
/// transition is in flight. One persistent closure per popover instance (kept
/// in `holder`); retargeting mid-flight continues from the current progress
/// with the duration scaled to the remaining distance. Honors
/// `prefers-reduced-motion` by jumping straight to the target.
fn animate_progress(
    progress: Signal<f64>,
    mut render_open: Signal<bool>,
    target: f64,
    holder: &Rc<RefCell<Option<PopoverAnimation>>>,
) {
    // Called from a `use_effect`: every read here uses `peek()` so progress
    // does NOT become a reactive dependency (the effect must not re-run per
    // animation frame, or the timeline would restart each frame).
    let from = *progress.peek();
    let finish_instantly = |mut progress: Signal<f64>, mut render_open: Signal<bool>| {
        if *progress.peek() != target {
            progress.set(target);
        }
        if target <= 0.0 && *render_open.peek() {
            render_open.set(false);
        }
    };
    if (from - target).abs() < 1e-6 {
        if target <= 0.0 && *render_open.peek() {
            render_open.set(false);
        }
        return;
    }
    let window = web_sys::window();
    let Some(window) = window.filter(|_| !prefers_reduced_motion()) else {
        finish_instantly(progress, render_open);
        return;
    };

    if holder.borrow().is_none() {
        let Some(anim) = PopoverAnimation::new(window, progress, render_open) else {
            finish_instantly(progress, render_open);
            return;
        };
        *holder.borrow_mut() = Some(anim);
    }
    if let Some(anim) = holder.borrow().as_ref() {
        anim.retarget(from, target);
    }
}

fn prefers_reduced_motion() -> bool {
    web_sys::window()
        .and_then(|window| window.match_media("(prefers-reduced-motion: reduce)").ok())
        .flatten()
        .map(|query| query.matches())
        .unwrap_or(false)
}

struct AnimationTimeline {
    from: Cell<f64>,
    target: Cell<f64>,
    start: Cell<Option<f64>>,
    duration_ms: Cell<f64>,
    raf_id: Cell<Option<i32>>,
    tick: RefCell<Option<web_sys::js_sys::Function>>,
}

/// The per-popover animation driver: one long-lived rAF closure plus the
/// timeline state it reads. Dropped (and any pending frame cancelled) with the
/// component.
struct PopoverAnimation {
    window: web_sys::Window,
    timeline: Rc<AnimationTimeline>,
    _closure: Closure<dyn FnMut(f64)>,
}

impl PopoverAnimation {
    fn new(
        window: web_sys::Window,
        mut progress: Signal<f64>,
        mut render_open: Signal<bool>,
    ) -> Option<Self> {
        let timeline = Rc::new(AnimationTimeline {
            from: Cell::new(0.0),
            target: Cell::new(0.0),
            start: Cell::new(None),
            duration_ms: Cell::new(1.0),
            raf_id: Cell::new(None),
            tick: RefCell::new(None),
        });

        let timeline_for_frames = timeline.clone();
        let window_for_frames = window.clone();
        let closure = Closure::wrap(Box::new(move |now: f64| {
            let timeline = &timeline_for_frames;
            timeline.raf_id.set(None);
            let start = timeline.start.get().unwrap_or(now);
            if timeline.start.get().is_none() {
                timeline.start.set(Some(now));
            }
            let from = timeline.from.get();
            let target = timeline.target.get();
            let t = ((now - start) / timeline.duration_ms.get()).clamp(0.0, 1.0);
            progress.set(from + (target - from) * t);
            if t < 1.0 {
                let scheduled = timeline.tick.borrow().as_ref().and_then(|tick| {
                    window_for_frames
                        .request_animation_frame(tick.unchecked_ref())
                        .ok()
                });
                match scheduled {
                    Some(id) => timeline.raf_id.set(Some(id)),
                    None => {
                        progress.set(target);
                        if target <= 0.0 {
                            render_open.set(false);
                        }
                    }
                }
            } else if target <= 0.0 {
                render_open.set(false);
            }
        }) as Box<dyn FnMut(f64)>);
        let tick: web_sys::js_sys::Function = closure
            .as_ref()
            .unchecked_ref::<web_sys::js_sys::Function>()
            .clone();
        *timeline.tick.borrow_mut() = Some(tick);

        Some(Self {
            window,
            timeline,
            _closure: closure,
        })
    }

    fn retarget(&self, from: f64, target: f64) {
        let base = if target > from {
            OPEN_ANIM_MS
        } else {
            CLOSE_ANIM_MS
        };
        self.timeline.from.set(from);
        self.timeline.target.set(target);
        self.timeline.start.set(None);
        self.timeline
            .duration_ms
            .set((base * (target - from).abs()).max(1.0));
        if self.timeline.raf_id.get().is_none() {
            let scheduled = self.timeline.tick.borrow().as_ref().and_then(|tick| {
                self.window
                    .request_animation_frame(tick.unchecked_ref())
                    .ok()
            });
            self.timeline.raf_id.set(scheduled);
        }
    }
}

impl Drop for PopoverAnimation {
    fn drop(&mut self) {
        if let Some(id) = self.timeline.raf_id.take() {
            let _ = self.window.cancel_animation_frame(id);
        }
    }
}

/// Gradient stops for the outline fill: the tone's trigger fill at the trigger
/// end of the shape, the panel's away fill at the far end. CSS variables
/// resolve through the `chrome_class` on the layer.
fn gradient_stops(side: PopoverSide) -> (&'static str, &'static str) {
    const TRIGGER_FILL: &str =
        "var(--ux-popover-trigger-fill-top, var(--studio-color-surface-raised))";
    const PANEL_FILL: &str =
        "var(--ux-popover-panel-fill-away, var(--studio-color-surface-raised))";
    match side {
        PopoverSide::Below => (TRIGGER_FILL, PANEL_FILL),
        PopoverSide::Above => (PANEL_FILL, TRIGGER_FILL),
    }
}

fn device_pixel_ratio() -> f64 {
    web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0)
}

#[derive(Clone, Copy, Debug)]
struct RectSnapshot {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl RectSnapshot {
    fn from_dom_rect(rect: web_sys::DomRect) -> Self {
        Self {
            x: rect.x(),
            y: rect.y(),
            width: rect.width(),
            height: rect.height(),
        }
    }

    fn is_empty(self) -> bool {
        self.width < 1.0 || self.height < 1.0
    }

    /// Grow the rect on all sides (the trigger's visible footprint while the
    /// popover is open, matching `OutlineRect::inflate`).
    fn inflate(self, by: f64) -> Self {
        Self {
            x: self.x - by,
            y: self.y - by,
            width: self.width + 2.0 * by,
            height: self.height + 2.0 * by,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SizeSnapshot {
    width: f64,
    height: f64,
}

impl SizeSnapshot {
    fn fallback() -> Self {
        Self {
            width: FALLBACK_PANEL_WIDTH_PX,
            height: FALLBACK_PANEL_HEIGHT_PX,
        }
    }

    fn from_pixels_rect(rect: dioxus::html::geometry::PixelsRect) -> Self {
        Self {
            width: rect.size.width,
            height: rect.size.height,
        }
    }

    fn from_dom_rect(rect: web_sys::DomRect) -> Self {
        Self {
            width: rect.width(),
            height: rect.height(),
        }
    }

    fn is_empty(self) -> bool {
        self.width < 1.0 || self.height < 1.0
    }

    /// Equal within measurement noise. Sub-epsilon deltas must not rewrite
    /// `panel_size` (`measure_trigger_once`'s observer-loop guard); real
    /// corrections — the stabilization passes chase 1px ones — still land.
    fn approx_eq(self, other: Self) -> bool {
        const EPSILON_PX: f64 = 0.1;
        (self.width - other.width).abs() < EPSILON_PX
            && (self.height - other.height).abs() < EPSILON_PX
    }
}

#[derive(Clone, Copy, Debug)]
struct PopoverPosition {
    left: f64,
    top: f64,
    visible: bool,
    side: PopoverSide,
}

impl PopoverPosition {
    fn hidden(placement: PopoverPlacement) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            visible: false,
            side: placement.side(),
        }
    }

    fn from_anchor(anchor: RectSnapshot, panel: SizeSnapshot, placement: PopoverPlacement) -> Self {
        let (viewport_width, viewport_height) = viewport_size();
        let side = placement.side().resolve(anchor, panel, viewport_height);
        let top = side.panel_top(anchor, panel, viewport_height);
        // Horizontal alignment targets the trigger's VISIBLE edge — the
        // outline swells by TRIGGER_INFLATE_PX while open, and aligning to
        // the raw rect left a small stepped shelf where a welded edge was
        // expected.
        let left = panel_left(
            anchor.inflate(TRIGGER_INFLATE_PX),
            panel,
            placement.align(),
            viewport_width,
        );

        Self {
            left,
            top,
            visible: true,
            side,
        }
    }

    fn style(self) -> String {
        let visibility = if self.visible { "visible" } else { "hidden" };
        format!(
            "left: {:.1}px; top: {:.1}px; visibility: {visibility};",
            self.left, self.top
        )
    }
}

/// A shelf narrower than the corner radius reads as a rendering mistake, so
/// panel edges within this distance of a trigger edge snap to weld exactly.
const EDGE_SNAP_PX: f64 = POPOVER_CORNER_RADIUS_PX;

/// Panel left edge: the aligned position, clamped inside the viewport margin,
/// then magnetically snapped to the trigger's visible edges. `anchor` is the
/// trigger's VISIBLE rect (inflated while open). Aligned edges weld in the
/// outline; genuinely offset edges (beyond the snap band) get proper concave
/// fillets.
fn panel_left(
    anchor: RectSnapshot,
    panel: SizeSnapshot,
    align: PopoverAlign,
    viewport_width: f64,
) -> f64 {
    let desired = match align {
        PopoverAlign::Start => anchor.x,
        PopoverAlign::Middle => anchor.x + (anchor.width - panel.width) / 2.0,
        PopoverAlign::End => anchor.x + anchor.width - panel.width,
    };
    let max_left = (viewport_width - panel.width - POPOVER_MARGIN_PX).max(POPOVER_MARGIN_PX);
    let clamped = desired.clamp(POPOVER_MARGIN_PX, max_left);
    snap_to_trigger_edges(clamped, anchor, panel)
}

/// If the panel's left or right edge lands within [`EDGE_SNAP_PX`] of the
/// trigger's corresponding edge (e.g. after viewport clamping), shift the
/// panel so the edges weld instead of leaving a sub-radius shelf. The nearer
/// edge wins when both are in range. The shift may exceed the viewport-margin
/// clamp by up to the snap distance — a clean weld beats a strict margin.
fn snap_to_trigger_edges(left: f64, anchor: RectSnapshot, panel: SizeSnapshot) -> f64 {
    let shift_for_left = anchor.x - left;
    let shift_for_right = (anchor.x + anchor.width) - (left + panel.width);
    let shift = if shift_for_left.abs() <= shift_for_right.abs() {
        shift_for_left
    } else {
        shift_for_right
    };
    if shift.abs() <= EDGE_SNAP_PX {
        left + shift
    } else {
        left
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopoverSide {
    Above,
    Below,
}

impl PopoverSide {
    fn resolve(self, anchor: RectSnapshot, panel: SizeSnapshot, viewport_height: f64) -> Self {
        let max_top = viewport_height - panel.height - POPOVER_MARGIN_PX;
        let below_top = Self::Below.viewport_panel_top(anchor, panel);
        let above_top = Self::Above.viewport_panel_top(anchor, panel);
        let below_fits = below_top <= max_top;
        let above_fits = above_top >= POPOVER_MARGIN_PX;

        match self {
            Self::Below if below_fits || !above_fits => Self::Below,
            Self::Below => Self::Above,
            Self::Above if above_fits || !below_fits => Self::Above,
            Self::Above => Self::Below,
        }
    }

    fn viewport_panel_top(self, anchor: RectSnapshot, panel: SizeSnapshot) -> f64 {
        match self {
            Self::Below => anchor.y + anchor.height - POPOVER_BORDER_WIDTH_PX,
            Self::Above => anchor.y - panel.height + POPOVER_BORDER_WIDTH_PX,
        }
    }

    fn panel_top(self, anchor: RectSnapshot, panel: SizeSnapshot, viewport_height: f64) -> f64 {
        let max_top = (viewport_height - panel.height - POPOVER_MARGIN_PX).max(POPOVER_MARGIN_PX);
        self.viewport_panel_top(anchor, panel)
            .clamp(POPOVER_MARGIN_PX, max_top)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopoverAlign {
    Start,
    Middle,
    End,
}

impl PopoverPlacement {
    fn side(self) -> PopoverSide {
        match self {
            Self::TopStart | Self::TopMiddle | Self::TopEnd => PopoverSide::Above,
            Self::BottomStart | Self::BottomMiddle | Self::BottomEnd => PopoverSide::Below,
        }
    }

    fn align(self) -> PopoverAlign {
        match self {
            Self::TopStart | Self::BottomStart => PopoverAlign::Start,
            Self::TopMiddle | Self::BottomMiddle => PopoverAlign::Middle,
            Self::TopEnd | Self::BottomEnd => PopoverAlign::End,
        }
    }
}

fn viewport_size() -> (f64, f64) {
    let Some(window) = web_sys::window() else {
        return (1024.0, 768.0);
    };
    let width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(1024.0);
    let height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(768.0);
    (width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(x: f64, width: f64) -> RectSnapshot {
        RectSnapshot {
            x,
            y: 10.0,
            width,
            height: 24.0,
        }
    }

    fn panel(width: f64) -> SizeSnapshot {
        SizeSnapshot {
            width,
            height: 100.0,
        }
    }

    #[test]
    fn end_alignment_welds_to_visible_trigger_edge() {
        let visible = anchor(500.0, 32.0).inflate(TRIGGER_INFLATE_PX);
        let left = panel_left(visible, panel(300.0), PopoverAlign::End, 1024.0);
        assert_eq!(
            left + 300.0,
            visible.x + visible.width,
            "panel right edge must weld to the inflated trigger edge"
        );
    }

    #[test]
    fn start_alignment_welds_to_visible_trigger_edge() {
        let visible = anchor(500.0, 32.0).inflate(TRIGGER_INFLATE_PX);
        let left = panel_left(visible, panel(300.0), PopoverAlign::Start, 1024.0);
        assert_eq!(left, visible.x);
    }

    #[test]
    fn small_clamp_offset_snaps_back_to_weld() {
        // Viewport clamp (max_left = 1024 - 300 - 12 = 712) pushes the panel
        // 6px short of the trigger edge; the snap band welds it anyway.
        let visible = anchor(985.0, 30.0).inflate(TRIGGER_INFLATE_PX); // right = 1018
        let left = panel_left(visible, panel(300.0), PopoverAlign::End, 1024.0);
        assert_eq!(
            left + 300.0,
            visible.x + visible.width,
            "sub-radius shelf from clamping must snap to a weld"
        );
    }

    #[test]
    fn large_offsets_keep_their_fillets() {
        // Clamped 20px short of the trigger edge: beyond the snap band, the
        // ledge is wide enough for proper fillets and must stay put.
        let visible = anchor(1002.0, 30.0).inflate(TRIGGER_INFLATE_PX); // right = 1035
        let left = panel_left(visible, panel(300.0), PopoverAlign::End, 1024.0);
        assert_eq!(left, 712.0, "clamped position beyond the snap band is kept");
    }

    #[test]
    fn middle_alignment_of_wide_panel_is_untouched() {
        let visible = anchor(500.0, 32.0).inflate(TRIGGER_INFLATE_PX);
        let left = panel_left(visible, panel(300.0), PopoverAlign::Middle, 1024.0);
        let expected = visible.x + (visible.width - 300.0) / 2.0;
        assert_eq!(
            left, expected,
            "wide centered panels have no edge in snap range"
        );
    }

    #[test]
    fn snap_shifts_by_the_nearer_edge() {
        // Panel barely wider than the trigger, offset so the left edge is the
        // closer weld: it wins.
        let visible = anchor(100.0, 40.0);
        let left = snap_to_trigger_edges(98.0, visible, panel(46.0));
        assert_eq!(left, 100.0);
    }

    #[test]
    fn panel_size_epsilon_passes_real_changes_and_eats_noise() {
        // The observer-loop guard: float noise must compare equal (so a
        // ResizeObserver fire can't rewrite `panel_size` forever), while the
        // 1px corrections the stabilization passes exist for must not.
        let base = SizeSnapshot {
            width: 280.0,
            height: 180.0,
        };
        let noise = SizeSnapshot {
            width: 280.02,
            height: 179.98,
        };
        let grown = SizeSnapshot {
            width: 280.0,
            height: 181.0,
        };
        assert!(base.approx_eq(noise));
        assert!(!base.approx_eq(grown));
    }

    #[test]
    fn settled_outline_follows_an_unclamped_panel_exactly() {
        // The normal case: the panel fits below, so the final rect's top IS
        // the trigger's seam edge, and the seam lerp must be the identity at
        // every t (the box top never leaves the seam while growing).
        let anchor_rect = OutlineRect {
            x: 100.0,
            y: 100.0,
            w: 40.0,
            h: 24.0,
        };
        let fin = OutlineRect {
            x: 100.0,
            y: 100.0 + 24.0 - POPOVER_BORDER_WIDTH_PX,
            w: 300.0,
            h: 200.0,
        };
        for t in [0.0, 0.3, 0.7, 1.0] {
            let rect = panel_rect_at(t, anchor_rect, fin, PopoverSide::Below);
            assert_eq!(
                rect.y,
                anchor_rect.y + anchor_rect.h - POPOVER_BORDER_WIDTH_PX,
                "unclamped seam edge must stay welded at t={t}"
            );
        }
    }

    #[test]
    fn settled_outline_follows_a_clamped_panel() {
        // The short-viewport case: `panel_top` clamped the panel back across
        // its own trigger (fin.y is ABOVE the seam). The settled box must
        // land on the panel's actual rect — a box pinned at the seam draws
        // the chrome detached from its content (the add-node-picker story
        // artifact under the 760px capture viewport).
        let anchor_rect = OutlineRect {
            x: 130.0,
            y: 310.0,
            w: 120.0,
            h: 28.0,
        };
        let fin = OutlineRect {
            x: 135.0,
            y: 281.0,
            w: 320.0,
            h: 467.0,
        };
        let settled = panel_rect_at(1.0, anchor_rect, fin, PopoverSide::Below);
        assert_eq!(settled.y, fin.y, "box top must follow the clamped panel");
        assert_eq!(settled.y + settled.h, fin.y + fin.h);
        let sliver = panel_rect_at(0.0, anchor_rect, fin, PopoverSide::Below);
        assert_eq!(
            sliver.y,
            anchor_rect.y + anchor_rect.h - POPOVER_BORDER_WIDTH_PX,
            "the animation still starts as a sliver at the seam"
        );
        assert!(sliver.h <= POPOVER_BORDER_WIDTH_PX + 1e-9);
    }

    #[test]
    fn clamped_above_outline_follows_the_panel_downward() {
        // Above-side twin: the panel clamped down across the trigger; the
        // settled bottom edge must be the panel's, not the trigger's top.
        let anchor_rect = OutlineRect {
            x: 130.0,
            y: 40.0,
            w: 120.0,
            h: 28.0,
        };
        let fin = OutlineRect {
            x: 135.0,
            y: 12.0,
            w: 320.0,
            h: 467.0,
        };
        let settled = panel_rect_at(1.0, anchor_rect, fin, PopoverSide::Above);
        assert_eq!(settled.y, fin.y);
        assert_eq!(settled.y + settled.h, fin.y + fin.h);
    }

    #[test]
    fn covered_trigger_is_detected_only_when_fully_inside_the_panel() {
        // The measured add-node-picker clamp case: trigger raw rect
        // (137.9, 311)–(250.2, 336); panel (134.9, 281)–(454.9, 748). The
        // panel's left edge welds EXACTLY onto the inflated trigger edge
        // (134.9 = 137.9 − 3), which the slack must treat as covered.
        let trigger = RectSnapshot {
            x: 137.9,
            y: 311.0,
            width: 112.3,
            height: 25.0,
        };
        let panel = SizeSnapshot {
            width: 320.0,
            height: 467.0,
        };
        let clamped = PopoverPosition {
            left: 134.9,
            top: 281.0,
            visible: true,
            side: PopoverSide::Below,
        };
        assert!(panel_covers_trigger(trigger, panel, clamped));
        // A panel whose left edge sits a few px INSIDE the trigger footprint
        // leaves trigger showing — not covered.
        let offset = PopoverPosition {
            left: 140.0,
            ..clamped
        };
        assert!(!panel_covers_trigger(trigger, panel, offset));
        // The ordinary welded panel below the trigger never covers it.
        let welded = PopoverPosition {
            left: 134.9,
            top: trigger.y + trigger.height - POPOVER_BORDER_WIDTH_PX,
            visible: true,
            side: PopoverSide::Below,
        };
        assert!(!panel_covers_trigger(trigger, panel, welded));
    }

    #[test]
    fn attached_placeholder_keeps_open_classes() {
        // The in-flow placeholder must keep the open-state layout classes
        // (size, display, border) alongside the paint-nothing override — an
        // emptied/bare button synthesizes its baseline at the bottom edge
        // and grows the surrounding line box, reflowing the page a few px
        // (docs/defects/2026-07-23-popover-open-resizes-card.md).
        let class = popover_button_class(true, true, "rest", "open");
        assert_eq!(class, "open ux-popover-trigger-placeholder");
        assert_eq!(popover_button_class(true, false, "rest", "open"), "open");
        assert_eq!(popover_button_class(false, false, "rest", "open"), "rest");
    }

    #[test]
    fn anchored_placeholder_never_pins_the_anchor_rect() {
        // The measured rect in anchored mode belongs to the ANCHOR element;
        // pinning it onto the hidden trigger hint would resize the hint to
        // the whole control (P2c item 3).
        let rect = RectSnapshot {
            x: 10.0,
            y: 10.0,
            width: 240.0,
            height: 80.0,
        };
        let pinned = trigger_placeholder_style(true, false, Some(rect));
        assert!(pinned.contains("width: 240.0px"));
        assert_eq!(trigger_placeholder_style(true, true, Some(rect)), "");
        assert_eq!(trigger_placeholder_style(false, false, Some(rect)), "");
    }
}
