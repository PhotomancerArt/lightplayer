//! Opaque runtime identity for a binding registry entry.

use core::fmt;

/// Stable handle for one binding row in the engine binding registry.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingId(pub u32);

impl BindingId {
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

impl fmt::Display for BindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::BindingId;

    #[test]
    fn binding_id_orders_by_raw() {
        assert!(BindingId(1) < BindingId(2));
        assert_eq!(BindingId(7), BindingId(7));
    }
}
