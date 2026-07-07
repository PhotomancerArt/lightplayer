use dioxus::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use wasm_bindgen::{JsCast, closure::Closure};

use crate::base::{StudioIcon, StudioIconName};

static NEXT_POPOVER_ID: AtomicUsize = AtomicUsize::new(1);

const POPOVER_MARGIN_PX: f64 = 12.0;
const POPOVER_BORDER_WIDTH_PX: f64 = 1.0;
const POPOVER_CORNER_RADIUS_PX: f64 = 8.0;
const FALLBACK_PANEL_WIDTH_PX: f64 = 280.0;
const FALLBACK_PANEL_HEIGHT_PX: f64 = 180.0;
const MEASURE_RETRY_LIMIT: u8 = 3;
const STABILIZE_MEASURE_DELAYS_MS: [i32; 2] = [50, 250];

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
    let trigger_rect = use_signal(|| None::<RectSnapshot>);
    let mut panel_size = use_signal(|| None::<SizeSnapshot>);
    let position = use_signal(|| PopoverPosition::hidden(placement));
    let auto_update = use_hook(|| Rc::new(RefCell::new(None::<PopoverAutoUpdate>)));
    let current_position = position();
    let button_class =
        popover_button_class(open(), &class, &open_class, &chrome_class, current_position);
    let panel_class = popover_panel_class(&popup_class, &chrome_class, current_position);
    let bridge_class = popover_bridge_class(&chrome_class, current_position);
    let panel_style = current_position.style();
    let bridge_style = current_position.bridge_style();

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
    use_effect(move || {
        if open() {
            show_popover_layer(&layer_id_for_effect);
            measure_trigger_with_stabilization(
                trigger_id_for_effect.clone(),
                panel_id_for_effect.clone(),
                panel_size,
                trigger_rect,
                position,
                placement,
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
            hide_popover_layer(&layer_id_for_effect);
            auto_update_for_effect.borrow_mut().take();
        }
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
                style: "cursor: pointer;",
                r#type: "button",
                aria_label: "{label}",
                title: "{title}",
                aria_expanded: "{open()}",
                onclick: move |event| {
                    event.stop_propagation();
                    open.toggle();
                },
                {trigger}
            }
            if open() {
                div {
                    id: "{layer_id}",
                    class: "ux-popover-layer",
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
                    div {
                        id: "{panel_id}",
                        class: "{panel_class}",
                        style: "{panel_style}",
                        role: "dialog",
                        "data-story-wait": if current_position.visible { "0" } else { "1" },
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
                        {children}
                    }
                    div {
                        class: "{bridge_class}",
                        style: "{bridge_style}",
                        aria_hidden: "true",
                        span { class: "ux-popover-bridge-corner ux-popover-bridge-corner-left" }
                        span { class: "ux-popover-bridge-corner ux-popover-bridge-corner-right" }
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
) {
    measure_trigger_once(
        trigger_id.clone(),
        panel_id.clone(),
        panel_size,
        trigger_rect,
        position,
        placement,
    );
    for delay_ms in STABILIZE_MEASURE_DELAYS_MS {
        schedule_delayed_measure_trigger(
            trigger_id.clone(),
            panel_id.clone(),
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

fn schedule_delayed_measure_trigger(
    trigger_id: String,
    panel_id: String,
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
    schedule_measure_trigger_element(
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

fn popover_button_class(
    open: bool,
    class: &str,
    open_class: &str,
    chrome_class: &str,
    position: PopoverPosition,
) -> String {
    if !open {
        return class.to_string();
    }

    format!(
        "{open_class} {chrome_class} ux-popover-trigger-attached ux-popover-trigger-attached-{}",
        position.side.class_token()
    )
}

fn popover_panel_class(popup_class: &str, chrome_class: &str, position: PopoverPosition) -> String {
    format!(
        "{popup_class} {chrome_class} ux-popover-panel ux-attached-popover-panel ux-attached-popover-panel-{} {}",
        position.side.class_token(),
        position.panel_corner_class()
    )
}

fn popover_bridge_class(chrome_class: &str, position: PopoverPosition) -> String {
    format!(
        "{chrome_class} ux-popover-bridge ux-popover-bridge-{} {}",
        position.side.class_token(),
        position.bridge_corner_class()
    )
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
    bridge_left: f64,
    bridge_top: f64,
    bridge_width: f64,
    bridge_height: f64,
    visible: bool,
    side: PopoverSide,
    show_left_corner: bool,
    show_right_corner: bool,
}

impl PopoverPosition {
    fn hidden(placement: PopoverPlacement) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            bridge_left: 0.0,
            bridge_top: 0.0,
            bridge_width: 0.0,
            bridge_height: POPOVER_BORDER_WIDTH_PX,
            visible: false,
            side: placement.side(),
            show_left_corner: true,
            show_right_corner: true,
        }
    }

    fn from_anchor(anchor: RectSnapshot, panel: SizeSnapshot, placement: PopoverPlacement) -> Self {
        let (viewport_width, viewport_height) = viewport_size();
        let side = placement.side().resolve(anchor, panel, viewport_height);
        let top = side.panel_top(anchor, panel, viewport_height);
        let horizontal =
            HorizontalAttachment::from_anchor(anchor, panel, placement.align(), viewport_width);
        let bridge_top = side.bridge_top(anchor);

        Self {
            left: horizontal.left,
            top,
            bridge_left: anchor.x,
            bridge_top,
            bridge_width: anchor.width,
            bridge_height: POPOVER_BORDER_WIDTH_PX,
            visible: true,
            side,
            show_left_corner: horizontal.show_left_corner,
            show_right_corner: horizontal.show_right_corner,
        }
    }

    fn style(self) -> String {
        let visibility = if self.visible { "visible" } else { "hidden" };
        format!(
            "left: {:.1}px; top: {:.1}px; visibility: {visibility};",
            self.left, self.top
        )
    }

    fn bridge_style(self) -> String {
        let visibility = if self.visible { "visible" } else { "hidden" };
        format!(
            "left: {:.1}px; top: {:.1}px; width: {:.1}px; height: {:.1}px; visibility: {visibility};",
            self.bridge_left, self.bridge_top, self.bridge_width, self.bridge_height
        )
    }

    fn panel_corner_class(self) -> &'static str {
        match (self.side, self.show_left_corner, self.show_right_corner) {
            (PopoverSide::Below, false, true) => "ux-attached-popover-panel-square-top-left",
            (PopoverSide::Below, true, false) => "ux-attached-popover-panel-square-top-right",
            (PopoverSide::Above, false, true) => "ux-attached-popover-panel-square-bottom-left",
            (PopoverSide::Above, true, false) => "ux-attached-popover-panel-square-bottom-right",
            _ => "",
        }
    }

    fn bridge_corner_class(self) -> &'static str {
        match (self.show_left_corner, self.show_right_corner) {
            (false, true) => "ux-popover-bridge-no-left-corner",
            (true, false) => "ux-popover-bridge-no-right-corner",
            _ => "",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopoverSide {
    Above,
    Below,
}

impl PopoverSide {
    fn class_token(self) -> &'static str {
        match self {
            Self::Above => "above",
            Self::Below => "below",
        }
    }

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

    fn bridge_top(self, anchor: RectSnapshot) -> f64 {
        match self {
            Self::Below => anchor.y + anchor.height - POPOVER_BORDER_WIDTH_PX,
            Self::Above => anchor.y,
        }
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

#[derive(Clone, Copy, Debug)]
struct HorizontalAttachment {
    left: f64,
    show_left_corner: bool,
    show_right_corner: bool,
}

impl HorizontalAttachment {
    fn from_anchor(
        anchor: RectSnapshot,
        panel: SizeSnapshot,
        align: PopoverAlign,
        viewport_width: f64,
    ) -> Self {
        let desired_viewport_left = match align {
            PopoverAlign::Start => anchor.x,
            PopoverAlign::Middle => anchor.x + (anchor.width - panel.width) / 2.0,
            PopoverAlign::End => anchor.x + anchor.width - panel.width,
        };
        let max_left = (viewport_width - panel.width - POPOVER_MARGIN_PX).max(POPOVER_MARGIN_PX);
        let viewport_left = desired_viewport_left.clamp(POPOVER_MARGIN_PX, max_left);
        Self::from_viewport_left(anchor, panel, viewport_left, desired_viewport_left)
    }

    fn from_viewport_left(
        anchor: RectSnapshot,
        panel: SizeSnapshot,
        viewport_left: f64,
        desired_viewport_left: f64,
    ) -> Self {
        let bridge_left_in_panel = anchor.x - viewport_left;
        let bridge_right_in_panel = bridge_left_in_panel + anchor.width;
        let corner_clearance = (POPOVER_CORNER_RADIUS_PX - POPOVER_BORDER_WIDTH_PX).max(0.0);
        let show_left_corner = bridge_left_in_panel >= corner_clearance;
        let show_right_corner = panel.width - bridge_right_in_panel >= corner_clearance;

        match (show_left_corner, show_right_corner) {
            (true, true) | (false, true) | (true, false) => Self {
                left: viewport_left,
                show_left_corner,
                show_right_corner,
            },
            (false, false) => Self::nearest_edge(anchor, panel, desired_viewport_left),
        }
    }

    fn nearest_edge(anchor: RectSnapshot, panel: SizeSnapshot, desired_viewport_left: f64) -> Self {
        let start_viewport_left = anchor.x;
        let end_viewport_left = anchor.x + anchor.width - panel.width;
        if (desired_viewport_left - start_viewport_left).abs()
            <= (desired_viewport_left - end_viewport_left).abs()
        {
            return Self {
                left: start_viewport_left,
                show_left_corner: false,
                show_right_corner: true,
            };
        }

        Self {
            left: end_viewport_left,
            show_left_corner: true,
            show_right_corner: false,
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
