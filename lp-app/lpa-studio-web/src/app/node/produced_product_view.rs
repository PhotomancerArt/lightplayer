//! Leaf presentation for a produced product.

use dioxus::prelude::*;
use lpa_studio_core::{
    ColorOrder, ControlDisplayLayout, ControlSampleEncoding, UiAction, UiControlProductPreview,
    UiProducedProduct, UiProductKind, UiProductPreview, UiProductPreviewFrame,
    UiProductTrackingState,
};

use crate::app::node::SlotDetailButton;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedProductView(
    product: UiProducedProduct,
    #[props(default = false)] separated: bool,
    #[props(default = false)] initially_open: bool,
    #[props(default)] focus_action: Option<UiAction>,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let class = if separated {
        format!(
            "{} tw:border-t tw:border-border-muted",
            product_view_class(product.kind)
        )
    } else {
        product_view_class(product.kind).to_string()
    };
    let label = product_label(product.kind);
    let aspects = product.visible_aspects();

    rsx! {
        article { class,
            ProductPreview {
                kind: product.kind,
                preview: product.preview.clone(),
                tracking: product.tracking,
                frame: product.frame,
                focus_action,
                on_action,
            }
            footer { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-2 tw:gap-y-1 tw:text-xs tw:text-muted-foreground",
                strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{product.name}" }
                span { "{label}" }
                if let Some(detail) = preview_detail(&product.preview, product.tracking) {
                    span { "{detail}" }
                }
                if let Some(detail) = tracking_detail(product.tracking) {
                    span { "{detail}" }
                }
                if let Some(detail) = product.detail.as_ref() {
                    span { "{detail}" }
                }
                SlotDetailButton {
                    label: product.name.clone(),
                    aspects,
                    initially_open,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductPreview(
    kind: UiProductKind,
    preview: UiProductPreview,
    tracking: UiProductTrackingState,
    frame: UiProductPreviewFrame,
    focus_action: Option<UiAction>,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let frame_class = product_frame_class(kind);
    let frame_style = preview_frame_style(&preview, frame);
    let overlay = product_tracking_overlay(kind, tracking);

    rsx! {
        div { class: "{frame_class}", style: "{frame_style}",
            match preview {
                UiProductPreview::VisualSrgb8 {
                    width,
                    height,
                    bytes,
                    ..
                } => rsx! {
                    ProductPixelGrid { width, height, bytes }
                },
                UiProductPreview::ControlNative(preview) => rsx! {
                    ControlProductPreview { preview }
                },
                UiProductPreview::Pending => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: if tracking == UiProductTrackingState::Tracking {
                            ProductSkeletonTone::Working
                        } else {
                            ProductSkeletonTone::Quiet
                        },
                        title: "Tracking product",
                        detail: "Waiting for the first preview.",
                        show_text: tracking == UiProductTrackingState::Tracking,
                    }
                },
                UiProductPreview::Error { message } => rsx! {
                    ProductMessage {
                        tone: ProductMessageTone::Error,
                        message,
                    }
                },
                UiProductPreview::Unsupported { reason } => rsx! {
                    ProductMessage {
                        tone: ProductMessageTone::Warning,
                        message: reason,
                    }
                },
                UiProductPreview::Empty => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: ProductSkeletonTone::Quiet,
                        title: "No product",
                        detail: "This output has not resolved to a product.",
                        show_text: true,
                    }
                },
                UiProductPreview::MetadataOnly => rsx! {
                    ProductSkeleton {
                        kind,
                        tone: ProductSkeletonTone::Quiet,
                        title: "Metadata only",
                        detail: "Studio does not render this product type yet.",
                        show_text: true,
                    }
                },
            }
            if let Some(overlay) = overlay {
                ProductTrackingOverlay {
                    title: overlay.title,
                    detail: overlay.detail,
                    focus_action,
                    on_action,
                }
            }
        }
    }
}

