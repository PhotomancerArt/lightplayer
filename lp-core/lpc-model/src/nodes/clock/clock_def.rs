use serde::{Deserialize, Serialize};

use alloc::string::String;

use crate::{BindingDefs, ClockControls};

/// Authored clock node definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, lpc_slot_macros::SlotRecord)]
#[slot(root, view)]
pub struct ClockDef {
    #[slot(skip)]
    pub kind: String,

    /// Authored slot bindings for clock outputs.
    #[serde(default, skip_serializing_if = "BindingDefs::is_empty")]
    pub bindings: BindingDefs,

    /// Runtime clock controls.
    #[serde(default)]
    pub controls: ClockControls,
}

impl Default for ClockDef {
    fn default() -> Self {
        Self {
            kind: String::from(Self::KIND),
            bindings: BindingDefs::default(),
            controls: ClockControls::default(),
        }
    }
}

impl ClockDef {
    pub const KIND: &'static str = "clock";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClockDefView, NodeDef, SlotPath, SlotShapeRegistry, StaticSlotShape};

    #[test]
    fn clock_def_parses_minimal_inline_node() {
        let def = NodeDef::from_toml_str(
            r#"
kind = "clock"
"#,
        )
        .expect("clock def");

        let NodeDef::Clock(def) = def else {
            panic!("clock def");
        };
        assert_eq!(def.kind, "clock");
        assert!(*def.controls.running.value());
        assert_eq!(*def.controls.rate.value(), 1.0);
    }

    #[test]
    fn generated_clock_def_view_compiles() {
        let mut registry = SlotShapeRegistry::default();
        ClockDef::ensure_registered(&mut registry).expect("clock shape");

        let view = ClockDefView::compile(&registry).expect("clock def view");

        assert_eq!(view.registry_revision(), registry.revision());
        assert_eq!(
            view.bindings().path(),
            &SlotPath::parse("bindings").unwrap()
        );
        assert_eq!(
            view.controls().path(),
            &SlotPath::parse("controls").unwrap()
        );
    }
}
