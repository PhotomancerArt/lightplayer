//! Table row presentation for a single config slot.

use dioxus::prelude::*;
use lpa_studio_core::{
    ProjectSlotAddress, UiAction, UiConfigSlot, UiConfigSlotBody, UiNodeDirtyState,
    UiSlotComposite, UiSlotFieldState, UiSlotMapKeyKind,
};

use crate::app::node::slot_edit_actions::slot_revert_action;
use crate::app::node::slot_option_presence::{
    OptionPresenceWidth, option_presence_child_slot, option_presence_chip,
};
use crate::app::node::{
    AssetEditor, EnumVariantField, MapAddEntry, MapEntryKeyField, MapEntryRemoveButton,
    OptionPresenceActionButton, OptionPresenceCell, OptionPresenceCheckbox, OptionPresenceStyle,
    SlotDetailButton, SlotDetailRevert, SlotRecordEditor, SlotValueEditor, primary_affordance,
    slot_row_class,
};
use crate::base::{StudioIcon, StudioIconName};

/// Edit chrome for a touched slot row: persisted edits wear the warning
/// (amber) tint and count toward Save, transient edits the live (blue) tint
/// (applied to the running project and never written by Save). The tint plus
/// the affordance icon carry the whole row treatment — no text chips (M3 UX
/// gate); text stays in the popups and the save panel. Rows with an own edit
/// entry additionally get the inline revert icon; the detail popup keeps its
/// revert footer as the second access point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SlotEditChrome {
    Unsaved,
    Live,
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
pub fn ConfigSlotRow(
    slot: UiConfigSlot,
    depth: usize,
    index: usize,
    #[props(default = false)] initially_open: bool,
    #[props(default = None)] initially_expanded: Option<bool>,
    /// Set when this row is a map entry of a map with this key domain
    /// (threaded by the parent map's record editor): renders the per-entry
    /// remove affordance and the click-to-edit key label.
    #[props(default = None)]
    entry_key_kind: Option<UiSlotMapKeyKind>,
    /// Open the map add-entry key input on first render (stories).
    #[props(default = false)]
    initially_adding: bool,
    /// Open this map entry row's key input on first render (stories).
    #[props(default = false)]
    initially_key_editing: bool,
    /// Option-presence rendering style: [`OptionPresenceStyle::PresenceInRow`]
    /// is the live default (P5); stories select the non-default candidates
    /// for the P7 review comparison.
    #[props(default)]
    option_presence: OptionPresenceStyle,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    let child_record = match &slot.body {
        UiConfigSlotBody::Record(record) if !record.fields.is_empty() => Some(record.clone()),
        _ => None,
    };
    let child_asset = match &slot.body {
        UiConfigSlotBody::Asset(asset) => Some(asset.clone()),
        _ => None,
    };
    let has_children = child_record.is_some() || child_asset.is_some();
    let mut expanded = use_signal(|| initially_expanded.unwrap_or(depth > 0 || !has_children));
    let aspects = slot.visible_aspects();
    let primary = primary_affordance(&aspects);
    let chrome = slot_edit_chrome(&slot.state);
    let row_class = match chrome {
        Some(SlotEditChrome::Live) if slot.state.invalid.is_none() => live_row_class(),
        _ => slot_row_class(primary, index),
    };
    let indent = depth * 14;
    // Value edits on a present option row target the interior `some` slot;
    // the option's own address is the some/none toggle's remove target.
    let body_address = match &slot.optionality {
        Some(optionality) if optionality.included => slot
            .address
            .as_ref()
            .and_then(|address| address.child_field("some")),
        _ => slot.address.clone(),
    };
    // Key domain threaded to child rows when this row is a map composite.
    let child_entry_key_kind = match &slot.composite {
        Some(UiSlotComposite::Map(map)) => Some(map.key_kind),
        _ => None,
    };
    let entry_wiring = (entry_key_kind.is_some() && slot.state.editable)
        .then(|| slot.address.clone().zip(on_action))
        .flatten();
    let remove_entry = entry_wiring.clone();
    // Click-to-edit key label for map entry rows (dispatches `MoveEntry` on
    // the parent map); rows without edit wiring keep the static label.
    let key_edit = entry_key_kind.zip(entry_wiring);
    // Inline revert: only rows with an OWN edit entry offer it (prefix-only
    // dirty composites carry `None`; their per-entry revert is the save
    // panel's). Same action as the popup footer — two access points.
    let row_revert = chrome.and_then(|chrome| {
        Some(RowRevert {
            chrome,
            address: slot.edit_entry_address.clone()?,
            on_action: on_action?,
        })
    });
    // Option rows render as presence (P5, the live default): the value cell
    // is the stable-width presence cell and the trailing gesture slot holds
    // the set/clear affordance. The reservation tier follows the body's
    // value-kind width class.
    let presence = slot.optionality;
    let presence_chip = presence
        .and_then(|optionality| option_presence_chip(option_presence, optionality.included));
    let presence_width = OptionPresenceWidth::for_body(&slot.body);
    // Candidate B: the interior value renders as a child row when set.
    let presence_child = match presence {
        Some(optionality)
            if option_presence == OptionPresenceStyle::ChildRow && optionality.included =>
        {
            Some(option_presence_child_slot(&slot, body_address.clone()))
        }
        _ => None,
    };

    rsx! {
        div { class: "tw:grid tw:min-w-0",
            div { class: row_class,
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:gap-1.5", style: "padding-left: {indent}px;",
                    if has_children {
                        button {
                            class: "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border-0 tw:bg-transparent tw:p-0 tw:text-muted-foreground tw:transition-colors tw:hover:text-strong-foreground tw:focus-visible:outline tw:focus-visible:outline-1 tw:focus-visible:outline-border-strong",
                            style: "appearance: none; -webkit-appearance: none; border: 0; background: transparent; cursor: pointer;",
                            r#type: "button",
                            aria_label: if expanded() { "Collapse slot" } else { "Expand slot" },
                            title: if expanded() { "Collapse slot" } else { "Expand slot" },
                            onclick: move |_| expanded.set(!expanded()),
                            span { class: expand_chevron_class(expanded()),
                                style: "stroke-width: 3;",
                                StudioIcon {
                                    name: StudioIconName::Collapsed,
                                    size: 16,
                                }
                            }
                        }
                    } else {
                        span { class: "tw:h-6 tw:w-6 tw:flex-none" }
                    }
                    div { class: "tw:min-w-0",
                        if let Some((key_kind, (address, handler))) = key_edit {
                            MapEntryKeyField {
                                label: slot.label.clone(),
                                key_kind,
                                address,
                                on_action: handler,
                                initially_editing: initially_key_editing,
                            }
                        } else if has_children {
                            button {
                                class: "tw:block tw:min-w-0 tw:appearance-none tw:border-0 tw:bg-transparent tw:p-0 tw:text-left",
                                style: "appearance: none; -webkit-appearance: none; border: 0; background: transparent; cursor: pointer;",
                                r#type: "button",
                                aria_label: if expanded() { "Collapse slot" } else { "Expand slot" },
                                onclick: move |_| expanded.set(!expanded()),
                                strong { class: "tw:block tw:min-w-0 tw:text-sm tw:font-semibold tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                            }
                        } else {
                            strong { class: "tw:block tw:min-w-0 tw:text-sm tw:font-semibold tw:leading-tight tw:text-strong-foreground tw:break-words", "{slot.label}" }
                        }
                    }
                }
                div { class: "tw:flex tw:min-w-0 tw:items-center tw:justify-end tw:gap-2 tw:text-sm tw:leading-tight tw:text-muted-foreground",
                    if let Some(revert) = row_revert {
                        SlotRowRevertButton { revert }
                    }
                    if let Some(optionality) = presence {
                        // Option-ness as presence (P5 live default): the
                        // value cell is the stable-width presence cell
                        // (editor or chip, one reserved footprint), and the
                        // trailing gesture slot holds the fixed-square
                        // set/clear affordance — both ends jump-free by
                        // construction.
                        OptionPresenceCell {
                            chip: presence_chip,
                            width: presence_width,
                            SlotBodyPreview {
                                body: slot.body.clone(),
                                state: slot.state.clone(),
                                expanded: expanded(),
                                address: body_address.clone(),
                                composite: slot.composite.clone(),
                                initially_adding,
                                on_action,
                            }
                        }
                        if option_presence == OptionPresenceStyle::CheckboxSquare {
                            OptionPresenceCheckbox {
                                optionality,
                                address: slot.address.clone(),
                                on_action,
                            }
                        } else {
                            OptionPresenceActionButton {
                                optionality,
                                address: slot.address.clone(),
                                on_action,
                            }
                        }
                    } else {
                        SlotBodyPreview {
                            body: slot.body.clone(),
                            state: slot.state.clone(),
                            expanded: expanded(),
                            address: body_address,
                            composite: slot.composite.clone(),
                            initially_adding,
                            on_action,
                        }
                    }
                    if let Some((address, handler)) = remove_entry {
                        MapEntryRemoveButton { address, on_action: handler }
                    }
                }
                SlotDetailButton {
                    label: slot.label.clone(),
                    aspects,
                    initially_open,
                    revert: slot_detail_revert(chrome, slot.edit_entry_address.clone(), on_action),
                }
            }
            if let Some(child) = presence_child {
                // Candidate B's interior value row (rhymes with map
                // entries): always visible while set — presence, not
                // expansion, controls it.
                div { class: "tw:grid tw:min-w-0 tw:overflow-hidden tw:border-t tw:border-border-muted",
                    ConfigSlotRow { slot: child, depth: depth + 1, index: 0, on_action }
                }
            }
            if expanded() {
                if let Some(record) = child_record {
                    SlotRecordEditor {
                        record,
                        depth: depth + 1,
                        separated: true,
                        entry_key_kind: child_entry_key_kind,
                        on_action,
                    }
                }
                if let Some(asset) = child_asset {
                    AssetSlotEditor { asset, on_action }
                }
            }
        }
    }
}

/// The one revert verb vocabulary (M3 UX gate): "Revert" for unsaved
/// (persisted) edits, "Reset" for live (transient) controls — shared by the
/// inline row icon and the detail-popup footer so the two access points can
/// never diverge.
fn chrome_revert_labels(chrome: SlotEditChrome) -> (&'static str, &'static str) {
    match chrome {
        SlotEditChrome::Unsaved => ("Revert", "Discard this pending edit"),
        SlotEditChrome::Live => ("Reset", "Reset this live control to its authored value"),
    }
}

/// Tone for the inline revert button, from the same status token families as
/// the row tint and the edited affordance icon: warning (amber) for unsaved
/// persisted edits, live (blue) for transient controls — so the button reads
/// as part of the row's unsaved/live chrome rather than a neutral control.
fn chrome_revert_button_class(chrome: SlotEditChrome) -> &'static str {
    match chrome {
        SlotEditChrome::Unsaved => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-warning-border tw:bg-status-warning-bg tw:p-0 tw:text-status-warning-foreground tw:transition-colors tw:hover:border-status-warning-foreground"
        }
        SlotEditChrome::Live => {
            "tw:inline-flex tw:h-6 tw:w-6 tw:flex-none tw:cursor-pointer tw:appearance-none tw:items-center tw:justify-center tw:rounded-xs tw:border tw:border-status-live-border tw:bg-status-live-bg tw:p-0 tw:text-status-live-foreground tw:transition-colors tw:hover:border-status-live-foreground"
        }
    }
}

