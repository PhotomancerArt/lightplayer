//! The gradient editor — the chooser's TAKEOVER view (M4 P5).
//!
//! Spike §9's `ddview-edit`: ✎ on a cycle chip or a project row swaps the
//! WHOLE popover content for this, rather than opening a second surface next
//! to the first. One popover, two views, because editing a palette is not a
//! detail of choosing one — it is the other thing you can do with the same
//! value, and a nested popup over a popup has nowhere to go on a phone.
//!
//! Two rules make the takeover safe to hand a running show:
//!
//! - **Nothing is written while editing.** The draft lives here, in local
//!   signals; `on_done` fires ONCE with the finished gradient and `on_cancel`
//!   discards it. Every other palette gesture emits on each touch (the actor
//!   coalesces a slider's flood), but a stop drag would put half-built ramps
//!   on the channel for as long as the user hunts for a color.
//! - **The catalog is never edited.** Editing a built-in edits a copy; where
//!   the copy LANDS is the caller's business ([`super::palette_chooser`]
//!   puts it back wherever the edit came from), and the provenance line says
//!   which of the two is happening.
//!
//! Only two of [`Colorspace`]'s six spaces are exposed — sRGB and Oklab.
//! The model stores all six and a loaded palette in any of them edits fine
//! (the segment simply shows neither as active); what the editor does not do
//! is offer Hsl/Hsv/Oklch as authoring choices, because "which space do I
//! interpolate in" is a two-answer question in practice (D8: imports stay
//! sRGB for WLED fidelity, new palettes want Oklab).

use dioxus::prelude::*;
use lpa_palettes::{from_display_srgb, sample_linear, sample_step, to_display_srgb};
use lpc_model::{
    Colorspace, Gradient, GradientStop, InterpMethod, MAX_GRADIENT_STOPS, MIN_GRADIENT_STOPS,
};

use crate::base::{GradientStripCanvas, StudioIcon, StudioIconName};

/// How close (in bar pixels) a pointer has to land to grab a stop instead of
/// missing it. Also the dead zone the double-click ADD respects, so
/// "double-click the bar" never means "double-click this stop".
const GRAB_RADIUS_PX: f64 = 11.0;

/// Where an edited gradient came from — the whole content of the provenance
/// line, and the difference between forking and editing in place.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PaletteOrigin {
    /// The gradient is (still) a shipped catalog palette: done lands a COPY
    /// as the authored value and the catalog entry stays untouched.
    BuiltinCopy(String),
    /// A palette this project already authored: done edits it in place.
    ProjectCustom,
}

