//! Save-panel change list: dense pending-edit rows with per-entry revert.
//!
//! The pending-edit surfaces share this module: the project detail popup's
//! per-bucket sections (unsaved / failed) render the full editor list
//! bucketed, and the node detail popup renders the node's own entries the
//! same way. Sections are `DetailSection`s titled with the bucket name (the
//! count rides the title row's meta cell) and tinted via
//! [`bucket_section_tint`]. Long lists scroll INSIDE the popover (the list
//! caps its own height) per the `DetailPopover` conventions.

use dioxus::prelude::*;
use lpa_studio_core::{UiAction, UiPendingEdit, UiPendingEditKind, UiPendingEditPhase};

use crate::base::{DetailSectionTint, InlineButton, InlineButtonTone};

/// The save-panel buckets, mirroring `UiPendingEditPhase` for filtering
/// entries into their popup sections.
///
/// There is no live/debug bucket (D7): a debug override is not dirty, so the
/// controller never lists it — the save panel is about work Save would write
/// and work that failed, nothing else.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PendingEditBucket {
    /// Written to project files on save (the "Unsaved" section).
    Persisted,
    /// Failed edits needing attention (the "Failed" section).
    Failed,
}

/// The entries belonging to one save-panel bucket, in the DTO's stable order.
pub(crate) fn entries_in(edits: &[UiPendingEdit], bucket: PendingEditBucket) -> Vec<UiPendingEdit> {
    edits
        .iter()
        .filter(|edit| edit_bucket(edit) == bucket)
        .cloned()
        .collect()
}

fn edit_bucket(edit: &UiPendingEdit) -> PendingEditBucket {
    match edit.phase {
        UiPendingEditPhase::Persisted => PendingEditBucket::Persisted,
        UiPendingEditPhase::Failed { .. } => PendingEditBucket::Failed,
    }
}

/// The [`DetailSectionTint`] a pending-edit bucket section wears while it
/// holds entries — the same treatment as edited/failed slot rows, worn
/// on the section TITLE per the `DetailSection` convention. A bucket at zero
/// stays untinted (its section reads as plain information).
///
/// Shared by every pending-edit surface (project popup, node popup) so the
/// phase → color mapping lives in exactly one place.
pub(crate) fn bucket_section_tint(bucket: PendingEditBucket, count: usize) -> DetailSectionTint {
    if count == 0 {
        return DetailSectionTint::None;
    }
    match bucket {
        PendingEditBucket::Persisted => DetailSectionTint::Warning,
        PendingEditBucket::Failed => DetailSectionTint::Error,
    }
}

/// Display string for what an entry does, folding in the saved value it
/// replaces when the entry carries one (P3, ADR follow-up (b)): assigns read
/// `old → new` (degrading to `set → new` when no old value is known);
/// structural gestures keep their verb and append the replaced value as one
/// dense `(was …)` token where the base held something. Rows stay one line.
/// Asset-body edits are whole-file replaces, so they read `body → {size}`
/// with no old value.
fn kind_display(kind: &UiPendingEditKind, old_value: Option<&str>) -> String {
    match (kind, old_value) {
        (UiPendingEditKind::Assign { value_display }, Some(old_value)) => {
            format!("{old_value} \u{2192} {value_display}")
        }
        (UiPendingEditKind::Assign { value_display }, None) => {
            format!("set \u{2192} {value_display}")
        }
        (UiPendingEditKind::Added, Some(old_value)) => format!("added (was {old_value})"),
        (UiPendingEditKind::Added, None) => "added".to_string(),
        (UiPendingEditKind::Removed, Some(old_value)) => format!("removed (was {old_value})"),
        (UiPendingEditKind::Removed, None) => "removed".to_string(),
        (UiPendingEditKind::Moved { from, to }, _) => format!("key {from} \u{2192} {to}"),
        // A staged body deletion is a file fate, not a value change: say so
        // instead of the replace form ("body → deleted").
        (UiPendingEditKind::AssetBody { detail }, _) if detail == "deleted" => {
            "file deleted on save".to_string()
        }
        (UiPendingEditKind::AssetBody { detail }, _) => format!("body \u{2192} {detail}"),
        // The staged removal row: the entry's label is the removed node's
        // name and its path is the attachment site; the staged file
        // deletions list separately as their own file rows.
        (UiPendingEditKind::NodeRemoved, _) => {
            "node removed \u{2014} staged until save".to_string()
        }
    }
}

