//! Opaque handle into engine-managed render-product storage.

/// Small, copyable identifier for a render product; suitable as a map key.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderProductId(u32);

impl RenderProductId {
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
    use super::RenderProductId;

    #[test]
    fn render_product_id_round_trips_raw() {
        let id = RenderProductId::new(42);
        assert_eq!(id.as_u32(), 42);
    }
}