impl PaletteOrigin {
    /// The provenance line under the title.
    #[must_use]
    pub fn provenance(&self) -> String {
        match self {
            Self::BuiltinCopy(name) => format!("copy of built-in \u{201c}{name}\u{201d}"),
            Self::ProjectCustom => "project custom".to_string(),
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PaletteEditor(
    /// The palette the editor OPENS on. Copied into the draft on mount; the
    /// caller's value is not touched again until `on_done`.
    gradient: Gradient,
    /// The palette's name, for the title row.
    name: String,
    origin: PaletteOrigin,
    /// Fires once, with the finished gradient. The only write this view makes.
    on_done: EventHandler<Gradient>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| gradient.clone());
    let mut selected = use_signal(|| 0usize);
    let mut dragging = use_signal(|| false);

    let current = draft();
    let stop_count = current.stops.len();
    let index = selected().min(stop_count.saturating_sub(1));
    let stop = current.stops.get(index).copied().unwrap_or_default();
    let can_remove = stop_count > MIN_GRADIENT_STOPS as usize;
    let can_add = stop_count < MAX_GRADIENT_STOPS as usize;

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-2 tw:p-2",
            // Title row: what is being edited, where it will land, and the
            // two ways out.
            div { class: "tw:flex tw:min-w-0 tw:items-start tw:gap-2",
                span { class: "tw:mt-0.5 tw:flex tw:flex-none tw:text-subtle-foreground", aria_hidden: "true",
                    StudioIcon { name: StudioIconName::Edited, size: 12 }
                }
                div { class: "tw:grid tw:min-w-0 tw:grow tw:gap-0.5",
                    span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-bold tw:text-strong-foreground",
                        "{name}"
                    }
                    span { class: "tw:min-w-0 tw:truncate tw:text-[10px] tw:text-subtle-foreground",
                        "{origin.provenance()}"
                    }
                }
                button {
                    class: "tw:flex-none tw:cursor-pointer tw:appearance-none tw:rounded-xs tw:border tw:border-border-subtle tw:bg-card-muted tw:px-2 tw:py-0.5 tw:text-[11px] tw:font-bold tw:text-strong-foreground",
                    r#type: "button",
                    title: "Keep this palette",
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        on_done.call(draft());
                    },
                    "done"
                }
                button {
                    class: "tw:flex tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:border-0 tw:bg-transparent tw:p-0 tw:text-subtle-foreground tw:hover:text-strong-foreground",
                    r#type: "button",
                    title: "Discard these edits",
                    aria_label: "Discard these edits",
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        on_cancel.call(());
                    },
                    StudioIcon { name: StudioIconName::Cancel, size: 12 }
                }
            }

            // The bar: the live strip, the stop handles over it, and the only
            // pointer surface in the editor. Handles are `pointer-events:
            // none` so every gesture reports coordinates against the BAR
            // (the XY pad's rule) instead of whichever child was hit.
            div {
                class: "tw:relative tw:h-8 tw:w-full tw:min-w-0 tw:cursor-crosshair tw:select-none",
                style: "touch-action: none;",
                onpointerdown: move |event: Event<PointerData>| {
                    let Some((at, radius)) = bar_position(&event) else {
                        return;
                    };
                    let Some(hit) = stop_index_near(&draft(), at, radius) else {
                        return;
                    };
                    capture_bar_pointer(&event);
                    selected.set(hit);
                    dragging.set(true);
                },
                onpointermove: move |event: Event<PointerData>| {
                    if !dragging() {
                        return;
                    }
                    if event.data().held_buttons().is_empty() {
                        dragging.set(false);
                        return;
                    }
                    let Some((at, _)) = bar_position(&event) else {
                        return;
                    };
                    let moved = with_stop_at(&draft(), selected(), at);
                    draft.set(moved);
                },
                onpointerup: move |_| dragging.set(false),
                onpointercancel: move |_| dragging.set(false),
                // Double-click on empty bar adds a stop AT THE COLOR ALREADY
                // THERE, so the ramp does not jump when a stop appears.
                ondoubleclick: move |event: Event<MouseData>| {
                    event.stop_propagation();
                    let Some((at, radius)) = bar_click_position(&event) else {
                        return;
                    };
                    let current = draft();
                    if stop_index_near(&current, at, radius).is_some() {
                        return;
                    }
                    let (added, index) = with_stop_added(&current, at);
                    draft.set(added);
                    selected.set(index);
                },
                div { class: "tw:pointer-events-none tw:absolute tw:inset-0",
                    GradientStripCanvas { gradient: current.clone() }
                }
                for (position , handle) in current.stops.iter().enumerate() {
                    div {
                        key: "{position}",
                        class: stop_handle_class(position == index),
                        style: "{stop_handle_style(handle.at, stop_display_srgb(&current, position))}",
                    }
                }
            }

            // The selected stop, exactly.
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-2",
                label { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1 tw:text-[11px] tw:text-subtle-foreground",
                    "stop"
                    input {
                        class: "tw:h-5 tw:w-8 tw:cursor-pointer tw:border tw:border-border-subtle tw:bg-page tw:p-0",
                        r#type: "color",
                        value: "{hex_of(stop_display_srgb(&current, index))}",
                        title: "The selected stop's color",
                        oninput: move |event: FormEvent| {
                            let Some(srgb) = srgb_from_hex(&event.value()) else {
                                return;
                            };
                            let recolored = with_stop_color(&draft(), selected(), srgb);
                            draft.set(recolored);
                        },
                    }
                }
                label { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1 tw:text-[11px] tw:text-subtle-foreground",
                    "at"
                    input {
                        class: "tw:w-14 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-1 tw:py-0.5 tw:text-right tw:font-mono tw:text-[11px] tw:tabular-nums tw:text-strong-foreground",
                        r#type: "number",
                        min: "0",
                        max: "1",
                        step: "0.01",
                        value: "{stop.at:.2}",
                        title: "Where the stop sits along the ramp",
                        oninput: move |event: FormEvent| {
                            let Ok(at) = event.value().parse::<f32>() else {
                                return;
                            };
                            let moved = with_stop_at(&draft(), selected(), at);
                            draft.set(moved);
                        },
                    }
                }
                button {
                    class: "tw:inline-flex tw:cursor-pointer tw:appearance-none tw:items-center tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-transparent tw:px-1.5 tw:py-0.5 tw:text-[11px] tw:text-subtle-foreground tw:hover:text-status-error-foreground tw:disabled:cursor-default tw:disabled:opacity-50",
                    r#type: "button",
                    disabled: !can_remove,
                    title: if can_remove {
                        "Delete the selected stop"
                    } else {
                        "A gradient needs at least two stops"
                    },
                    onclick: move |event: MouseEvent| {
                        event.stop_propagation();
                        let trimmed = with_stop_removed(&draft(), selected());
                        selected.set(0);
                        draft.set(trimmed);
                    },
                    StudioIcon { name: StudioIconName::Remove, size: 11 }
                    "delete"
                }
                if !can_add {
                    span { class: "tw:text-[10px] tw:text-status-attention-foreground",
                        "{MAX_GRADIENT_STOPS} stops is the most a gradient holds."
                    }
                }
            }

            // How the ramp is READ: the space it interpolates in, and the
            // method it interpolates with.
            div { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:justify-between tw:gap-2",
                div { class: "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle",
                    for (label , space) in EDITOR_SPACES {
                        button {
                            key: "{label}",
                            class: super::palette_chooser::seg_button_class(current.space == space),
                            r#type: "button",
                            title: "Interpolate this palette in this space",
                            onclick: move |event: MouseEvent| {
                                event.stop_propagation();
                                let respaced = with_space(&draft(), space);
                                draft.set(respaced);
                            },
                            "{label}"
                        }
                    }
                }
                div { class: "tw:inline-flex tw:overflow-hidden tw:rounded-xs tw:border tw:border-border-subtle",
                    for (label , method) in EDITOR_METHODS {
                        button {
                            key: "{label}",
                            class: super::palette_chooser::seg_button_class(current.method == method),
                            r#type: "button",
                            title: "How a sample between two stops is taken",
                            onclick: move |event: MouseEvent| {
                                event.stop_propagation();
                                let mut next = draft();
                                next.method = method;
                                draft.set(next);
                            },
                            "{label}"
                        }
                    }
                }
            }
        }
    }
}

