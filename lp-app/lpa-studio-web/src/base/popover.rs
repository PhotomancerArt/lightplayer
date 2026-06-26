use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::base::{StudioIcon, StudioIconName};

static NEXT_POPOVER_ID: AtomicUsize = AtomicUsize::new(1);

const POPOVER_MARGIN_PX: f64 = 12.0;
const POPOVER_GAP_PX: f64 = 8.0;
const FALLBACK_PANEL_WIDTH_PX: f64 = 280.0;
const FALLBACK_PANEL_HEIGHT_PX: f64 = 180.0;
const MEASURE_RETRY_LIMIT: u8 = 3;
const STABILIZE_MEASURE_DELAYS_MS: [i32; 2] = [50, 250];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PopoverPlacement {
    BottomStart,
    BottomEnd,
}

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
    #[props(default = PopoverPlacement::BottomEnd)] placement: PopoverPlacement,
    #[props(default = false)] initially_open: bool,
    children: Element,
) -> Element {
    let mut open = use_signal(|| initially_open);
    let trigger_id = use_hook(|| {
        let id = NEXT_POPOVER_ID.fetch_add(1, Ordering::Relaxed);
        format!("ux-popover-trigger-{id}")
    });
    let trigger_rect = use_signal(|| None::<RectSnapshot>);
    let mut panel_size = use_signal(|| None::<SizeSnapshot>);
    let position = use_signal(|| PopoverPosition::hidden());
    let current_position = position();
    let button_class = popover_button_class(open(), &class, &open_class);
    let panel_class = popover_panel_class(&popup_class);
    let panel_style = current_position.style();

    let trigger_id_for_effect = trigger_id.clone();
    let trigger_id_for_mount = trigger_id.clone();
    use_effect(move || {
        if open() {
            measure_trigger_with_stabilization(
                trigger_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
            );
        }
    });

    rsx! {
        span { class: "tw:relative tw:inline-grid tw:min-w-0 tw:place-items-center",
            button {
                id: "{trigger_id}",
                class: "{button_class}",
                style: "cursor: pointer;",
                r#type: "button",
                aria_label: "{label}",
                title: "{title}",
                aria_expanded: "{open()}",
                onclick: move |_| {
                    open.toggle();
                },
                StudioIcon {
                    name: icon,
                    size: icon_size,
                }
            }
            if open() {
                div {
                    class: "tw:fixed tw:inset-0 tw:z-[70] tw:bg-transparent",
                    aria_hidden: "true",
                    onclick: move |_| open.set(false),
                }
                div {
                    class: "{panel_class}",
                    style: "{panel_style}",
                    role: "dialog",
                    "data-story-wait": if current_position.visible { "0" } else { "1" },
                    onmounted: move |event| {
                        let trigger_id_for_panel = trigger_id_for_mount.clone();
                        let panel_element = event.data();
                        spawn(async move {
                            let Ok(rect) = panel_element.get_client_rect().await else {
                                return;
                            };
                            let size = SizeSnapshot::from_pixels_rect(rect);
                            panel_size.set(Some(size));
                            measure_trigger_once(
                                trigger_id_for_panel,
                                panel_size,
                                trigger_rect,
                                position,
                                placement,
                            );
                        });
                    },
                    {children}
                }
            }
        }
    }
}

fn measure_trigger_with_stabilization(
    trigger_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    measure_trigger_once(
        trigger_id.clone(),
        panel_size,
        trigger_rect,
        position,
        placement,
    );
    for delay_ms in STABILIZE_MEASURE_DELAYS_MS {
        schedule_delayed_measure_trigger(
            trigger_id.clone(),
            panel_size,
            trigger_rect,
            position,
            placement,
            delay_ms,
        );
    }
}

fn measure_trigger_once(
    trigger_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    let current_panel_size = panel_size();
    measure_trigger_element(
        trigger_id,
        current_panel_size,
        trigger_rect,
        position,
        placement,
    );
}

fn schedule_delayed_measure_trigger(
    trigger_id: String,
    panel_size: Signal<Option<SizeSnapshot>>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    delay_ms: i32,
) {
    let Some(window) = web_sys::window() else {
        return;
    };

    let callback = Closure::once(move || {
        measure_trigger_once(trigger_id, panel_size, trigger_rect, position, placement);
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
    current_panel_size: Option<SizeSnapshot>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    schedule_measure_trigger_element(
        trigger_id,
        current_panel_size,
        trigger_rect,
        position,
        placement,
        0,
    );
}

fn schedule_measure_trigger_element(
    trigger_id: String,
    current_panel_size: Option<SizeSnapshot>,
    trigger_rect: Signal<Option<RectSnapshot>>,
    position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
    attempt: u8,
) {
    let Some(window) = web_sys::window() else {
        spawn_measure_trigger_element(
            trigger_id,
            current_panel_size,
            trigger_rect,
            position,
            placement,
            attempt,
        );
        return;
    };

    let fallback_trigger_id = trigger_id.clone();
    let fallback_trigger_rect = trigger_rect;
    let fallback_position = position;
    let callback = Closure::once(move || {
        spawn_measure_trigger_element(
            trigger_id,
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

fn popover_button_class(open: bool, class: &str, open_class: &str) -> String {
    if !open {
        return class.to_string();
    }

    open_class.to_string()
}

fn popover_panel_class(popup_class: &str) -> String {
    format!("{popup_class} ux-popover-panel")
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
}

#[derive(Clone, Copy, Debug)]
struct PopoverPosition {
    left: f64,
    top: f64,
    visible: bool,
}

impl PopoverPosition {
    fn hidden() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            visible: false,
        }
    }

    fn from_anchor(anchor: RectSnapshot, panel: SizeSnapshot, placement: PopoverPlacement) -> Self {
        let (viewport_width, viewport_height) = viewport_size();
        let preferred_left = match placement {
            PopoverPlacement::BottomStart => anchor.x,
            PopoverPlacement::BottomEnd => anchor.x + anchor.width - panel.width,
        };
        let max_left = (viewport_width - panel.width - POPOVER_MARGIN_PX).max(POPOVER_MARGIN_PX);
        let left = preferred_left.clamp(POPOVER_MARGIN_PX, max_left);

        let below = anchor.y + anchor.height + POPOVER_GAP_PX;
        let above = anchor.y - panel.height - POPOVER_GAP_PX;
        let max_top = viewport_height - panel.height - POPOVER_MARGIN_PX;
        let top = if below <= max_top || above < POPOVER_MARGIN_PX {
            below.min(max_top).max(POPOVER_MARGIN_PX)
        } else {
            above.max(POPOVER_MARGIN_PX)
        };

        Self {
            left,
            top,
            visible: true,
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
