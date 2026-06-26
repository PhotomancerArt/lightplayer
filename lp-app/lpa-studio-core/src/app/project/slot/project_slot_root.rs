use core::fmt;

/// Named root of a node-owned slot tree.
///
/// Current project sync uses roots such as `def` and `state`; `Other` keeps the
/// address model open for future or custom roots without turning roots back
/// into untyped strings everywhere.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProjectSlotRoot {
    Def,
    State,
    Other(String),
}

impl ProjectSlotRoot {
    /// The authored/configuration root.
    pub fn def() -> Self {
        Self::Def
    }

    /// The runtime state root.
    pub fn state() -> Self {
        Self::State
    }

    /// A non-standard or future root name.
    pub fn other(name: impl Into<String>) -> Self {
        Self::Other(name.into())
    }

    /// Human-readable root name.
    pub fn name(&self) -> &str {
        match self {
            Self::Def => "def",
            Self::State => "state",
            Self::Other(name) => name,
        }
    }
}

impl fmt::Display for ProjectSlotRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