/// Inline revert affordance data for a touched row with an own edit entry.
#[derive(Clone, PartialEq)]
struct RowRevert {
    chrome: SlotEditChrome,
    address: ProjectSlotAddress,
    on_action: EventHandler<UiAction>,
}

/// Compact revert icon button on a touched slot row: the same revert icon
/// token as the node/project headers, dispatching `SlotEditOp::Revert` at the
/// row's own edit entry. Tooltip verb per [`chrome_revert_labels`], tone per
/// [`chrome_revert_button_class`]. It renders at the LEADING edge of the
/// end-aligned value area: the value controls stay right-anchored, so the
/// button appearing/disappearing on the first edit never shifts the input.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotRowRevertButton(revert: RowRevert) -> Element {
    let RowRevert {
        chrome,
        address,
        on_action,
    } = revert;
    let (label, title) = chrome_revert_labels(chrome);

    rsx! {
        button {
            class: chrome_revert_button_class(chrome),
            r#type: "button",
            aria_label: label,
            title: "{label}: {title}",
            onclick: move |event| {
                event.stop_propagation();
                on_action.call(slot_revert_action(address.clone()));
            },
            StudioIcon {
                name: StudioIconName::Revert,
                size: 13,
            }
        }
    }
}

/// The detail-popup revert affordance for a touched slot, using the same
/// verb vocabulary as the inline row icon. The target is the row's **own**
/// edit entry (`UiConfigSlot.edit_entry_address`); composite rows that are
/// only prefix-dirty carry no entry of their own and offer no row revert —
/// per-entry revert for those lives in the save panel.
fn slot_detail_revert(
    chrome: Option<SlotEditChrome>,
    address: Option<ProjectSlotAddress>,
    on_action: Option<EventHandler<UiAction>>,
) -> Option<SlotDetailRevert> {
    let (label, title) = chrome_revert_labels(chrome?);
    Some(SlotDetailRevert {
        label,
        title,
        address: address?,
        on_action: on_action?,
    })
}

