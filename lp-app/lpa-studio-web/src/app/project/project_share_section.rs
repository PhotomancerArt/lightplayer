//! The project popup's "Share" rows: export a zip, copy the project as
//! JSON.
//!
//! There is no cloud provider, so the clipboard and the filesystem are the
//! whole distribution story (`docs/adr/2026-07-28-share-envelopes.md`). Zip
//! export already existed on gallery cards; this brings both forms to the
//! project you currently have open, which is where you actually notice you
//! want to send it to someone.
//!
//! # Why these disable while dirty
//!
//! Both forms read the **library snapshot** — the bytes on disk — while
//! unsaved edits live in the overlay. Exporting a dirty project would
//! silently hand over the last-saved version, which is the worst available
//! outcome: it looks like it worked. Rather than quietly export stale
//! bytes, the rows disable and say to save first.
//!
//! (The nicer behaviour is a Save-then-export that chains on save
//! completion. That needs a completion seam the web edge does not have
//! today — see the plan's P5 notes.)
//!
//! A project with no library identity — the storeless demo path, or a
//! device-hosted project this library does not know — renders no share
//! section at all rather than rows that cannot work.

use dioxus::prelude::*;

use crate::app::home::package_export::{ExportForm, ExportTarget, export_package_as};
use crate::app::share::share_url::project_link_absolute;
use crate::base::{StudioIcon, StudioIconName, Toasts};
use crate::core::inline_link_row_class;

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ProjectShareSection(
    /// The open project's library identity (`prj…` uid, slug).
    uid: String,
    slug: String,
    /// Unsaved persisted edits: while any exist, both forms would export
    /// the last-saved bytes, so they are disabled.
    #[props(default = 0)]
    unsaved: usize,
) -> Element {
    let dirty = unsaved > 0;
    let zip_target = ExportTarget {
        uid: uid.clone(),
        slug: slug.clone(),
    };
    let toasts = try_consume_context::<Toasts>();
    let (link_slug, link_uid) = (slug.clone(), uid.clone());
    let json_target = ExportTarget { uid, slug };

    rsx! {
        div { class: "tw:grid tw:min-w-0 tw:gap-1.5",
            // The link first — the address bar IS the link (D1), and this
            // row is where people look for it (G1 finding, 2026-08-29).
            // Never dirty-disabled: the link is identity, not bytes.
            ShareRow {
                label: "Copy link",
                hint: "Copy this project's link — the same address the address bar shows.",
                icon: StudioIconName::ExternalLink,
                disabled: false,
                on_press: move |_| {
                    crate::clipboard::write_text(&project_link_absolute(&link_slug, &link_uid));
                    if let Some(mut toasts) = toasts {
                        toasts.say("Link copied");
                    }
                },
            }
            ShareRow {
                label: "Download zip",
                hint: "Download this project as a zip archive.",
                icon: StudioIconName::Download,
                disabled: dirty,
                on_press: move |_| export_package_as(zip_target.clone(), ExportForm::Zip),
            }
            ShareRow {
                label: "Copy JSON",
                hint: "Copy this project as a shareable JSON envelope.",
                icon: StudioIconName::Copy,
                disabled: dirty,
                on_press: move |_| {
                    export_package_as(json_target.clone(), ExportForm::JsonToClipboard)
                },
            }
            if dirty {
                p { class: "tw:m-0 tw:pt-1 tw:text-[0.68rem] tw:leading-snug tw:text-subtle-foreground",
                    "Save first — sharing sends the last saved version, not your unsaved edits."
                }
            }
        }
    }
}

/// One share affordance: icon, label, and a disabled state that explains
/// itself through the row's title.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ShareRow(
    label: &'static str,
    hint: &'static str,
    icon: StudioIconName,
    disabled: bool,
    on_press: EventHandler<()>,
) -> Element {
    let title = if disabled {
        "Save this project to share it."
    } else {
        hint
    };
    let class = inline_link_row_class(disabled);

    rsx! {
        button {
            class,
            r#type: "button",
            disabled,
            title,
            onclick: move |event| {
                event.stop_propagation();
                if !disabled {
                    on_press.call(());
                }
            },
            span { class: "tw:inline-flex tw:h-[15px] tw:w-[15px] tw:flex-none tw:items-center tw:justify-center", aria_hidden: "true",
                StudioIcon { name: icon, size: 14 }
            }
            span { class: "tw:min-w-0 tw:truncate", "{label}" }
        }
    }
}
