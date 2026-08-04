//! Bus **wiring** view DTOs: channels, values, and linked writer/reader
//! sites. Rendered by the module card's wiring drawer as the flow view
//! (`[writers] → [value box] → [readers]`, one view per scope — the
//! 2026-08-03 wiring-UI spike, `spikes/wiring-ui/index.html`).
//!
//! Site *flavor* is first-class here because the flow view draws it:
//! default-origin bindings, engaged panel writers (R10), module publishes
//! (R7), writers shadowed by priority (R5/R11), and readers that live in a
//! child scope but resolve to this channel (R5 inheritance, spike gate 3).

use crate::{
    UiAction, UiProductKind, UiProductPreview, UiProductPreviewFrame, UiProductTrackingState,
    UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
};

/// One scope's wiring: every channel of that scope referenced by at least
/// one binding.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusView {
    /// Channels in wire order (binding-index discovery order).
    pub channels: Vec<UiBusChannelView>,
}

impl UiBusView {
    /// A bus view with no channels (empty project or snapshot pending).
    pub fn empty() -> Self {
        Self {
            channels: Vec::new(),
        }
    }
}

/// One bus channel row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusChannelView {
    /// Structured scope this channel entry lists in — the Studio-side key
    /// is `(scope, name)`; same-named channels in different scopes are
    /// distinct rows. `None` for pre-scope snapshots/fixtures.
    pub scope: Option<lpc_wire::WireScopeRef>,
    /// Derived display label for the scope (owner node's label; empty for
    /// the root scope). Presentation only — never used as a key.
    pub scope_label: Option<String>,
    /// Channel name (`time`, `trigger`, `visual.out`, …).
    pub name: String,
    /// Established semantic kind label, when known.
    pub kind: Option<String>,
    /// Resolved current value display, when the snapshot carried values.
    pub value: Option<String>,
    /// Resolution failure detail, when the value could not resolve.
    pub value_error: Option<String>,
    /// The primary-visual channel (`visual.out`) — the product's main
    /// output; previews hang off it (roadmap M6).
    pub primary_visual: bool,
    /// Two or more writers tie at top priority (E3): resolution is
    /// ambiguous until the author picks. Display-only — the pick gesture
    /// is future work (modules.md §5).
    pub contended: bool,
    /// Live preview payload when the resolved value is a visual product —
    /// exactly the fields the shared `ProductPreview` component consumes,
    /// so the value box shows the picture, not a debug string. `None`
    /// when the value is not a product or its preview is not in the
    /// tracked preview stream.
    pub preview: Option<UiBusChannelPreview>,
    /// Sites publishing to this channel, highest priority first.
    pub writers: Vec<UiBusSiteView>,
    /// Sites consuming from this channel.
    pub readers: Vec<UiBusSiteView>,
}

/// A channel value's product preview, lean enough for one wiring row.
///
/// Deliberately NOT [`crate::UiProducedProduct`]: that carries binding,
/// authoring, and dirty state that belong to a produced slot row, none of
/// which a bus channel's value box has.
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusChannelPreview {
    /// Product family (drives the preview frame treatment).
    pub kind: UiProductKind,
    /// Current preview bytes/state.
    pub preview: UiProductPreview,
    /// Whether Studio is watching this product now.
    pub tracking: UiProductTrackingState,
    /// Stable frame used before bytes arrive.
    pub frame: UiProductPreviewFrame,
}

impl UiBusChannelView {
    /// Detail-popup aspects for a channel pane, mirroring the produced-value
    /// popup shape: an info section, then the wiring (writers → readers) with
    /// multi-writer semantics spelled out (roadmap D11: merge modes and
    /// multiple sources are clearly indicated in the detail popup).
    pub fn visible_aspects(&self) -> Vec<UiSlotAspect> {
        vec![self.info_aspect(), self.wiring_aspect()]
    }

    fn info_aspect(&self) -> UiSlotAspect {
        let mut aspect = UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Channel")
            .with_row(UiSlotAspectRow::new("Name", format!("bus:{}", self.name)));
        if let Some(kind) = &self.kind {
            aspect = aspect.with_row(UiSlotAspectRow::new("Kind", kind.clone()));
        }
        if let Some(value) = &self.value {
            aspect = aspect.with_row(UiSlotAspectRow::new("Value", value.clone()));
        } else if let Some(error) = &self.value_error {
            aspect = aspect
                .with_row(UiSlotAspectRow::new("Value", "unresolved").with_detail(error.clone()));
        }
        if self.primary_visual {
            aspect = aspect.with_row(
                UiSlotAspectRow::new("Role", "Primary visual output")
                    .with_detail("The project's main output; previews render this channel."),
            );
        }
        aspect
    }

