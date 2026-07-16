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
/// Panel content starts fading in after this fraction of the open timeline.
const CONTENT_FADE_DELAY: f64 = 0.10;

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
/// top layer paints above everything, the trigger's content re-parents into
/// it while open; the in-flow button stays as an invisible size-pinned
/// placeholder holding layout and keyboard focus. Opening animates by
/// interpolating the panel's input rect and re-unioning each frame
/// (`prefers-reduced-motion` jumps to the settled shape). Decision record:
/// `docs/adr/2026-07-15-popover-svg-merged-outline.md`.
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
    children: Element,
) -> Element {
    let mut open = use_signal(|| initially_open);
    let trigger_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-trigger-{id}")
    });
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
    let trigger_placeholder = trigger_placeholder_style(attached, trigger_rect());
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
    let panel_style = format!("{} {panel_clip}", current_position.style());
    let content_style = panel_content_style(t);
    let (grad_stop_near, grad_stop_far) = gradient_stops(current_position.side);
    let trigger_visual_style = open_trigger_style(trigger_rect());
    let trigger_for_layer = trigger.clone();

    let trigger_id_for_click = trigger_id.clone();
    let trigger_id_for_effect = trigger_id.clone();
    let panel_id_for_effect = panel_id.clone();
    let layer_id_for_effect = layer_id.clone();
    let auto_update_for_effect = auto_update.clone();
    let trigger_id_for_layer_mount = trigger_id.clone();
    let panel_id_for_layer_mount = panel_id.clone();
    let trigger_id_for_panel_mount = trigger_id.clone();
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
                trigger_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
                stabilized,
            );
            ensure_popover_auto_update(
                auto_update_for_effect.clone(),
                trigger_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                layer_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
            );
        } else {
            auto_update_for_effect.borrow_mut().take();
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
                        // trigger's size on the very first attached frame.
                        if let Some(rect) = trigger_rect_by_id(&trigger_id_for_click) {
                            if !rect.is_empty() {
                                trigger_rect.set(Some(rect));
                            }
                        }
                    }
                    open.toggle();
                },
                // While attached, the trigger's content renders in the top
                // layer instead; this button stays as an invisible same-size
                // placeholder that keeps layout and keyboard focus.
                if !attached {
                    {trigger}
                }
            }
            if render_open() {
                div {
                    id: "{layer_id}",
                    class: "ux-popover-layer {chrome_class}",
                    "popover": "manual",
                    onmounted: move |_| {
                        show_popover_layer(&layer_id_for_layer_mount);
                        let trigger_id_for_panel = trigger_id_for_layer_mount.clone();
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
                            let trigger_id_for_panel = trigger_id_for_panel_mount.clone();
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
                        },
                        div { style: "{content_style}", {children} }
                    }
                    // The trigger's visual, re-parented into the top layer so
                    // it paints above the outline fill (the top layer covers
                    // the in-flow button). Presentational only: clicking it
                    // closes, focus stays on the in-flow placeholder button.
                    if attached {
                        div {
                            class: "ux-popover-open-trigger {open_class}",
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
            {children}
        }
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
        panel_size.set(Some(size));
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

fn trigger_rect_by_id(trigger_id: &str) -> Option<RectSnapshot> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.get_element_by_id(trigger_id)?;
    Some(RectSnapshot::from_dom_rect(
        element.get_bounding_client_rect(),
    ))
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
        // Content and chrome live in the top layer; the in-flow button is an
        // invisible placeholder pinned to the trigger's measured size.
        "ux-popover-trigger-placeholder".to_string()
    } else {
        open_class.to_string()
    }
}

fn popover_panel_class(popup_class: &str) -> String {
    // `ux-svg-popover-panel` strips the panel's own background/border/shadow;
    // the merged outline path draws all of that.
    format!("{popup_class} ux-popover-panel ux-svg-popover-panel")
}

/// Inline size pin for the in-flow placeholder button while attached, so
/// swapping its content into the top layer cannot shift layout.
fn trigger_placeholder_style(attached: bool, rect: Option<RectSnapshot>) -> String {
    match (attached, rect) {
        (true, Some(rect)) => format!("width: {:.1}px; height: {:.1}px;", rect.width, rect.height),
        _ => String::new(),
    }
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
fn panel_rect_at(t: f64, anchor: OutlineRect, fin: OutlineRect, side: PopoverSide) -> OutlineRect {
    let eased = ease_out_cubic(t);
    let left = lerp(anchor.x, fin.x, eased);
    let right = lerp(anchor.x + anchor.w, fin.x + fin.w, eased);
    match side {
        PopoverSide::Below => {
            let top = anchor.y + anchor.h - POPOVER_BORDER_WIDTH_PX;
            let bottom = lerp(anchor.y + anchor.h, fin.y + fin.h, eased);
            OutlineRect {
                x: left,
                y: top,
                w: right - left,
                h: (bottom - top).max(0.0),
            }
        }
        PopoverSide::Above => {
            let bottom = anchor.y + POPOVER_BORDER_WIDTH_PX;
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
        let left = panel_left(anchor, panel, placement.align(), viewport_width);

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

/// Panel left edge: the aligned position, clamped inside the viewport margin.
/// Any trigger/panel alignment produces a valid merged outline (aligned edges
/// weld; offsets get concave fillets), so no corner-visibility logic is
/// needed here anymore.
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
    desired.clamp(POPOVER_MARGIN_PX, max_left)
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
