//! Promoted controls: a project's curated public knobs
//! (effects-are-projects ADR, `docs/adr/2026-07-28-effects-are-projects.md`).

use alloc::string::String;

use crate::{BindingRef, OptionSlot, Slotted, ValueSlot};

/// One promoted control on a project node: an **alias** to a slot on a
/// direct child, plus optional display overrides.
///
/// An alias carries **no value** — values live on the target slot, so
/// overlay dirty state, transient edits, and binding state all observe the
/// one real slot. (This deliberately differs from
/// [`crate::nodes::shader::ShaderSlotDef`], which owns defaults; an alias
/// with its own default would be a second source of truth.)
#[derive(Clone, Debug, PartialEq, Slotted)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct PromotedControlDef {
    /// Alias target: a node-slot ref relative to this project node, e.g.
    /// `node:./sim#speed`. Must be the node form ([`BindingRef::Node`])
    /// and resolve to a **direct child** of the project node — the loader
    /// rejects bus/unset forms and deeper or unresolvable targets with a
    /// path-qualified error.
    pub target: ValueSlot<BindingRef>,
    /// Display label override; the target slot's own label applies when
    /// absent.
    pub label: OptionSlot<ValueSlot<String>>,
    /// Display unit override (e.g. `"Hz"`).
    pub unit: OptionSlot<ValueSlot<String>>,
    /// Control-range override for the rendered widget.
    pub min: OptionSlot<ValueSlot<f32>>,
    /// Control-range override for the rendered widget.
    pub max: OptionSlot<ValueSlot<f32>>,
}

impl Default for PromotedControlDef {
    fn default() -> Self {
        Self::to_target(BindingRef::Unset)
    }
}

impl PromotedControlDef {
    /// A bare alias to `target` with no display overrides.
    pub fn to_target(target: BindingRef) -> Self {
        Self {
            target: ValueSlot::new(target),
            label: OptionSlot::none(),
            unit: OptionSlot::none(),
            min: OptionSlot::none(),
            max: OptionSlot::none(),
        }
    }
}