/// The two spaces the editor offers (see the module docs for why not six).
const EDITOR_SPACES: [(&str, Colorspace); 2] =
    [("sRGB", Colorspace::Srgb), ("Oklab", Colorspace::Oklab)];

const EDITOR_METHODS: [(&str, InterpMethod); 3] = [
    ("Step", InterpMethod::Step),
    ("Linear", InterpMethod::Linear),
    ("Smooth", InterpMethod::Smooth),
];

/// The stop a pointer at `at` grabs: the nearest one within `radius`
/// (expressed as a fraction of the bar), or `None` — which is what makes
/// "double-click empty bar to add" distinguishable from "grab a stop".
#[must_use]
pub fn stop_index_near(gradient: &Gradient, at: f32, radius: f32) -> Option<usize> {
    gradient
        .stops
        .iter()
        .enumerate()
        .map(|(index, stop)| (index, (stop.at - at).abs()))
        .filter(|(_, distance)| *distance <= radius)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(index, _)| index)
}

/// Move one stop. Positions clamp to `[0, 1]` — the model's own validity
/// bound — and the list is NOT resorted, so a stop dragged past its
/// neighbour keeps its identity (and its selection) under the pointer.
#[must_use]
pub fn with_stop_at(gradient: &Gradient, index: usize, at: f32) -> Gradient {
    let mut next = gradient.clone();
    if let Some(stop) = next.stops.get_mut(index) {
        stop.at = if at.is_finite() { at.clamp(0.0, 1.0) } else { 0.0 };
    }
    next
}

