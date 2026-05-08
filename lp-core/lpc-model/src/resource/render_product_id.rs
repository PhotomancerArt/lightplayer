/// Small, copyable identifier for a render product; suitable as a map key.
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
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
    use crate::resource::render_product_id::RenderProductId;

    #[test]
    fn render_product_id_round_trips_raw() {
        let id = RenderProductId::new(42);
        assert_eq!(id.as_u32(), 42);
        assert_eq!(RenderProductId::new(id.as_u32()), id);
    }
}
