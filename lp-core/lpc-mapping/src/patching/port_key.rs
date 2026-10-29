//! A port's key: one data line of an output, keyed by a number. "The next
//! port" is the next key in order; keys have gaps after deletions.

/// A port's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortKey(pub u32);

impl core::fmt::Display for PortKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}
