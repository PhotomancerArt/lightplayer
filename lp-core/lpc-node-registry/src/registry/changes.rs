//! Definition change classification.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::ArtifactLocation;

use super::sync_result::DefChangeDetail;
use super::{NodeDefEntry, NodeDefLoc, NodeDefState, NodeDefUpdates};

pub(crate) fn state_changed(before: &NodeDefState, after: &NodeDefState) -> bool {
    match (before, after) {
        (NodeDefState::Loaded(b), NodeDefState::Loaded(a)) => {
            if b.invocation_sites(&lpc_model::SlotPath::root()).is_empty() {
                lpc_model::NodeDef::body_changed(b, a)
            } else {
                lpc_model::NodeDef::shell_changed(b, a)
            }
        }
        _ => before != after,
    }
}

pub(crate) fn build_change_details(
    before: &BTreeMap<NodeDefLoc, NodeDefState>,
    updates: &NodeDefUpdates,
    entries: &BTreeMap<NodeDefLoc, NodeDefEntry>,
) -> Vec<(NodeDefLoc, DefChangeDetail)> {
    updates
        .changed
        .iter()
        .filter_map(|loc| {
            let before_state = before.get(loc)?;
            let after_state = entries.get(loc).map(|entry| &entry.state)?;
            Some((loc.clone(), classify_def_change(before_state, after_state)))
        })
        .collect()
}

fn classify_def_change(before: &NodeDefState, after: &NodeDefState) -> DefChangeDetail {
    match (before, after) {
        (_, NodeDefState::ParseError(_)) if !matches!(before, NodeDefState::ParseError(_)) => {
            DefChangeDetail::EnteredError
        }
        (NodeDefState::ParseError(_), NodeDefState::Loaded(_)) => DefChangeDetail::LeftError,
        (NodeDefState::Loaded(b), NodeDefState::Loaded(a)) if b.kind() != a.kind() => {
            DefChangeDetail::KindChanged {
                from: b.kind(),
                to: a.kind(),
            }
        }
        _ => DefChangeDetail::Content,
    }
}

pub(crate) fn dedupe_locations(locations: &mut Vec<ArtifactLocation>) {
    locations.sort_unstable();
    locations.dedup();
}
