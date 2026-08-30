//! The Projects page's archive drawer (spike `project-share` §5, Q12).
//!
//! Archiving is the removal verb (D8): nothing is deleted, the link keeps
//! resolving for the project's members and stops resolving for everybody
//! else, and **Restore** is the loud action. So the archive is not a page —
//! it is a collapsed section at the bottom of the library, closed by
//! default, holding rows that look like what they are: dimmed, quiet, and
//! one click from coming back.
//!
//! There is deliberately **no Delete forever** this slice (Q9, deferred):
//! the only irreversible act in the product would want its own confirmation
//! grammar, and the archive works without it.
//!
//! # Shape
//!
//! [`ArchivedProjectsSection`] is the live half — one `ListMyProjects` per
//! signed-in account, split on [`ProjectMeta::archived`]. Everything visible
//! is [`ArchivedProjectsList`], which takes pure props so the story renders
//! it with no service and no session.
//!
//! # What a row can say (P2 friction)
//!
//! `ListMyProjects` answers with [`ProjectMeta`] — uid, slug, access,
//! owner, archived — and no sidecar, so a row has **no display name, no
//! preview and no archived-on date**. The spike's row carries all three.
//! Rows therefore lead with the slug (the readable half of the link, and
//! what the address bar shows) over the canonical path, which is honest and
//! recognizable; the date and the preview strip want `ProjectList` to grow
//! a sidecar, not a client-side guess.

use dioxus::prelude::*;
use dioxus_icons::lucide::{Archive, ChevronDown, ChevronRight};
use lpc_cloud_api::request::{ListMyProjects, RestoreProject};
use lpc_cloud_api::{ProjectMeta, share_link};
use lpc_history::PrefixedUid;

use crate::base::{InlineButtonTone, Toasts, inline_text_button_class};
use crate::cloud::{CloudSession, FetchCloudPort};

/// One archived project, as a row needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchivedProject {
    /// The whole of the identity, and what `RestoreProject` addresses.
    pub uid: PrefixedUid,
    /// The readable half of the link — the row's headline, for want of a
    /// display name (see the module docs).
    pub slug: String,
}

impl ArchivedProject {
    /// The canonical share path, rendered under the name in mono.
    pub fn path(&self) -> String {
        share_link::canonical_path(&self.slug, self.uid)
    }

    /// The row's headline: the slug, or the bare uid for a project whose
    /// name slugified to nothing.
    pub fn headline(&self) -> String {
        if self.slug.is_empty() {
            self.uid.to_string()
        } else {
            self.slug.clone()
        }
    }
}

/// The live section: the account's archived projects, or nothing at all.
///
/// Renders nothing when signed out, when the service does not answer, and
/// when the archive is empty — an empty drawer is not information, and the
/// library page's own emptiness is a different statement.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ArchivedProjectsSection() -> Element {
    let Some(session) = try_consume_context::<Signal<CloudSession>>() else {
        return rsx! {};
    };
    let toasts = try_consume_context::<Toasts>();
    let account = use_memo(move || session().me().map(|me| me.uid.to_string()));
    let mut reload = use_signal(|| 0u32);
    let mut archived = use_signal(Vec::<ArchivedProject>::new);
    use_effect(move || {
        let _ = reload.read();
        if account().is_none() {
            archived.set(Vec::new());
            return;
        }
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), ListMyProjects).await {
                Ok(list) => archived.set(archived_of(&list.projects)),
                Err(error) => {
                    // Quiet: a library page is still a library page without
                    // its archive drawer.
                    log::debug!("archive: could not list projects: {error}");
                    archived.set(Vec::new());
                }
            }
        });
    });

    let projects = archived();
    if projects.is_empty() {
        return rsx! {};
    }
    let on_restore = EventHandler::new(move |uid: PrefixedUid| {
        spawn(async move {
            match lpa_cloud_client::call(&FetchCloudPort::new(), RestoreProject { uid }).await {
                Ok(_) => {
                    reload += 1;
                    if let Some(mut toasts) = toasts {
                        toasts.say("Restored — the link resolves again.");
                    }
                }
                Err(error) => {
                    log::warn!("archive: could not restore {uid}: {error}");
                    if let Some(mut toasts) = toasts {
                        toasts.warn("Could not restore this project — it is still archived.");
                    }
                }
            }
        });
    });
    rsx! {
        ArchivedProjectsList { projects, on_restore }
    }
}

