//! Opaque handle into engine-managed runtime buffer storage.

/// Small, copyable identifier for a runtime buffer; suitable as a map key.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeBufferId(u32);

impl RuntimeBufferId {
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeBufferId;

    #[test]
    fn runtime_buffer_id_round_trips_raw() {
        let id = RuntimeBufferId::new(42);
        assert_eq!(id.as_u32(), 42);
        assert_eq!(RuntimeBufferId::new(id.as_u32()), id);
    }
}
