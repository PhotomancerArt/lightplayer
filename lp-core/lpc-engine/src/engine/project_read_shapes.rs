//! Shape registry project read helpers.

use lpc_wire::{ReadLevel, ShapeReadQuery, ShapeReadResult};

use super::Engine;

impl Engine {
    pub(super) fn read_project_shapes(&self, query: ShapeReadQuery) -> ShapeReadResult {
        let registry = match query.level {
            ReadLevel::Ids | ReadLevel::Summary | ReadLevel::Detail => Some(
                self.slot_shapes()
                    .snapshot_page_with_static_catalog(None, usize::MAX)
                    .0,
            ),
        };
        ShapeReadResult {
            level: query.level,
            registry,
        }
    }
}
