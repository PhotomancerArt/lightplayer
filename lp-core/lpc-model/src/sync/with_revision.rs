//! [`WithRevision`]: a value paired with the sync revision at which it last
//! changed.
//!
//! This is the small wrapper used when a logical value must carry its own
//! change marker. Slot leaves use it to say "this whole value changed at
//! revision N"; slot containers use separate revision fields for structural
//! changes such as map keys, enum variants, or option presence.

use crate::Revision;

/// A value of type `T` plus the [`Revision`] at which it last changed.
///
/// `WithRevision<T>` does not decide when revisions advance. Callers pass the
/// revision explicitly, usually by reading [`crate::current_revision`] during a
/// mutation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct WithRevision<T> {
    value: T,
    changed_at: Revision,
}

impl<T> WithRevision<T> {
    /// Create a value with an explicit change revision.
    pub fn new(revision: Revision, value: T) -> Self {
        Self {
            value,
            changed_at: revision,
        }
    }

    /// Get a reference to the value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the value.
    ///
    /// This does not update the stored revision. Use [`Self::set`] or
    /// [`Self::mark_updated`] after mutating when the change should be visible
    /// to sync.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Set the value and update its change revision.
    pub fn set(&mut self, revision: Revision, value: T) {
        self.value = value;
        self.changed_at = revision;
    }

    /// Mark this value as changed without replacing the value.
    ///
    /// Useful when the value was modified via [`Self::get_mut`] and you
    /// want to track the change.
    pub fn mark_updated(&mut self, revision: Revision) {
        self.changed_at = revision;
    }

    /// Return the revision at which this value last changed.
    pub fn changed_at(&self) -> Revision {
        self.changed_at
    }

    /// Get a mutable reference to the revision marker.
    pub fn changed_at_mut(&mut self) -> &mut Revision {
        &mut self.changed_at
    }

    /// Get a reference to the value (alias for [`Self::get`]).
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consume the wrapper and return the inner value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: PartialEq> PartialEq<T> for WithRevision<T> {
    fn eq(&self, other: &T) -> bool {
        &self.value == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_value_and_revision() {
        let field = WithRevision::new(Revision::new(10), 42);
        assert_eq!(field.get(), &42);
        assert_eq!(field.changed_at(), Revision::new(10));
    }

    #[test]
    fn set_replaces_value_and_revision() {
        let mut field = WithRevision::new(Revision::new(5), 10);
        field.set(Revision::new(20), 30);
        assert_eq!(field.get(), &30);
        assert_eq!(field.changed_at(), Revision::new(20));
    }

    #[test]
    fn mark_updated_changes_revision_only() {
        let mut field = WithRevision::new(Revision::new(5), 10);
        *field.get_mut() = 20;
        field.mark_updated(Revision::new(15));
        assert_eq!(field.get(), &20);
        assert_eq!(field.changed_at(), Revision::new(15));
    }

    #[test]
    fn changed_at_mut_updates_revision_marker() {
        let mut field = WithRevision::new(Revision::new(5), 10);
        *field.changed_at_mut() = Revision::new(15);
        assert_eq!(field.changed_at(), Revision::new(15));
    }

    #[test]
    fn into_value_returns_inner_value() {
        let field = WithRevision::new(Revision::new(5), 42);
        let value = field.into_value();
        assert_eq!(value, 42);
    }

    #[test]
    fn value_alias_matches_get() {
        let field = WithRevision::new(Revision::new(5), 42);
        assert_eq!(field.value(), &42);
        assert_eq!(field.get(), field.value());
    }

    #[test]
    fn partial_eq_compares_inner_value() {
        let field = WithRevision::new(Revision::new(5), 42);
        assert_eq!(field, 42);
        assert_ne!(field, 10);
    }
}