#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn SlotBodyPreview(
    body: UiConfigSlotBody,
    state: UiSlotFieldState,
    expanded: bool,
    #[props(default = None)] address: Option<ProjectSlotAddress>,
    #[props(default = None)] composite: Option<UiSlotComposite>,
    #[props(default = false)] initially_adding: bool,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    match body {
        UiConfigSlotBody::Empty => rsx! {
            span { class: "tw:text-subtle-foreground", "unset" }
        },
        UiConfigSlotBody::Value(value) => rsx! {
            SlotValueEditor { value, state, address, on_action }
        },
        UiConfigSlotBody::Record(record) => match composite {
            Some(UiSlotComposite::Enum(composite)) => rsx! {
                EnumVariantField { composite, state, address, on_action }
            },
            Some(UiSlotComposite::Map(composite)) => {
                let label = summary_label(record.fields.len(), "entry", "entries");
                // The add affordance shows alongside the expanded entries;
                // an empty map has no rows to expand, so it shows directly.
                let adding = expanded || record.fields.is_empty();
                rsx! {
                    span { class: record_summary_class(expanded), "{label}" }
                    if adding {
                        MapAddEntry {
                            composite,
                            state,
                            address,
                            initially_open: initially_adding,
                            on_action,
                        }
                    }
                }
            }
            None => {
                let label = summary_label(record.fields.len(), "field", "fields");
                rsx! {
                    span { class: record_summary_class(expanded), "{label}" }
                }
            }
        },
        UiConfigSlotBody::Asset(asset) => rsx! {
            code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-muted-foreground", "{asset.source}" }
        },
    }
}

