//! Leaf presentation for a produced product.

use dioxus::prelude::*;
use lpa_studio_core::{UiProducedProduct, UiProductKind};

use crate::app::node::{DirtyMark, ProducedBindingMark};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedProductView(
    product: UiProducedProduct,
    #[props(default = false)] separated: bool,
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

    rsx! {
        article { class,
            div { class: "tw:grid tw:min-h-20 tw:grid-cols-8 tw:gap-1",
                for index in 0..24 {
                    span { key: "{index}", class: preview_cell_class(product.kind, index) }
                }
            }
            footer { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-center tw:gap-x-2 tw:gap-y-1 tw:text-xs tw:text-muted-foreground",
                ProducedBindingMark {
                    label: product.name.clone(),
                    bindings: product.binding.bindings.clone(),
                }
                strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{product.name}" }
                span { "{label}" }
                if let Some(detail) = product.detail.as_ref() {
                    span { "{detail}" }
                }
                DirtyMark { dirty: product.dirty }
            }
        }
    }
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

fn preview_cell_class(kind: UiProductKind, index: usize) -> &'static str {
    match kind {
        UiProductKind::Empty if index % 5 == 0 => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-card-subtle"
        }
        UiProductKind::Empty => "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-card-muted",
        UiProductKind::Visual if index % 5 == 0 => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-accent"
        }
        UiProductKind::Visual => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-working-border"
        }
        UiProductKind::Control if index % 4 == 0 => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-good-foreground"
        }
        UiProductKind::Control => "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-good-bg",
        UiProductKind::Other => "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-card-muted",
    }
}
