/// Monotonic change marker for synchronized state.
///
/// `Revision` answers one question: "at what shared sync revision did this
/// observable data last change?" It is intentionally distinct from schema,
/// protocol, or file-format versions.
///
/// Runtime orchestration advances the current revision as synchronized state
/// changes. Individual slot values, containers, and shape registry entries store
/// the revision at which they were last updated.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct Revision(pub i64);

impl Revision {
    /// Create a revision from its raw monotonic value.
    pub fn new(id: i64) -> Self {
        Self(id)
    }

    /// Return the raw monotonic revision value.
    pub fn as_i64(self) -> i64 {
        self.0
    }

    /// Return the next revision after this one.
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl Default for Revision {
    fn default() -> Self {
        Self(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_exposes_raw_value() {
        let revision = Revision::new(42);
        assert_eq!(revision.as_i64(), 42);
    }

    #[test]
    fn next_advances_revision() {
        let revision = Revision::new(10);
        let next = revision.next();
        assert_eq!(next.as_i64(), 11);
    }

    #[test]
    fn default_revision_is_zero() {
        let revision = Revision::default();
        assert_eq!(revision.as_i64(), 0);
    }
}
