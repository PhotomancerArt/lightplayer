/// Common metadata for all hardware drivers.
///
/// Family-specific traits such as [`crate::Ws281xDriver`] and
/// [`crate::ButtonDriver`] extend this trait with endpoint discovery and open
/// operations.
pub trait HwDriver {
    /// Stable driver identifier used in endpoint IDs and resource claims.
    fn driver_id(&self) -> &str;

    /// Human-facing label for diagnostics and endpoint lists.
    fn display_label(&self) -> &str {
        self.driver_id()
    }
}