/// The drawer itself. Pure — the story mounts it with fixtures.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ArchivedProjectsList(
    projects: Vec<ArchivedProject>,
    #[props(default)] on_restore: Option<EventHandler<PrefixedUid>>,
    /// Stories only: mount the drawer open (capture cannot click).
    #[props(default = false)]
    initially_open: bool,
) -> Element {
    let mut open = use_signal(|| initially_open);
    let count = projects.len();
    rsx! {
        section { class: "tw:grid tw:min-w-0 tw:gap-2",
            button {
                class: DRAWER_HEADER_CLASS,
                r#type: "button",
                aria_expanded: "{open()}",
                onclick: move |_| open.toggle(),
                span { class: "tw:flex tw:flex-none tw:text-dim-foreground",
                    if open() {
                        ChevronDown { size: 14 }
                    } else {
                        ChevronRight { size: 14 }
                    }
                }
                span { class: "tw:flex tw:flex-none tw:text-dim-foreground",
                    Archive { size: 14 }
                }
                span { class: "tw:text-xs tw:font-extrabold tw:uppercase", "Archived" }
                span { class: "tw:text-xs tw:font-semibold tw:text-dim-foreground", "{count}" }
            }
            if open() {
                div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:rounded-md tw:border tw:border-border tw:bg-card-subtle",
                    for project in projects.iter() {
                        ArchivedRow {
                            key: "{project.uid}",
                            project: project.clone(),
                            on_restore,
                        }
                    }
                }
            }
        }
    }
}

/// One archived row: dimmed name over its link, and the one verb back.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn ArchivedRow(project: ArchivedProject, on_restore: Option<EventHandler<PrefixedUid>>) -> Element {
    let uid = project.uid;
    rsx! {
        div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-3 tw:border-b tw:border-border-muted tw:px-3.5 tw:py-2.5 tw:last:border-b-0",
            span { class: "tw:grid tw:min-w-0 tw:flex-1 tw:gap-px",
                span { class: "tw:min-w-0 tw:truncate tw:text-xs tw:font-semibold tw:text-subtle-foreground",
                    "{project.headline()}"
                }
                span { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-[10px] tw:text-dim-foreground",
                    "{project.path()}"
                }
            }
            button {
                class: inline_text_button_class(InlineButtonTone::Accent, false),
                r#type: "button",
                title: "Bring this project back to the library",
                onclick: move |_| {
                    if let Some(on_restore) = on_restore {
                        on_restore.call(uid);
                    }
                },
                "Restore"
            }
        }
    }
}

/// The archived half of a project list, in a stable order (by the readable
/// half, then by uid — two projects may share a slug, it is cosmetic).
pub fn archived_of(projects: &[ProjectMeta]) -> Vec<ArchivedProject> {
    let mut archived: Vec<ArchivedProject> = projects
        .iter()
        .filter(|meta| meta.archived)
        .map(|meta| ArchivedProject {
            uid: meta.uid,
            slug: meta.slug.clone(),
        })
        .collect();
    archived.sort_by(|a, b| a.slug.cmp(&b.slug).then_with(|| a.uid.cmp(&b.uid)));
    archived
}

/// The drawer's own header — a disclosure, not a card.
const DRAWER_HEADER_CLASS: &str = "tw:flex tw:w-full tw:min-w-0 tw:cursor-pointer tw:items-center tw:gap-2 tw:rounded-sm tw:border tw:border-transparent tw:bg-transparent tw:px-0.5 tw:py-1 tw:text-left tw:text-subtle-foreground tw:transition-colors tw:hover:text-strong-foreground";

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_cloud_api::{Access, Actor};
    use lpc_history::UidPrefix;

    #[test]
    fn only_archived_projects_reach_the_drawer_and_the_order_is_stable() {
        let projects = vec![
            meta("zook-dome", 1, false),
            meta("ember-field", 2, true),
            meta("a-first-try", 3, true),
        ];
        let archived = archived_of(&projects);
        let slugs: Vec<&str> = archived.iter().map(|p| p.slug.as_str()).collect();
        assert_eq!(slugs, ["a-first-try", "ember-field"]);
    }

    #[test]
    fn a_row_shows_the_canonical_link() {
        let project = ArchivedProject {
            uid: PrefixedUid::mint(UidPrefix::Project, &[6u8; 16]),
            slug: "ember-field".to_string(),
        };
        assert_eq!(project.headline(), "ember-field");
        assert_eq!(project.path(), format!("/p/ember-field-{}", project.uid));
    }

    /// A name that slugified to nothing still names itself.
    #[test]
    fn a_slugless_row_leads_with_its_uid() {
        let project = ArchivedProject {
            uid: PrefixedUid::mint(UidPrefix::Project, &[8u8; 16]),
            slug: String::new(),
        };
        assert_eq!(project.headline(), project.uid.to_string());
        assert_eq!(project.path(), format!("/p/{}", project.uid));
    }

    /// No preflight: the drawer's disclosure header names its own background.
    #[test]
    fn the_drawer_header_names_a_background() {
        assert!(DRAWER_HEADER_CLASS.contains("tw:bg-"));
    }

    fn meta(slug: &str, seed: u8, archived: bool) -> ProjectMeta {
        ProjectMeta {
            uid: PrefixedUid::mint(UidPrefix::Project, &[seed; 16]),
            slug: slug.to_string(),
            access: Access::View,
            owner: Actor::Anonymous,
            archived,
        }
    }
}
