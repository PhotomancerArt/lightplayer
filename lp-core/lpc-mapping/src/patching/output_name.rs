//! An output's name, as a start names it (`main`, `out`).
//!
//! Outputs have no order: patching never runs "into the next output".

use alloc::string::String;

/// The name of an output node.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OutputName(String);

impl OutputName {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self(String::from(name))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for OutputName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}
