use lpc_model::{SlotData, SlotPath, SlotShapeId, SlotShapeRegistrySnapshot};

#[derive(Clone)]
pub struct FullSync {
    pub registry: SlotShapeRegistrySnapshot,
    pub roots: Vec<(String, SlotShapeId, SlotData)>,
}

#[derive(Clone, Debug)]
pub struct SlotPatch {
    pub root: String,
    pub path: SlotPath,
    pub change: SlotChange,
}

#[derive(Clone, Debug)]
pub enum SlotChange {
    Replace(SlotData),
}