/// Recolor one stop from a DISPLAY sRGB color (what the color well speaks)
/// into the gradient's own space.
#[must_use]
pub fn with_stop_color(gradient: &Gradient, index: usize, srgb: [f32; 3]) -> Gradient {
    let mut next = gradient.clone();
    let space = next.space;
    if let Some(stop) = next.stops.get_mut(index) {
        stop.c = from_display_srgb(space, srgb);
    }
    next
}

/// Add a stop at `at`, colored with what the ramp ALREADY shows there, and
/// report its index. A full gradient is returned unchanged (with the index
/// of the nearest stop), because the bound is the model's, not the editor's.
#[must_use]
pub fn with_stop_added(gradient: &Gradient, at: f32) -> (Gradient, usize) {
    if gradient.stops.len() >= MAX_GRADIENT_STOPS as usize {
        return (gradient.clone(), 0);
    }
    let at = if at.is_finite() { at.clamp(0.0, 1.0) } else { 0.0 };
    let mut next = gradient.clone();
    next.stops.push(GradientStop {
        at,
        c: sample_in_space(gradient, at),
    });
    let index = next.stops.len() - 1;
    (next, index)
}

/// Remove a stop, unless that would take the gradient below the model's
/// two-stop floor — at which point there is no ramp left to describe.
#[must_use]
pub fn with_stop_removed(gradient: &Gradient, index: usize) -> Gradient {
    if gradient.stops.len() <= MIN_GRADIENT_STOPS as usize || index >= gradient.stops.len() {
        return gradient.clone();
    }
    let mut next = gradient.clone();
    next.stops.remove(index);
    next
}

/// Re-author the gradient in `space`, CONVERTING every stop so the palette
/// keeps looking like itself.
///
/// The alternative — reinterpreting the same numbers in the new space — is
/// what the D8 comparison card does deliberately, and it is the wrong answer
/// for an editor: "interpolate in Oklab" is a statement about the blend
/// between my colors, not a request to replace them.
#[must_use]
pub fn with_space(gradient: &Gradient, space: Colorspace) -> Gradient {
    if gradient.space == space {
        return gradient.clone();
    }
    Gradient {
        space,
        method: gradient.method,
        stops: gradient
            .stops
            .iter()
            .map(|stop| GradientStop {
                at: stop.at,
                c: from_display_srgb(space, to_display_srgb(gradient.space, stop.c)),
            })
            .collect(),
    }
}

/// One stop's color as display sRGB — what the handle dot and the color well
/// show.
#[must_use]
pub fn stop_display_srgb(gradient: &Gradient, index: usize) -> [f32; 3] {
    gradient
        .stops
        .get(index)
        .map_or([0.0, 0.0, 0.0], |stop| {
            to_display_srgb(gradient.space, stop.c)
        })
}

/// `#rrggbb` for an `<input type="color">`.
#[must_use]
pub fn hex_of(srgb: [f32; 3]) -> String {
    let byte = |channel: f32| (channel.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(srgb[0]),
        byte(srgb[1]),
        byte(srgb[2])
    )
}