    fn wiring_aspect(&self) -> UiSlotAspect {
        let mut aspect = UiSlotAspect::new(UiSlotAspectKind::Binding, "Wiring")
            .with_affordance(UiSlotAffordance::Bound);
        if self.contended {
            aspect = aspect.with_row(
                UiSlotAspectRow::new("Contention", "ambiguous").with_detail(
                    "Multiple writers tie at top priority (E3); the author's pick \
                     resolves it. Until then resolution is ambiguous.",
                ),
            );
        }
        if self.writers.len() > 1 {
            aspect = aspect.with_row(
                UiSlotAspectRow::new("Writers", format!("{} writers", self.writers.len()))
                    .with_detail(
                        "Readers resolve the highest-priority writer; map-valued readers                          merge all writers by key.",
                    ),
            );
        }
        for site in &self.writers {
            aspect = aspect.with_row(site_row("Writer", site));
        }
        if self.writers.is_empty() {
            aspect = aspect.with_row(UiSlotAspectRow::new("Writer", "none"));
        }
        for site in &self.readers {
            aspect = aspect.with_row(site_row("Reader", site));
        }
        if self.readers.is_empty() {
            aspect = aspect.with_row(UiSlotAspectRow::new("Reader", "none"));
        }
        aspect
    }
}

fn site_row(label: &str, site: &UiBusSiteView) -> UiSlotAspectRow {
    let mut value = match &site.slot {
        Some(slot) => format!("{} .{slot}", site.node_label),
        None => site.node_label.clone(),
    };
    if let Some(scope) = &site.child_scope {
        value = format!("{scope} · {value}");
    }
    let mut row = UiSlotAspectRow::new(label, value);
    let mut details: Vec<&str> = Vec::new();
    match site.origin {
        UiBusSiteOrigin::Authored => {}
        UiBusSiteOrigin::Panel => details.push("panel writer (engaged)"),
        UiBusSiteOrigin::Default => details.push("default binding"),
    }
    if site.publish {
        details.push("module publish");
    }
    if site.shadowed {
        details.push("shadowed by a higher-priority writer");
    }
    if site.child_scope.is_some() {
        details.push("resolves here from a child scope (R5)");
    }
    if !details.is_empty() {
        row = row.with_detail(details.join(" · "));
    }
    if let Some(focus) = &site.focus {
        row = row.with_action(focus.clone());
    }
    row
}

/// One writer/reader site on a channel — always a navigation affordance:
/// clicking dispatches the focus action so the user lands on the node
/// (roadmap D7: the UI feels linked, no path hunting).
#[derive(Clone, Debug, PartialEq)]
pub struct UiBusSiteView {
    /// Display label of the owning node.
    pub node_label: String,
    /// Anchor slot on the node, when the binding has one.
    pub slot: Option<String>,
    /// Where the binding came from — authored, an engaged panel writer
    /// (R10), or default policy. The flow view styles each differently.
    pub origin: UiBusSiteOrigin,
    /// Publishing site owned by a module node: an R7 publish/export
    /// (styled bus-violet — the module contributes, a leaf writes).
    pub publish: bool,
    /// Writer only: outranked by a higher-priority writer for this
    /// channel (R5/R11) — drawn dim and dashed, still listed.
    pub shadowed: bool,
    /// Reader only: lives in a descendant module scope and resolves to
    /// this channel by walking outward (R5). The label is the display
    /// path from this scope down to the site's scope (usually one module
    /// label, "plasma_1"; "/"-joined at depth ≥ 2). Presentation only.
    pub child_scope: Option<String>,
    /// Focus/reveal action for the owning node, when it is in the tree.
    pub focus: Option<UiAction>,
}

impl UiBusSiteView {
    /// True when the binding came from default policy rather than
    /// authoring — kept as a reading aid where only that bit matters.
    pub fn default_origin(&self) -> bool {
        self.origin == UiBusSiteOrigin::Default
    }
}