/// Revert-button wording matching the per-slot detail popups: "Revert" for
/// unsaved persisted edits (and failed entries, where it clears the parked
/// error) — except the staged node removal, whose revert restores the node
/// (and cancels its staged file deletions), so it reads "Restore". Debug
/// overrides never reach this list (D7); their verb is Clear, on the row.
fn revert_label(edit: &UiPendingEdit) -> (&'static str, &'static str) {
    if matches!(edit.kind, UiPendingEditKind::NodeRemoved) {
        return (
            "Restore",
            "Restore this node and cancel its staged file deletions",
        );
    }
    match edit.phase {
        UiPendingEditPhase::Persisted => ("Revert", "Discard this pending edit"),
        UiPendingEditPhase::Failed { .. } => ("Revert", "Discard this failed edit"),
    }
}

/// One bucket's change list for the project detail popup. Renders nothing
/// for an empty bucket (the header count row is the whole empty state).
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn PendingEditList(entries: Vec<UiPendingEdit>, on_action: EventHandler<UiAction>) -> Element {
    if entries.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "tw:grid tw:max-h-44 tw:content-start tw:overflow-y-auto tw:pt-0.5",
            for entry in entries {
                PendingEditRow { entry, on_action }
            }
        }
    }
}

/// One dense change-list row: node label + slot path, the op/value line, the
/// failure reason for failed entries, and the entry's small revert button.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn PendingEditRow(entry: UiPendingEdit, on_action: EventHandler<UiAction>) -> Element {
    let kind = kind_display(&entry.kind, entry.old_value.as_deref());
    let (label, title) = revert_label(&entry);
    let reason = match &entry.phase {
        UiPendingEditPhase::Failed { reason } if !reason.is_empty() => Some(reason.clone()),
        _ => None,
    };
    let revert = entry.revert.clone();

    rsx! {
        div { class: "tw:grid tw:grid-cols-[minmax(0,1fr)_auto] tw:items-center tw:gap-x-2 tw:border-t tw:border-border-muted tw:py-1 tw:first:border-t-0",
            div { class: "tw:grid tw:min-w-0 tw:gap-0.5",
                p { class: "tw:m-0 tw:flex tw:min-w-0 tw:items-baseline tw:gap-x-1.5 tw:text-xs tw:leading-snug",
                    span { class: "tw:flex-none tw:font-bold tw:text-subtle-foreground", "{entry.node_label}" }
                    code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-muted-foreground", "{entry.slot_path_display}" }
                }
                p { class: "tw:m-0 tw:min-w-0 tw:truncate tw:font-mono tw:text-[0.68rem] tw:leading-snug tw:text-muted-foreground", "{kind}" }
                if let Some(reason) = reason {
                    p { class: "tw:m-0 tw:text-[0.68rem] tw:leading-snug tw:text-status-error-foreground tw:break-words", "{reason}" }
                }
            }
            if let Some(revert) = revert {
                InlineButton {
                    label: label.to_string(),
                    title: title.to_string(),
                    text: label.to_string(),
                    tone: InlineButtonTone::Warning,
                    on_press: move |_| on_action.call(revert.clone()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(path: &str, phase: UiPendingEditPhase) -> UiPendingEdit {
        UiPendingEdit {
            node_label: "Orbit".to_string(),
            node_path: "/demo.module/orbit.shader".to_string(),
            slot_path_display: path.to_string(),
            kind: UiPendingEditKind::Added,
            old_value: None,
            phase,
            revert: None,
        }
    }

    #[test]
    fn entries_filter_into_their_bucket_preserving_order() {
        let edits = vec![
            edit("entries[a]", UiPendingEditPhase::Persisted),
            edit(
                "entries[c]",
                UiPendingEditPhase::Failed {
                    reason: "rejected".to_string(),
                },
            ),
            edit("entries[b]", UiPendingEditPhase::Persisted),
        ];

        let paths = |bucket| {
            entries_in(&edits, bucket)
                .into_iter()
                .map(|edit| edit.slot_path_display)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(PendingEditBucket::Persisted),
            vec!["entries[a]", "entries[b]"]
        );
        assert_eq!(paths(PendingEditBucket::Failed), vec!["entries[c]"]);
    }

    #[test]
    fn kind_display_shows_value_for_assigns_and_words_for_gestures() {
        assert_eq!(
            kind_display(
                &UiPendingEditKind::Assign {
                    value_display: "0.5".to_string()
                },
                None
            ),
            "set \u{2192} 0.5"
        );
        assert_eq!(kind_display(&UiPendingEditKind::Added, None), "added");
        assert_eq!(kind_display(&UiPendingEditKind::Removed, None), "removed");
        assert_eq!(
            kind_display(
                &UiPendingEditKind::Moved {
                    from: "[a]".to_string(),
                    to: "[c]".to_string()
                },
                None
            ),
            "key [a] \u{2192} [c]"
        );
    }

    #[test]
    fn kind_display_folds_in_the_old_value_when_known() {
        // Assigns read old → new; structural kinds append one dense token.
        assert_eq!(
            kind_display(
                &UiPendingEditKind::Assign {
                    value_display: "0.85".to_string()
                },
                Some("0.5")
            ),
            "0.5 \u{2192} 0.85"
        );
        assert_eq!(
            kind_display(&UiPendingEditKind::Removed, Some("{\"warm\":0.5}")),
            "removed (was {\"warm\":0.5})"
        );
        assert_eq!(
            kind_display(&UiPendingEditKind::Added, Some("{}")),
            "added (was {})"
        );
        // A move's display is its key transition; old values never apply.
        assert_eq!(
            kind_display(
                &UiPendingEditKind::Moved {
                    from: "[a]".to_string(),
                    to: "[c]".to_string()
                },
                Some("16")
            ),
            "key [a] \u{2192} [c]"
        );
    }

    #[test]
    fn bucket_tint_follows_the_slot_row_treatment_only_while_populated() {
        assert_eq!(
            bucket_section_tint(PendingEditBucket::Persisted, 2),
            DetailSectionTint::Warning
        );
        assert_eq!(
            bucket_section_tint(PendingEditBucket::Failed, 1),
            DetailSectionTint::Error
        );
        for bucket in [PendingEditBucket::Persisted, PendingEditBucket::Failed] {
            assert_eq!(bucket_section_tint(bucket, 0), DetailSectionTint::None);
        }
    }

    #[test]
    fn node_removed_and_deleted_file_rows_read_as_fates_not_value_changes() {
        // The staged removal row (label = removed node, path = the site).
        assert_eq!(
            kind_display(&UiPendingEditKind::NodeRemoved, None),
            "node removed \u{2014} staged until save"
        );
        // Old values never leak onto the removal row (the site's base JSON
        // would be noise; the label already names the node).
        assert_eq!(
            kind_display(&UiPendingEditKind::NodeRemoved, Some("{\"node\":1}")),
            "node removed \u{2014} staged until save"
        );
        // Staged file deletions are file fates…
        assert_eq!(
            kind_display(
                &UiPendingEditKind::AssetBody {
                    detail: "deleted".to_string()
                },
                None
            ),
            "file deleted on save"
        );
        // …while body replacements keep the replace form.
        assert_eq!(
            kind_display(
                &UiPendingEditKind::AssetBody {
                    detail: "3.2 KB".to_string()
                },
                None
            ),
            "body \u{2192} 3.2 KB"
        );
    }

    #[test]
    fn node_removed_revert_reads_restore() {
        let mut removal = edit("nodes[orbit]", UiPendingEditPhase::Persisted);
        removal.kind = UiPendingEditKind::NodeRemoved;
        let (label, title) = revert_label(&removal);
        assert_eq!(label, "Restore");
        assert!(title.contains("staged file deletions"));
    }

    #[test]
    fn revert_wording_matches_the_slot_detail_popups() {
        assert_eq!(
            revert_label(&edit("a", UiPendingEditPhase::Persisted)).0,
            "Revert"
        );
        assert_eq!(
            revert_label(&edit(
                "a",
                UiPendingEditPhase::Failed {
                    reason: "rejected".to_string()
                }
            ))
            .0,
            "Revert"
        );
    }
}