/// Parse what an `<input type="color">` reports back.
#[must_use]
pub fn srgb_from_hex(hex: &str) -> Option<[f32; 3]> {
    let digits = hex.strip_prefix('#').unwrap_or(hex);
    if digits.len() != 6 {
        return None;
    }
    let channel = |start: usize| {
        u8::from_str_radix(&digits[start..start + 2], 16)
            .ok()
            .map(|byte| f32::from(byte) / 255.0)
    };
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// The color the ramp shows at `at`, in the gradient's OWN space (not
/// display sRGB) — an added stop has to be storable, not merely visible.
fn sample_in_space(gradient: &Gradient, at: f32) -> [f32; 3] {
    let mut stops = gradient.stops.clone();
    stops.sort_by(|a, b| a.at.total_cmp(&b.at));
    match gradient.method {
        InterpMethod::Step => sample_step(&stops, at),
        InterpMethod::Linear | InterpMethod::Smooth => sample_linear(&stops, at),
    }
}

/// The handle's position and its dot color. Inline because both are per-stop
/// numbers; everything static rides [`stop_handle_class`].
#[must_use]
pub fn stop_handle_style(at: f32, srgb: [f32; 3]) -> String {
    format!(
        "left: {:.2}%; background: {};",
        at.clamp(0.0, 1.0) * 100.0,
        hex_of(srgb)
    )
}

fn stop_handle_class(selected: bool) -> String {
    let base = "tw:pointer-events-none tw:absolute tw:top-1/2 tw:h-3.5 tw:w-3.5 tw:-translate-x-1/2 tw:-translate-y-1/2 tw:rounded-full tw:border-2 tw:shadow-[0_1px_3px_rgb(0_0_0/0.55)]";
    if selected {
        format!("{base} tw:border-[var(--studio-color-text-strong)]")
    } else {
        format!("{base} tw:border-[var(--studio-color-surface)]")
    }
}

/// Where a pointer landed along the bar, plus the grab radius as a fraction
/// of that bar's width. `None` outside a real browser event (the story-less
/// host test path) or on a zero-width bar.
fn bar_position(event: &Event<PointerData>) -> Option<(f32, f32)> {
    use dioxus::web::WebEventExt;

    let web_event = event.data().try_as_web_event()?;
    bar_fraction(&web_event, f64::from(web_event.client_x()))
}

fn bar_click_position(event: &Event<MouseData>) -> Option<(f32, f32)> {
    use dioxus::web::WebEventExt;

    let web_event = event.data().try_as_web_event()?;
    bar_fraction(&web_event, f64::from(web_event.client_x()))
}

/// The shared px → fraction read, against the CURRENT target (the bar), so a
/// captured drag keeps measuring the bar it started on.
fn bar_fraction(event: &web_sys::MouseEvent, client_x: f64) -> Option<(f32, f32)> {
    use wasm_bindgen::JsCast;

    let bar = event
        .current_target()?
        .dyn_into::<web_sys::Element>()
        .ok()?;
    let rect = bar.get_bounding_client_rect();
    let width = rect.width();
    if width <= 0.0 {
        return None;
    }
    let at = ((client_x - rect.left()) / width).clamp(0.0, 1.0) as f32;
    Some((at, (GRAB_RADIUS_PX / width) as f32))
}

/// Keep a stop drag alive when the pointer leaves the (short) bar — the same
/// capture the XY pad and the knob take.
fn capture_bar_pointer(event: &Event<PointerData>) {
    use dioxus::web::WebEventExt;
    use wasm_bindgen::JsCast;

    if let Some(web_event) = event.data().try_as_web_event()
        && let Some(target) = web_event
            .current_target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    {
        let _ = target.set_pointer_capture(web_event.pointer_id());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(space: Colorspace) -> Gradient {
        Gradient {
            space,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: from_display_srgb(space, [0.0, 0.0, 0.0]),
                },
                GradientStop {
                    at: 1.0,
                    c: from_display_srgb(space, [1.0, 1.0, 1.0]),
                },
            ],
        }
    }

    #[test]
    fn a_pointer_grabs_the_nearest_stop_inside_the_radius_and_nothing_outside_it() {
        let gradient = ramp(Colorspace::Srgb);
        assert_eq!(stop_index_near(&gradient, 0.02, 0.05), Some(0));
        assert_eq!(stop_index_near(&gradient, 0.97, 0.05), Some(1));
        // The dead zone in the middle is what makes double-click-to-add
        // distinguishable from grabbing a handle.
        assert_eq!(stop_index_near(&gradient, 0.5, 0.05), None);
    }

    #[test]
    fn dragging_a_stop_clamps_to_the_ramp_and_keeps_the_stop_order_authored() {
        let gradient = ramp(Colorspace::Srgb);
        assert_eq!(with_stop_at(&gradient, 0, 1.4).stops[0].at, 1.0);
        assert_eq!(with_stop_at(&gradient, 0, -0.3).stops[0].at, 0.0);
        assert_eq!(with_stop_at(&gradient, 0, f32::NAN).stops[0].at, 0.0);

        // Dragged past its neighbour, the stop keeps its index — the list is
        // sorted by consumers, never by the drag.
        let crossed = with_stop_at(&gradient, 0, 1.0);
        assert_eq!(crossed.stops[0].c, gradient.stops[0].c);
        assert_eq!(crossed.validate(), Ok(()));

        // An index nobody has (a stale selection) changes nothing.
        assert_eq!(with_stop_at(&gradient, 9, 0.5), gradient);
    }

    #[test]
    fn a_new_stop_takes_the_color_the_ramp_already_showed_there() {
        let gradient = ramp(Colorspace::Srgb);
        let (added, index) = with_stop_added(&gradient, 0.5);

        assert_eq!(added.stops.len(), 3);
        assert_eq!(index, 2, "the new stop is the selected one");
        for channel in added.stops[index].c {
            assert!((channel - 0.5).abs() < 1e-5, "{:?}", added.stops[index].c);
        }
        assert_eq!(added.validate(), Ok(()));
    }

    #[test]
    fn a_full_gradient_refuses_another_stop() {
        let mut gradient = ramp(Colorspace::Srgb);
        while gradient.stops.len() < MAX_GRADIENT_STOPS as usize {
            gradient.stops.push(GradientStop {
                at: 0.5,
                c: [0.5, 0.5, 0.5],
            });
        }
        let (unchanged, _) = with_stop_added(&gradient, 0.25);
        assert_eq!(unchanged.stops.len(), MAX_GRADIENT_STOPS as usize);
        assert_eq!(unchanged.validate(), Ok(()));
    }

    #[test]
    fn deleting_stops_stops_at_the_models_two_stop_floor() {
        let (three, _) = with_stop_added(&ramp(Colorspace::Srgb), 0.4);
        let two = with_stop_removed(&three, 2);
        assert_eq!(two.stops.len(), 2);

        // The floor: the same gesture on a two-stop ramp is a no-op, not a
        // one-stop "gradient".
        assert_eq!(with_stop_removed(&two, 0), two);
        assert_eq!(with_stop_removed(&two, 9), two);
        assert_eq!(two.validate(), Ok(()));
    }

    /// Changing the interpolation space must not change the COLORS — it is a
    /// statement about the blend between them.
    #[test]
    fn switching_space_converts_the_stops_instead_of_reinterpreting_them() {
        let srgb = ramp(Colorspace::Srgb);
        let recolored = with_stop_color(&srgb, 1, [0.2, 0.55, 0.9]);

        let oklab = with_space(&recolored, Colorspace::Oklab);
        assert_eq!(oklab.space, Colorspace::Oklab);
        assert_eq!(oklab.method, recolored.method);
        for index in 0..oklab.stops.len() {
            let before = stop_display_srgb(&recolored, index);
            let after = stop_display_srgb(&oklab, index);
            for channel in 0..3 {
                assert!(
                    (before[channel] - after[channel]).abs() < 2e-3,
                    "stop {index}: {before:?} vs {after:?}"
                );
            }
        }
        // Same space in, same gradient out.
        assert_eq!(with_space(&oklab, Colorspace::Oklab), oklab);
    }

    #[test]
    fn the_color_well_speaks_display_srgb_in_both_directions() {
        let oklab = ramp(Colorspace::Oklab);
        let recolored = with_stop_color(&oklab, 0, [0.93, 0.41, 0.06]);

        // Stored in the gradient's own space...
        assert_ne!(recolored.stops[0].c, [0.93, 0.41, 0.06]);
        // ...and read back as the color the user picked.
        assert_eq!(hex_of(stop_display_srgb(&recolored, 0)), "#ed6910");

        assert_eq!(srgb_from_hex("#000000"), Some([0.0, 0.0, 0.0]));
        assert_eq!(srgb_from_hex("ffffff"), Some([1.0, 1.0, 1.0]));
        assert_eq!(srgb_from_hex("#fff"), None);
        assert_eq!(srgb_from_hex("#gggggg"), None);
    }

    #[test]
    fn the_provenance_line_names_the_built_in_a_copy_forked_from() {
        assert_eq!(
            PaletteOrigin::BuiltinCopy("Ocean".to_string()).provenance(),
            "copy of built-in \u{201c}Ocean\u{201d}"
        );
        assert_eq!(PaletteOrigin::ProjectCustom.provenance(), "project custom");
    }

    #[test]
    fn a_handle_carries_only_its_position_and_its_color_inline() {
        let style = stop_handle_style(0.25, [1.0, 0.0, 0.0]);
        assert_eq!(style, "left: 25.00%; background: #ff0000;");
        assert!(stop_handle_class(true).contains("text-strong"));
    }
}
