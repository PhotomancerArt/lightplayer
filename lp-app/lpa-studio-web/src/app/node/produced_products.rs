use dioxus::prelude::*;
use lpa_studio_core::{UiProducedProduct, UiProductKind};

use crate::app::node::{DirtyMark, ProducedBindingMark};

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProducedProducts(products: Vec<UiProducedProduct>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            SectionTitle { title: "Produced products" }
            div { class: "tw:grid tw:grid-cols-[repeat(auto-fit,minmax(180px,1fr))] tw:gap-2",
                for product in products {
                    ProducedProductTile { product }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ProducedProductTile(product: UiProducedProduct) -> Element {
    let class = match product.kind {
        UiProductKind::Visual => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:rounded-sm tw:border tw:border-accent-border tw:bg-[color-mix(in_oklab,var(--color-accent-bg)_60%,var(--color-card))] tw:p-3"
        }
        UiProductKind::Control => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:rounded-sm tw:border tw:border-status-good-border tw:bg-[color-mix(in_oklab,var(--color-status-good-bg)_55%,var(--color-card))] tw:p-3"
        }
        UiProductKind::Other => {
            "tw:grid tw:min-h-32 tw:min-w-0 tw:content-between tw:gap-3 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-3"
        }
    };
    let label = match product.kind {
        UiProductKind::Visual => "visual product",
        UiProductKind::Control => "control product",
        UiProductKind::Other => "product",
    };

    rsx! {
        article { class,
            div { class: "tw:grid tw:min-h-16 tw:grid-cols-6 tw:gap-1 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:p-1",
                for index in 0..24 {
                    span { key: "{index}", class: preview_cell_class(product.kind, index) }
                }
            }
            footer { class: "tw:grid tw:min-w-0 tw:gap-1",
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-2",
                    ProducedBindingMark {
                        label: product.name.clone(),
                        bindings: product.binding.bindings.clone(),
                    }
                    strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{product.name}" }
                    DirtyMark { dirty: product.dirty }
                }
                div { class: "tw:flex tw:flex-wrap tw:gap-2 tw:text-xs tw:text-muted-foreground",
                    span { "{label}" }
                    if let Some(detail) = product.detail.as_ref() {
                        span { "{detail}" }
                    }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SectionTitle(title: &'static str) -> Element {
    rsx! {
        h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-none tw:text-heading", "{title}" }
    }
}

fn preview_cell_class(kind: UiProductKind, index: usize) -> &'static str {
    match kind {
        UiProductKind::Visual if index % 5 == 0 => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-accent"
        }
        UiProductKind::Visual => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-working-bg"
        }
        UiProductKind::Control if index % 4 == 0 => {
            "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-good-foreground"
        }
        UiProductKind::Control => "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-status-good-bg",
        UiProductKind::Other => "tw:block tw:aspect-square tw:rounded-[1px] tw:bg-card-muted",
    }
}
