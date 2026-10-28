//! Presentation B of a node's patching rows (lean Q82): the start the flow
//! gives the node, as one quiet line ("circle1's flow: port D2, from its
//! first lamp"), then only the fields set here, each with Back to the
//! flow, then a way to set one more here.
//!
//! The same facts as presentation A (`patch_section.rs`, the default):
//! A's rows, kept where they are set here. G3 picks one, and the fix-up
//! deletes the other (Q98): if A wins, this file goes.

use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::ux::editor::Editor;
use crate::ux::patch_section::PatchField;
use crate::ux::ui::{UiField, UiItem, UiSource};

impl Editor {
    /// B's rows, from A's fields.
    pub(crate) fn presentation_b(
        &self,
        objects: &[(NodeId, TreeRef)],
        fields: Vec<UiField>,
    ) -> Vec<UiItem> {
        let p = &self.project;
        let mut items = Vec::new();
        if let [(_, node)] = objects
            && let Some(line) = p.flow_start_line(node)
        {
            items.push(UiItem::Hint(line));
        }
        let mut not_here = Vec::new();
        for field in fields {
            if field.source == Some(UiSource::SetHere) {
                items.push(UiItem::Field(field));
            } else if let Some(word) = set_here_word(field.field) {
                not_here.push((word.to_string(), field.field));
            }
        }
        if !not_here.is_empty() {
            items.push(UiItem::SetHere { options: not_here });
        }
        items
    }
}

/// What B's "Set here…" list calls a field; `None` for one set through
/// another (the Port row sets the output).
fn set_here_word(field: PatchField) -> Option<&'static str> {
    Some(match field {
        PatchField::Port => "Port",
        PatchField::Pixel => "Starts at",
        PatchField::Direction => "Direction",
        PatchField::Offset => "Offset",
        PatchField::OnWire => "Off",
        PatchField::Output | PatchField::Start | PatchField::StartPixel | PatchField::Children => {
            return None;
        }
    })
}
