//! Bus channels: the wiring between nodes, as the product has it.
//!
//! In LightPlayer a node never names the node it feeds. It writes a
//! product to a named channel on its module's bus (a shader writes its
//! picture to `visual.out`), and another node reads that channel (a
//! fixture reads `visual.out`, and writes its lamps to `control.out`,
//! which an output reads). On disk that is a binding on the node:
//! `"output": {"target": "bus:visual.out"}`, `"input": {"source":
//! "bus:control.out"}` (`lpc-model`'s `BindingDefs`). The lab follows that
//! closely (D52, D55): every edge between two nodes on the canvas is one
//! of these channels, and says its name (D53).
//!
//! The names are the product's: `visual.*` carries a render product,
//! `control.*` a control product. A channel lives in a module: a read
//! looks outward, to the nearest module up that has a writer, and a write
//! stays where it is (fit-check §6).

/// A channel's name on its bus: `visual.out`, `control.out`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Channel(pub String);

impl Channel {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Where a shader writes its picture and a fixture reads it, unless
    /// they say otherwise.
    pub fn visual_out() -> Self {
        Self::new("visual.out")
    }

    /// Where a fixture writes its lamps and an output reads them, unless
    /// they say otherwise.
    pub fn control_out() -> Self {
        Self::new("control.out")
    }

    /// A control channel of an output's own, from its name: `scarf out` →
    /// `control.scarf_out`. The product's idiom for a second output in one
    /// module (fit-check §6).
    pub fn control_for(name: &str) -> Self {
        let word: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        Self::new(format!("control.{word}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The binding's form on disk: `bus:control.out`.
    pub fn binding(&self) -> String {
        format!("bus:{}", self.0)
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
