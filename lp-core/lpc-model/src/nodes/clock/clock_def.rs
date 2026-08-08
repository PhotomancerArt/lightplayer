use crate::{BindingDefs, ClockTransport, Slotted};

/// Authored clock node definition.
#[derive(Debug, Clone, Default, PartialEq, Slotted)]
pub struct ClockDef {
    /// Authored slot bindings for clock outputs.
    pub bindings: BindingDefs,

    /// Transient transport: play state, rate, scrub offset.
    ///
    /// `panel = "show"` sits on the RECORD, never on the leaves: grouping is
    /// model-declared (P6), and a promoted record whose named shape maps to
    /// a widget yields ONE panel control whose wires are its leaves'
    /// `default_bind` channels. A `Show` that promotes no leaf binding is a
    /// declaration bug — `shape_guardrails.rs` fails CI on it.
    #[slot(panel = "show")]
    pub transport: ClockTransport,
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
        assert_eq!(*def.transport.play_state.value(), crate::PlayState::Playing);
        assert_eq!(*def.transport.rate.value(), 1.0);
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
            view.transport().path(),
            &SlotPath::parse("transport").unwrap()
        );
    }
}