fn summary_label(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 {
        format!("1 {singular}")
    } else {
        format!("{count} {plural}")
    }
}

/// An asset slot's expanded body. File-backed editable assets render the
/// inline [`AssetEditor`] (edit in place, output stays visible); inline,
/// binary, or unresolvable assets keep the read-only presentation.
#[component]
#[allow(non_snake_case, reason = "Dioxus components use PascalCase")]
fn AssetSlotEditor(
    asset: lpa_studio_core::UiSlotAsset,
    #[props(default)] on_action: Option<EventHandler<UiAction>>,
) -> Element {
    if let Some(editor) = asset.inline_editor.clone() {
        return rsx! {
            AssetEditor { editor, on_action }
        };
    }

    rsx! {
        div { class: "tw:border-t tw:border-border-muted tw:bg-page tw:px-2 tw:py-2",
            div { class: "tw:mb-1.5 tw:flex tw:min-w-0 tw:items-center tw:justify-between tw:gap-2",
                code { class: "tw:min-w-0 tw:truncate tw:font-mono tw:text-xs tw:text-subtle-foreground", "{asset.source}" }
                span { class: "tw:flex-none tw:text-xs tw:font-bold tw:text-subtle-foreground", "{asset.editor_label()}" }
            }
            if let Some(detail) = asset.detail.as_ref() {
                p { class: "tw:m-0 tw:mb-1.5 tw:text-xs tw:text-subtle-foreground tw:break-words", "{detail}" }
            }
            if let Some(content) = asset.content {
                pre { class: "tw:m-0 tw:max-h-48 tw:overflow-auto tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-3 tw:font-mono tw:text-xs tw:leading-normal tw:text-muted-foreground",
                    code { "{content}" }
                }
            } else {
                pre { class: "tw:m-0 tw:rounded-xs tw:border tw:border-border-subtle tw:bg-terminal tw:p-3 tw:font-mono tw:text-xs tw:leading-normal tw:text-subtle-foreground",
                    code { "// asset content not loaded" }
                }
            }
        }
    }
}

