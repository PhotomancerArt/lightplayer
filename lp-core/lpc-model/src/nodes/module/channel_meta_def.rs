//! Per-channel authored meta overrides: the module author's curation
//! escape hatch (modules.md R9 / Q1 lean).
//!
//! Control meta normally derives from the currently-bound slots
//! (widest-range merge, panel.md P6); an entry here overrides the derived
//! presentation for one channel of the module's scope. The vocabulary
//! survives from the superseded spike's `PromotedControlDef` display
//! fields — aliasing itself is dead (binding is publicity, R3), so the
//! map key IS the channel name and no target exists.

use alloc::string::String;

use crate::{OptionSlot, Slotted, ValueSlot};

/// Display overrides for one channel of the module's scope. Absent fields
/// keep the meta derived from bound slots.
#[derive(Clone, Debug, Default, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ChannelMetaDef {
    /// Display label override; the derived label applies when absent.
    pub label: OptionSlot<ValueSlot<String>>,
    /// Display unit override (e.g. `"Hz"`).
    pub unit: OptionSlot<ValueSlot<String>>,
    /// Control-range override for the rendered widget.
    pub min: OptionSlot<ValueSlot<f32>>,
    /// Control-range override for the rendered widget.
    pub max: OptionSlot<ValueSlot<f32>>,
}
