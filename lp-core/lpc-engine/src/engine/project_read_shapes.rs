//! Shape registry project read helpers.

use lpc_wire::{ReadLevel, ShapeReadQuery, ShapeReadResult};

use super::Engine;

impl Engine {
    pub(super) fn read_project_shapes(&self, query: ShapeReadQuery) -> ShapeReadResult {
        let (registry, complete, next) = match query.level {
            ReadLevel::Ids | ReadLevel::Summary | ReadLevel::Detail => {
                if let Some(limit) = query.limit {
                    let (snapshot, next) = self.slot_shapes().snapshot_page_with_static_catalog(
                        query.after,
                        usize::try_from(limit).unwrap_or(usize::MAX),
                    );
                    (Some(snapshot), next.is_none(), next)
                } else {
                    let (snapshot, next) = self
                        .slot_shapes()
                        .snapshot_page_with_static_catalog(query.after, 64);
                    (Some(snapshot), next.is_none(), next)
                }
            }
        };
        ShapeReadResult {
            level: query.level,
            registry,
            complete,
            next,
        }
    }
}