fn product_frame_class(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Visual | UiProductKind::Control => {
            "ux-produced-product-frame ux-produced-product-frame-capped"
        }
        _ => "ux-produced-product-frame",
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ControlProductPreview(preview: UiControlProductPreview) -> Element {
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_ref() else {
        return rsx! {
            ProductMessage {
                tone: ProductMessageTone::Warning,
                message: "Control product has no display layout.".to_string(),
            }
        };
    };

    if !control_sample_layout_has_rgb(&preview) {
        return rsx! {
            ProductMessage {
                tone: ProductMessageTone::Warning,
                message: "Control product sample layout is not RGB.".to_string(),
            }
        };
    }

    let lamps = control_lamp_styles(&preview);
    rsx! {
        div { class: "ux-produced-product-control-layout",
            for (index, style) in lamps.into_iter().enumerate() {
                span {
                    key: "{index}",
                    class: "ux-produced-product-control-lamp",
                    style: "{style}",
                }
            }
            if layout.lamps.is_empty() {
                ProductMessage {
                    tone: ProductMessageTone::Warning,
                    message: "Control product display layout is empty.".to_string(),
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductPixelGrid(width: u32, height: u32, bytes: Vec<u8>) -> Element {
    let columns = width.max(1);
    let rows = height.max(1);
    let grid_style = format!(
        "grid-template-columns: repeat({columns}, minmax(0, 1fr)); grid-template-rows: repeat({rows}, minmax(0, 1fr));"
    );
    let pixels = rgb_pixel_styles(&bytes);
    rsx! {
        div {
    class: "ux-produced-product-pixel-grid",
            style: "{grid_style}",
            for (index, style) in pixels.into_iter().enumerate() {
                span {
                    key: "{index}",
            class: "tw:block",
                    style: "{style}",
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductSkeleton(
    kind: UiProductKind,
    tone: ProductSkeletonTone,
    title: &'static str,
    detail: &'static str,
    #[props(default = true)] show_text: bool,
) -> Element {
    let class = product_skeleton_class(kind, tone);

    rsx! {
        div { class,
            div { class: "ux-produced-product-skeleton-graphic", aria_hidden: "true",
                for index in 0..12 {
                    span { key: "{index}", class: "ux-produced-product-skeleton-bar" }
                }
            }
            if show_text {
                div { class: "tw:grid tw:min-w-0 tw:gap-1 tw:text-center",
                    strong { class: "tw:text-sm tw:text-strong-foreground", "{title}" }
                    span { class: "tw:text-xs tw:leading-snug tw:text-muted-foreground", "{detail}" }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductMessage(tone: ProductMessageTone, message: String) -> Element {
    let class = match tone {
        ProductMessageTone::Warning => {
            "ux-produced-product-message ux-produced-product-message-warning"
        }
        ProductMessageTone::Error => {
            "ux-produced-product-message ux-produced-product-message-error"
        }
    };

    rsx! {
        div { class, "{message}" }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProductTrackingOverlay(
    title: &'static str,
    detail: &'static str,
    focus_action: Option<UiAction>,
    on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if let (Some(action), Some(handler)) = (focus_action, on_action) {
        return rsx! {
            button {
                class: "ux-produced-product-overlay ux-produced-product-overlay-button",
                r#type: "button",
                onclick: move |event| {
                    event.stop_propagation();
                    handler.call(action.clone());
                },
                strong { "{title}" }
                span { "{detail}" }
            }
        };
    }

    rsx! {
        div { class: "ux-produced-product-overlay",
            strong { "{title}" }
            span { "{detail}" }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductSkeletonTone {
    Quiet,
    Working,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProductMessageTone {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProductOverlayCopy {
    title: &'static str,
    detail: &'static str,
}

fn product_view_class(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Empty => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:bg-card-muted tw:p-3"
        }
        UiProductKind::Visual => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:bg-[color-mix(in_oklab,var(--color-accent-bg)_60%,var(--color-card))] tw:p-3"
        }
        UiProductKind::Control => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:bg-[color-mix(in_oklab,var(--color-status-good-bg)_55%,var(--color-card))] tw:p-3"
        }
        UiProductKind::Other => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:bg-card-muted tw:p-3"
        }
    }
}

fn product_label(kind: UiProductKind) -> &'static str {
    match kind {
        UiProductKind::Empty => "empty product",
        UiProductKind::Visual => "visual product",
        UiProductKind::Control => "control product",
        UiProductKind::Other => "product",
    }
}

fn preview_detail(preview: &UiProductPreview, tracking: UiProductTrackingState) -> Option<String> {
    match preview {
        UiProductPreview::VisualSrgb8 { width, height, .. } => Some(format!("{width} x {height}")),
        UiProductPreview::ControlNative(preview) => Some(format!(
            "{} x {} samples",
            preview.extent.rows, preview.extent.samples_per_row
        )),
        UiProductPreview::Pending if tracking == UiProductTrackingState::Tracking => {
            Some("preview pending".to_string())
        }
        UiProductPreview::Pending => None,
        UiProductPreview::MetadataOnly => Some("metadata only".to_string()),
        UiProductPreview::Empty
        | UiProductPreview::Unsupported { .. }
        | UiProductPreview::Error { .. } => None,
    }
}

fn tracking_detail(tracking: UiProductTrackingState) -> Option<&'static str> {
    match tracking {
        UiProductTrackingState::Untracked => Some("not tracked"),
        UiProductTrackingState::Paused => Some("paused"),
        UiProductTrackingState::Tracking => None,
    }
}

fn product_tracking_overlay(
    kind: UiProductKind,
    tracking: UiProductTrackingState,
) -> Option<ProductOverlayCopy> {
    let label = match kind {
        UiProductKind::Visual => "Visual output",
        UiProductKind::Control => "Control output",
        _ => return None,
    };
    match tracking {
        UiProductTrackingState::Untracked => Some(ProductOverlayCopy {
            title: if kind == UiProductKind::Visual {
                "Visual output not tracked"
            } else {
                "Control output not tracked"
            },
            detail: "Click to view",
        }),
        UiProductTrackingState::Paused => Some(ProductOverlayCopy {
            title: if label == "Visual output" {
                "Visual output paused"
            } else {
                "Control output paused"
            },
            detail: "Click to view",
        }),
        UiProductTrackingState::Tracking => None,
    }
}

fn preview_frame_style(preview: &UiProductPreview, frame: UiProductPreviewFrame) -> String {
    if let UiProductPreview::ControlNative(control) = preview
        && let Some(ControlDisplayLayout::Layout2d(layout)) = control.display_layout.as_ref()
    {
        return format!(
            "aspect-ratio: {} / {};",
            layout.width_hint.max(1),
            layout.height_hint.max(1)
        );
    }
    format!("aspect-ratio: {} / {};", frame.width, frame.height)
}

fn rgb_pixel_styles(bytes: &[u8]) -> Vec<String> {
    bytes
        .chunks_exact(3)
        .map(|chunk| {
            format!(
                "background-color: rgb({} {} {});",
                chunk[0], chunk[1], chunk[2]
            )
        })
        .collect()
}

fn control_sample_layout_has_rgb(preview: &UiControlProductPreview) -> bool {
    preview.sample_layout.spans.iter().any(|span| {
        matches!(
            span.encoding,
            ControlSampleEncoding::RgbPixels { count, .. } if count > 0
        )
    })
}

fn control_lamp_styles(preview: &UiControlProductPreview) -> Vec<String> {
    let Some(ControlDisplayLayout::Layout2d(layout)) = preview.display_layout.as_ref() else {
        return Vec::new();
    };
    layout
        .lamps
        .iter()
        .map(|lamp| {
            let [r, g, b] = control_rgb_at_sample(preview, lamp.sample_start).unwrap_or([0, 0, 0]);
            let diameter = (lamp.radius.max(0.006) * 96.0).clamp(3.5, 18.0);
            format!(
                "--lamp-r: {r}; --lamp-g: {g}; --lamp-b: {b}; left: {:.3}%; top: {:.3}%; width: max(5px, {:.3}%); height: max(5px, {:.3}%);",
                lamp.center[0].clamp(0.0, 1.0) * 100.0,
                lamp.center[1].clamp(0.0, 1.0) * 100.0,
                diameter,
                diameter,
            )
        })
        .collect()
}

fn control_rgb_at_sample(preview: &UiControlProductPreview, sample_start: u32) -> Option<[u8; 3]> {
    let span = preview.sample_layout.spans.iter().find(|span| {
        matches!(span.encoding, ControlSampleEncoding::RgbPixels { .. })
            && sample_start >= span.start
            && sample_start.saturating_add(3) <= span.start.saturating_add(span.len)
            && (sample_start - span.start).is_multiple_of(3)
    })?;
    let color_order = match span.encoding {
        ControlSampleEncoding::RgbPixels { color_order, .. } => color_order,
        ControlSampleEncoding::Raw => return None,
    };
    let sample = |offset: u32| -> Option<u8> {
        let index = sample_start.checked_add(offset)? as usize;
        let byte_index = index.checked_mul(2)?;
        let lo = *preview.bytes.get(byte_index)?;
        let hi = *preview.bytes.get(byte_index + 1)?;
        Some((u16::from_le_bytes([lo, hi]) >> 8) as u8)
    };
    let a = sample(0)?;
    let b = sample(1)?;
    let c = sample(2)?;
    Some(match color_order {
        ColorOrder::Rgb => [a, b, c],
        ColorOrder::Grb => [b, a, c],
        ColorOrder::Rbg => [a, c, b],
        ColorOrder::Gbr => [c, a, b],
        ColorOrder::Brg => [b, c, a],
        ColorOrder::Bgr => [c, b, a],
    })
}

fn product_skeleton_class(kind: UiProductKind, tone: ProductSkeletonTone) -> &'static str {
    match (kind, tone) {
        (UiProductKind::Visual, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-visual ux-produced-product-skeleton-working"
        }
        (UiProductKind::Visual, ProductSkeletonTone::Quiet) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-visual"
        }
        (UiProductKind::Control, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-control ux-produced-product-skeleton-working"
        }
        (UiProductKind::Control, ProductSkeletonTone::Quiet) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-control"
        }
        (_, ProductSkeletonTone::Working) => {
            "ux-produced-product-skeleton ux-produced-product-skeleton-working"
        }
        (_, ProductSkeletonTone::Quiet) => "ux-produced-product-skeleton",
    }
}
