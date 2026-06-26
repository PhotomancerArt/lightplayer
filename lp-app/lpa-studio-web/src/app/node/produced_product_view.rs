//! Leaf presentation for a produced product.

use dioxus::prelude::*;
use lpa_studio_core::{
    UiAction, UiProducedProduct, UiProductKind, UiProductPreview, UiProductPreviewFrame,
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
    let frame_style = format!("aspect-ratio: {} / {};", frame.width, frame.height);
    let overlay = visual_tracking_overlay(kind, tracking);

    rsx! {
        div { class: "ux-produced-product-frame", style: "{frame_style}",
            match preview {
                UiProductPreview::VisualSrgb8 {
                    width,
                    height,
                    bytes,
                    ..
                } => rsx! {
                    ProductPixelGrid { width, height, bytes }
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
        UiProductPreview::VisualSrgb8 {
            width,
            height,
            revision,
            ..
        } => Some(format!("{width} x {height} rev {revision}")),
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

fn visual_tracking_overlay(
    kind: UiProductKind,
    tracking: UiProductTrackingState,
) -> Option<ProductOverlayCopy> {
    if kind != UiProductKind::Visual {
        return None;
    }
    match tracking {
        UiProductTrackingState::Untracked => Some(ProductOverlayCopy {
            title: "Visual output not tracked",
            detail: "Click to view",
        }),
        UiProductTrackingState::Paused => Some(ProductOverlayCopy {
            title: "Visual output paused",
            detail: "Click to view",
        }),
        UiProductTrackingState::Tracking => None,
    }
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
