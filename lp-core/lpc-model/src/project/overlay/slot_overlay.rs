use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::{NodeDef, SlotPath, SlotPathSegment};

use super::{SlotEdit, SlotEditOp};

/// Canonical pending slot edits for one authored artifact.
///
/// A slot overlay keeps only the latest meaningful intent for each path. When a
/// structural edit makes descendant edits stale, those descendants are removed
/// so the overlay can be applied deterministically.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SlotOverlay {
    /// Pending slot operations keyed by target slot path.
    pub edits: BTreeMap<SlotPath, SlotEditOp>,
}

impl SlotOverlay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    pub fn contains_path(&self, path: &SlotPath) -> bool {
        self.edits.contains_key(path)
    }

    pub fn put_edit(&mut self, edit: SlotEdit) -> bool {
        let before = self.clone();
        let target = edit.path.clone();
        let clear_scopes = structural_clear_scopes(&edit);
        let clears_ancestor_remove = matches!(
            edit.op,
            SlotEditOp::EnsurePresent | SlotEditOp::AssignValue(_)
        );

        self.edits.retain(|existing_path, existing_op| {
            if existing_path == &target {
                return false;
            }
            if clear_scopes
                .iter()
                .any(|scope| is_strict_ancestor(scope, existing_path))
            {
                return false;
            }
            if clears_ancestor_remove
                && matches!(existing_op, SlotEditOp::Remove)
                && is_strict_ancestor(existing_path, &target)
            {
                return false;
            }
            true
        });

        if matches!(edit.op, SlotEditOp::Remove)
            && self.edits.iter().any(|(existing_path, existing_op)| {
                matches!(existing_op, SlotEditOp::Remove)
                    && is_strict_ancestor(existing_path, &target)
            })
        {
            return *self != before;
        }

        self.edits.insert(target, edit.op);
        *self != before
    }

    pub fn remove_edit(&mut self, path: &SlotPath) -> bool {
        self.edits.remove(path).is_some()
    }

    pub fn to_apply_plan(&self) -> Vec<SlotEdit> {
        let mut edits: Vec<_> = self
            .edits
            .iter()
            .map(|(path, op)| SlotEdit {
                path: path.clone(),
                op: op.clone(),
            })
            .collect();
        edits.sort_by(|left, right| apply_order_key(left).cmp(&apply_order_key(right)));
        edits
    }
}

fn apply_order_key(edit: &SlotEdit) -> (u8, usize, &SlotPath) {
    let op_order = match edit.op {
        SlotEditOp::EnsurePresent => 0,
        SlotEditOp::AssignValue(_) => 1,
        SlotEditOp::Remove => 2,
    };
    (op_order, edit.path.segments().len(), &edit.path)
}

fn structural_clear_scopes(edit: &SlotEdit) -> Vec<SlotPath> {
    match &edit.op {
        SlotEditOp::Remove => alloc::vec![edit.path.clone()],
        SlotEditOp::EnsurePresent => {
            let mut scopes = alloc::vec![edit.path.clone()];
            if ensure_present_clears_parent_scope(&edit.path) {
                scopes.push(parent_path(&edit.path));
            }
            scopes
        }
        SlotEditOp::AssignValue(_) => Vec::new(),
    }
}

fn ensure_present_clears_parent_scope(path: &SlotPath) -> bool {
    match path.segments() {
        [SlotPathSegment::Field(name)] => NodeDef::is_variant_name(name.as_str()),
        [.., SlotPathSegment::Field(_)] => true,
        _ => false,
    }
}

fn parent_path(path: &SlotPath) -> SlotPath {
    let Some((_, parent)) = path.segments().split_last() else {
        return SlotPath::root();
    };
    SlotPath::from_segments(parent.to_vec())
}

fn is_strict_ancestor(ancestor: &SlotPath, descendant: &SlotPath) -> bool {
    let ancestor = ancestor.segments();
    let descendant = descendant.segments();
    ancestor.len() < descendant.len() && descendant.starts_with(ancestor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LpValue;

    #[test]
    fn same_path_upserts_to_latest_intent() {
        let mut overlay = SlotOverlay::new();
        let path = SlotPath::parse("controls.rate").unwrap();

        assert!(overlay.put_edit(SlotEdit::assign_value(path.clone(), LpValue::F32(1.0))));
        assert!(overlay.put_edit(SlotEdit::assign_value(path.clone(), LpValue::F32(2.0))));

        assert_eq!(overlay.edits.len(), 1);
        assert_eq!(
            overlay.edits.get(&path),
            Some(&SlotEditOp::AssignValue(LpValue::F32(2.0)))
        );
    }

    #[test]
    fn parent_remove_clears_pending_descendants() {
        let mut overlay = SlotOverlay::new();
        overlay.put_edit(SlotEdit::assign_value(
            SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            LpValue::F32(2.0),
        ));
        overlay.put_edit(SlotEdit::remove(
            SlotPath::parse("entries[0].node").unwrap(),
        ));

        assert_eq!(overlay.edits.len(), 1);
        assert_eq!(
            overlay
                .edits
                .get(&SlotPath::parse("entries[0].node").unwrap()),
            Some(&SlotEditOp::Remove)
        );
    }

    #[test]
    fn descendant_assign_clears_ancestor_remove() {
        let mut overlay = SlotOverlay::new();
        overlay.put_edit(SlotEdit::remove(
            SlotPath::parse("entries[0].node").unwrap(),
        ));
        overlay.put_edit(SlotEdit::assign_value(
            SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            LpValue::F32(2.0),
        ));

        assert_eq!(overlay.edits.len(), 1);
        assert!(
            overlay
                .edits
                .contains_key(&SlotPath::parse("entries[0].node.controls.rate").unwrap())
        );
    }

    #[test]
    fn structural_ensure_clears_stale_descendants() {
        let mut overlay = SlotOverlay::new();
        overlay.put_edit(SlotEdit::assign_value(
            SlotPath::parse("entries[0].node.controls.rate").unwrap(),
            LpValue::F32(2.0),
        ));
        overlay.put_edit(SlotEdit::ensure_present(
            SlotPath::parse("entries[0].node.Shader").unwrap(),
        ));

        assert_eq!(overlay.edits.len(), 1);
        assert!(
            overlay
                .edits
                .contains_key(&SlotPath::parse("entries[0].node.Shader").unwrap())
        );
    }

    #[test]
    fn apply_plan_places_structural_ensures_before_assignments() {
        let mut overlay = SlotOverlay::new();
        overlay.put_edit(SlotEdit::ensure_present(
            SlotPath::parse("entries[0].node.Shader").unwrap(),
        ));
        overlay.put_edit(SlotEdit::assign_value(
            SlotPath::parse("entries[0].node.source.path").unwrap(),
            LpValue::String(alloc::string::String::from("./shader.glsl")),
        ));

        let plan = overlay.to_apply_plan();

        assert!(matches!(plan[0].op, SlotEditOp::EnsurePresent));
        assert!(matches!(plan[1].op, SlotEditOp::AssignValue(_)));
    }
}