/// The edit chrome for a row, when its slot has been touched.
fn slot_edit_chrome(state: &UiSlotFieldState) -> Option<SlotEditChrome> {
    if state.dirty == UiNodeDirtyState::Clean {
        return None;
    }
    Some(if state.live {
        SlotEditChrome::Live
    } else {
        SlotEditChrome::Unsaved
    })
}

/// Row treatment for live-dirty rows: the dedicated live (blue) tint keeps a
/// touched runtime control distinct from both the warning-tinted unsaved
/// (persisted) rows and the good/success (green) treatments.
fn live_row_class() -> &'static str {
    "tw:grid tw:min-w-0 tw:grid-cols-[minmax(120px,0.4fr)_minmax(0,1fr)_32px] tw:items-center tw:gap-2 tw:bg-[linear-gradient(270deg,var(--studio-status-live-bg)_0%,var(--studio-status-live-bg)_34%,transparent_100%)] tw:px-2 tw:py-1.5"
}

fn record_summary_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:text-xs tw:font-semibold tw:uppercase tw:text-subtle-foreground"
    } else {
        "tw:text-xs tw:font-semibold tw:uppercase tw:text-muted-foreground"
    }
}

fn expand_chevron_class(expanded: bool) -> &'static str {
    if expanded {
        "tw:inline-flex tw:rotate-90 tw:transition-transform"
    } else {
        "tw:inline-flex tw:transition-transform"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revert_button_wears_the_row_chrome_status_tokens() {
        // Unsaved: the warning (amber) family shared with the row tint and
        // the edited affordance icon.
        let unsaved = chrome_revert_button_class(SlotEditChrome::Unsaved);
        assert!(unsaved.contains("tw:border-status-warning-border"));
        assert!(unsaved.contains("tw:bg-status-warning-bg"));
        assert!(unsaved.contains("tw:text-status-warning-foreground"));

        // Live: the live (blue) family, same position and shape.
        let live = chrome_revert_button_class(SlotEditChrome::Live);
        assert!(live.contains("tw:border-status-live-border"));
        assert!(live.contains("tw:bg-status-live-bg"));
        assert!(live.contains("tw:text-status-live-foreground"));
    }

    #[test]
    fn revert_verbs_stay_per_chrome() {
        assert_eq!(chrome_revert_labels(SlotEditChrome::Unsaved).0, "Revert");
        assert_eq!(chrome_revert_labels(SlotEditChrome::Live).0, "Reset");
    }
}
