use dioxus::prelude::*;
use lpa_studio_core::{UiAssetEditorKind, UiConfigAsset};

use crate::app::node::DirtyMark;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConfigAssets(assets: Vec<UiConfigAsset>) -> Element {
    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-0",
            for (index, asset) in assets.into_iter().enumerate() {
                AssetPanel {
                    key: "{asset.label}",
                    asset,
                    separated: index > 0,
                }
            }
        }
    }
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AssetPanel(asset: UiConfigAsset, separated: bool) -> Element {
    let class = if separated {
        "tw:grid tw:min-w-0 tw:gap-2 tw:border-t tw:border-border-muted tw:pt-3"
    } else {
        "tw:grid tw:min-w-0 tw:gap-2"
    };

    rsx! {
        article { class,
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
                pre { class: "tw:m-0 tw:max-h-32 tw:overflow-auto tw:bg-page tw:p-3 tw:text-xs tw:leading-normal tw:text-muted-foreground",
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
