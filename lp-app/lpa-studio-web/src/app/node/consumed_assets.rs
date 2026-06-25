use dioxus::prelude::*;
use lpa_studio_core::{UiAssetEditorKind, UiConsumedAsset};

use crate::app::node::DirtyMark;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConsumedAssets(assets: Vec<UiConsumedAsset>) -> Element {
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            h4 { class: "tw:m-0 tw:text-xs tw:font-bold tw:uppercase tw:leading-none tw:text-heading", "Consumed assets" }
            div { class: "tw:grid tw:min-w-0 tw:gap-2",
                for asset in assets {
                    AssetPanel { asset }
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AssetPanel(asset: UiConsumedAsset) -> Element {
    rsx! {
        article { class: "tw:grid tw:min-w-0 tw:gap-2 tw:rounded-sm tw:border tw:border-border-subtle tw:bg-card-muted tw:p-3",
            header { class: "tw:flex tw:min-w-0 tw:flex-wrap tw:items-start tw:justify-between tw:gap-2",
                div { class: "tw:min-w-0",
                    div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5",
                        strong { class: "tw:min-w-0 tw:text-sm tw:text-strong-foreground tw:break-words", "{asset.label}" }
                        DirtyMark { dirty: asset.dirty }
                    }
                    code { class: "tw:block tw:font-mono tw:text-xs tw:text-muted-foreground tw:break-words", "{asset.source}" }
                }
                span { class: "tw:rounded-xs tw:border tw:border-border-subtle tw:bg-page tw:px-2 tw:py-1 tw:text-xs tw:font-bold tw:text-subtle-foreground", "{editor_label(asset.editor)}" }
            }
            if let Some(detail) = asset.detail.as_ref() {
                p { class: "tw:m-0 tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
            }
            if let Some(summary) = asset.summary.as_ref() {
                pre { class: "tw:m-0 tw:max-h-28 tw:overflow-auto tw:rounded-xs tw:border tw:border-border-muted tw:bg-page tw:p-2 tw:text-xs tw:leading-normal tw:text-muted-foreground",
                    code { "{summary}" }
                }
            }
        }
    }
}

fn editor_label(editor: UiAssetEditorKind) -> &'static str {
    match editor {
        UiAssetEditorKind::Text => "text",
        UiAssetEditorKind::Glsl => "glsl",
        UiAssetEditorKind::Svg => "svg",
        UiAssetEditorKind::Binary => "binary",
    }
}
