use dioxus::prelude::*;
use std::rc::Rc;

use crate::base::{StudioIcon, StudioIconName};

const POPOVER_MARGIN_PX: f64 = 12.0;
const POPOVER_GAP_PX: f64 = 8.0;
const FALLBACK_PANEL_WIDTH_PX: f64 = 280.0;
const FALLBACK_PANEL_HEIGHT_PX: f64 = 180.0;

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
    let mut trigger = use_signal(|| None::<Rc<MountedData>>);
    let trigger_rect = use_signal(|| None::<RectSnapshot>);
    let mut panel_size = use_signal(|| None::<SizeSnapshot>);
    let mut position = use_signal(|| PopoverPosition::hidden());
    let button_class = if open() { open_class } else { class };
    let panel_class = format!("{popup_class} ux-popover-panel");
    let panel_style = position().style();

    rsx! {
        span { class: "tw:relative tw:inline-grid tw:min-w-0 tw:place-items-center",
            button {
                class: "{button_class}",
                style: "cursor: pointer;",
                r#type: "button",
                aria_label: "{label}",
                title: "{title}",
                aria_expanded: "{open()}",
                onmounted: move |event| {
                    trigger.set(Some(event.data()));
                    if open() {
                        measure_trigger(trigger, panel_size, trigger_rect, position, placement);
                    }
                },
                onclick: move |event| {
                    event.stop_propagation();
                    let next_open = !open();
                    open.set(next_open);
                    if next_open {
                        measure_trigger(trigger, panel_size, trigger_rect, position, placement);
                    }
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
                    onclick: move |event| {
                        event.stop_propagation();
                        open.set(false);
                    },
                }
                div {
                    class: "{panel_class}",
                    style: "{panel_style}",
                    role: "dialog",
                    onclick: move |event| event.stop_propagation(),
                    onmounted: move |event| {
                        let panel_element = event.data();
                        let anchor = trigger_rect();
                        spawn(async move {
                            let Ok(rect) = panel_element.get_client_rect().await else {
                                return;
                            };
                            let size = SizeSnapshot::from_pixels_rect(rect);
                            panel_size.set(Some(size));
                            if let Some(anchor) = anchor {
                                position.set(PopoverPosition::from_anchor(anchor, size, placement));
                            }
                        });
                    },
                    {children}
                }
            }
        }
    }
}

fn measure_trigger(
    trigger: Signal<Option<Rc<MountedData>>>,
    panel_size: Signal<Option<SizeSnapshot>>,
    mut trigger_rect: Signal<Option<RectSnapshot>>,
    mut position: Signal<PopoverPosition>,
    placement: PopoverPlacement,
) {
    let Some(trigger_element) = trigger.read().as_ref().cloned() else {
        return;
    };
    let current_panel_size = panel_size();
    spawn(async move {
        let Ok(rect) = trigger_element.get_client_rect().await else {
            return;
        };
        let anchor = RectSnapshot::from_pixels_rect(rect);
        let size = current_panel_size.unwrap_or_else(SizeSnapshot::fallback);
        trigger_rect.set(Some(anchor));
        position.set(PopoverPosition::from_anchor(anchor, size, placement));
    });
}

#[derive(Clone, Copy, Debug)]
struct RectSnapshot {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl RectSnapshot {
    fn from_pixels_rect(rect: dioxus::html::geometry::PixelsRect) -> Self {
        Self {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
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
