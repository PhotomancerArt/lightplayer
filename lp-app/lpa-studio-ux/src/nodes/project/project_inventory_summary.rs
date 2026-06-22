use lpc_wire::WireProjectInventoryReadResponse;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectInventorySummary {
    pub node_count: usize,
    pub definition_count: usize,
    pub asset_count: usize,
}

impl From<&WireProjectInventoryReadResponse> for ProjectInventorySummary {
    fn from(inventory: &WireProjectInventoryReadResponse) -> Self {
        Self {
            node_count: inventory.nodes.len(),
            definition_count: inventory.defs.len(),
            asset_count: inventory.assets.len(),
        }
    }
}
