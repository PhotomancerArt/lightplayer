//! The repeats round each part, innermost first: what each place in a node
//! id's copies (`z@2@3`) counts.
//!
//! A node's id lists the copy it sits in for every repeat round its part,
//! innermost first, trailing zeros dropped (F5). So the k-th number in
//! `z@2@3` is a copy of the k-th repeat round `z`. Edits that add, remove or
//! dissolve a repeat change that list, and these helpers rewrite override
//! keys through it.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::part::{Mapping, Part, PartId, PartKind, TreeId};
use crate::patching::Override;

/// Every part's repeats round it, innermost first.
pub(crate) type CopyLevels = BTreeMap<PartId, Vec<PartId>>;

/// The repeats round every part of `parts`, innermost first. A repeat is
/// round the parts it holds, not round itself.
pub(crate) fn copy_levels(parts: &[Part]) -> CopyLevels {
    let mut levels = BTreeMap::new();
    let mut around = Vec::new();
    for part in parts {
        visit(part, &mut around, &mut levels);
    }
    levels
}

/// `around` is outermost first while walking.
fn visit(part: &Part, around: &mut Vec<PartId>, levels: &mut CopyLevels) {
    levels.insert(part.id.clone(), around.iter().rev().cloned().collect());
    let repeat = matches!(part.kind, PartKind::Repeat(_));
    if repeat {
        around.push(part.id.clone());
    }
    for child in part.parts() {
        visit(child, around, levels);
    }
    if repeat {
        around.pop();
    }
}

/// Rekey every override `rekey` gives a new id; the others stay as they
/// are. The new keys must not meet (each edit's rekeying is one to one).
pub(crate) fn rekey_overrides(m: &mut Mapping, rekey: impl Fn(&TreeId) -> Option<TreeId>) {
    let old = core::mem::take(&mut m.overrides);
    let mut new: BTreeMap<TreeId, Override> = BTreeMap::new();
    for (id, entry) in old {
        let id = rekey(&id).unwrap_or(id);
        let clash = new.insert(id, entry);
        debug_assert!(
            clash.is_none(),
            "an edit rekeyed two overrides onto one node"
        );
    }
    m.overrides = new;
}