/// Where a site's binding came from (mirrors `WireBindingOrigin`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiBusSiteOrigin {
    /// Authored in project data.
    Authored,
    /// An engaged panel writer — lazy runtime state, never authored
    /// (panel.md P1/P2; styled attention-orange like the engaged knob).
    Panel,
    /// Materialized from default binding policy.
    Default,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(label: &str, slot: Option<&str>, origin: UiBusSiteOrigin) -> UiBusSiteView {
        UiBusSiteView {
            node_label: label.to_string(),
            slot: slot.map(str::to_string),
            origin,
            publish: false,
            shadowed: false,
            child_scope: None,
            focus: None,
        }
    }

    fn channel() -> UiBusChannelView {
        UiBusChannelView {
            scope: None,
            scope_label: None,
            name: "trigger".to_string(),
            kind: Some("Instant".to_string()),
            value: Some("msg 3".to_string()),
            value_error: None,
            primary_visual: false,
            contended: false,
            preview: None,
            writers: vec![
                site("Button", Some("down"), UiBusSiteOrigin::Authored),
                site("Radio", Some("output"), UiBusSiteOrigin::Authored),
            ],
            readers: vec![site("Playlist", Some("trigger"), UiBusSiteOrigin::Default)],
        }
    }

    #[test]
    fn info_aspect_names_the_channel_with_scheme() {
        let aspects = channel().visible_aspects();
        let info = &aspects[0];
        assert_eq!(info.rows[0].label, "Name");
        assert_eq!(info.rows[0].value, "bus:trigger");
        assert_eq!(info.rows[1].value, "Instant");
    }

    #[test]
    fn multi_writer_channels_explain_merge_semantics() {
        let aspects = channel().visible_aspects();
        let wiring = &aspects[1];
        assert_eq!(wiring.affordance, Some(UiSlotAffordance::Bound));
        let summary = &wiring.rows[0];
        assert_eq!(summary.value, "2 writers");
        assert!(summary.detail.as_deref().unwrap().contains("merge"));
    }

    #[test]
    fn default_origin_sites_carry_the_default_detail() {
        let aspects = channel().visible_aspects();
        let wiring = &aspects[1];
        let reader = wiring
            .rows
            .iter()
            .find(|row| row.label == "Reader")
            .unwrap();
        assert_eq!(reader.value, "Playlist .trigger");
        assert_eq!(reader.detail.as_deref(), Some("default binding"));
    }

    #[test]
    fn sites_with_focus_carry_clickable_actions() {
        let mut ch = channel();
        ch.writers[0].focus = Some(crate::UiAction::from_op(
            crate::ControllerId::new("test.module"),
            crate::ProjectEditorOp::Focus,
        ));
        let aspects = ch.visible_aspects();
        let rows = &aspects[1].rows;
        let focused = rows
            .iter()
            .find(|row| row.value.starts_with("Button"))
            .unwrap();
        assert!(focused.action.is_some());
        let unfocused = rows
            .iter()
            .find(|row| row.value.starts_with("Radio"))
            .unwrap();
        assert!(unfocused.action.is_none());
    }

    #[test]
    fn site_flavors_and_child_scope_read_in_the_popup() {
        let mut ch = channel();
        ch.contended = true;
        ch.writers[0].origin = UiBusSiteOrigin::Panel;
        ch.writers[1].shadowed = true;
        ch.readers[0].child_scope = Some("plasma_1".to_string());
        let aspects = ch.visible_aspects();
        let wiring = &aspects[1];
        assert_eq!(wiring.rows[0].label, "Contention");
        let panel = wiring
            .rows
            .iter()
            .find(|row| row.value.starts_with("Button"))
            .unwrap();
        assert!(panel.detail.as_deref().unwrap().contains("panel writer"));
        let shadowed = wiring
            .rows
            .iter()
            .find(|row| row.value.starts_with("Radio"))
            .unwrap();
        assert!(shadowed.detail.as_deref().unwrap().contains("shadowed"));
        let inherited = wiring
            .rows
            .iter()
            .find(|row| row.label == "Reader")
            .unwrap();
        assert!(inherited.value.starts_with("plasma_1 · Playlist"));
        assert!(inherited.detail.as_deref().unwrap().contains("child scope"));
    }

    #[test]
    fn single_writer_has_no_merge_summary_and_primary_has_role() {
        let mut ch = channel();
        ch.writers.truncate(1);
        ch.primary_visual = true;
        let aspects = ch.visible_aspects();
        assert!(aspects[1].rows.iter().all(|row| row.label != "Writers"));
        assert!(aspects[0].rows.iter().any(|row| row.label == "Role"));
    }
}
