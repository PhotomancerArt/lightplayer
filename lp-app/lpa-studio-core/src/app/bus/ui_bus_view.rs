//! Bus pane view DTOs: channels, values, and linked writer/reader sites.

use crate::{UiAction, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow};

/// The bus pane body: every channel referenced by at least one binding.
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
    /// Sites publishing to this channel, highest priority first.
    pub writers: Vec<UiBusSiteView>,
    /// Sites consuming from this channel.
    pub readers: Vec<UiBusSiteView>,
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
    let value = match &site.slot {
        Some(slot) => format!("{} .{slot}", site.node_label),
        None => site.node_label.clone(),
    };
    let mut row = UiSlotAspectRow::new(label, value);
    if site.default_origin {
        row = row.with_detail("default binding");
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
    /// True when the binding came from default policy rather than authoring.
    pub default_origin: bool,
    /// Focus/reveal action for the owning node, when it is in the tree.
    pub focus: Option<UiAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(label: &str, slot: Option<&str>, default_origin: bool) -> UiBusSiteView {
        UiBusSiteView {
            node_label: label.to_string(),
            slot: slot.map(str::to_string),
            default_origin,
            focus: None,
        }
    }

    fn channel() -> UiBusChannelView {
        UiBusChannelView {
            name: "trigger".to_string(),
            kind: Some("Instant".to_string()),
            value: Some("msg 3".to_string()),
            value_error: None,
            primary_visual: false,
            writers: vec![
                site("Button", Some("down"), false),
                site("Radio", Some("output"), false),
            ],
            readers: vec![site("Playlist", Some("trigger"), true)],
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
            crate::ControllerId::new("test.project"),
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
    fn single_writer_has_no_merge_summary_and_primary_has_role() {
        let mut ch = channel();
        ch.writers.truncate(1);
        ch.primary_visual = true;
        let aspects = ch.visible_aspects();
        assert!(aspects[1].rows.iter().all(|row| row.label != "Writers"));
        assert!(aspects[0].rows.iter().any(|row| row.label == "Role"));
    }
}
