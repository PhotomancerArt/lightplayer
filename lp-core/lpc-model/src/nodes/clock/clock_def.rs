use crate::{BindingDefs, ClockControls, Slotted};

/// Authored clock node definition.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
pub struct ClockDef {
    /// Authored slot bindings for clock outputs.
    pub bindings: BindingDefs,

    /// Runtime clock controls.
    pub controls: ClockControls,
}

impl ClockDef {
    pub const KIND: &'static str = "clock";

    pub fn kind(&self) -> crate::NodeKind {
        crate::NodeKind::Clock
    }
}

#[cfg(test)]
mod tests {
    use crate::{ClockDefView, NodeDef, SlotPath, SlotShapeRegistry};

    #[test]
    fn clock_def_parses_minimal_inline_node() {
        let def = NodeDef::from_json_str(r#"{ "kind": "Clock" }"#).expect("clock def");

        let NodeDef::Clock(def) = def else {
            panic!("clock def");
        };
        assert!(*def.controls.running.value());
        assert_eq!(*def.controls.rate.value(), 1.0);
    }

    #[test]
    fn generated_clock_def_view_compiles() {
        let registry = SlotShapeRegistry::default();

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
