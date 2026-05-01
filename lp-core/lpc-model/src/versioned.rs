//! [`Versioned`]: a value with frame-stamped change tracking — the runtime
//! change-tracking primitive used by the spine.
//!
//! Server-side `*Props` structs (e.g. `TextureProps`) hold one `Versioned<T>`
//! per top-level field. Each `Versioned<T>` records the [`FrameId`] at which
//! its value last changed; the sync layer diffs against a client's
//! `since_frame` to emit minimal field-level deltas. `T` is the natural
//! Rust type per field (`Versioned<u32>`, `Versioned<Vec<u8>>`, …); reflection
//! over [`ModelValue`] is the wire / client view.

use crate::project::FrameId;

/// A value of type `T` plus the [`FrameId`] at which it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    value: T,
    version: FrameId,
}

impl<T> Versioned<T> {
    /// Create a new `Versioned` value with the given value and frame ID.
    pub fn new(frame_id: FrameId, value: T) -> Self {
        Self {
            value,
            version: frame_id,
        }
    }

    /// Get a reference to the value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the value.
    ///
    /// Note: this does NOT update the stored version. Use [`Self::set`] or
    /// [`Self::mark_updated`] if you want to track the change.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Set the value and update the stored version.
    pub fn set(&mut self, frame_id: FrameId, value: T) {
        self.value = value;
        self.version = frame_id;
    }

    /// Mark this `Versioned` value as updated without changing the value.
    ///
    /// Useful when the value was modified via [`Self::get_mut`] and you
    /// want to track the change.
    pub fn mark_updated(&mut self, frame_id: FrameId) {
        self.version = frame_id;
    }

    /// Get the frame ID stored as this value's version.
    pub fn changed_frame(&self) -> FrameId {
        self.version
    }

    /// Get a reference to the value (alias for [`Self::get`]).
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consume the `Versioned` value and return the inner value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: PartialEq> PartialEq<T> for Versioned<T> {
    fn eq(&self, other: &T) -> bool {
        &self.value == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prop_new() {
        let field = Versioned::new(FrameId::new(10), 42);
        assert_eq!(field.get(), &42);
        assert_eq!(field.changed_frame(), FrameId::new(10));
    }

    #[test]
    fn test_prop_set() {
        let mut field = Versioned::new(FrameId::new(5), 10);
        field.set(FrameId::new(20), 30);
        assert_eq!(field.get(), &30);
        assert_eq!(field.changed_frame(), FrameId::new(20));
    }

    #[test]
    fn test_prop_mark_updated() {
        let mut field = Versioned::new(FrameId::new(5), 10);
        *field.get_mut() = 20;
        field.mark_updated(FrameId::new(15));
        assert_eq!(field.get(), &20);
        assert_eq!(field.changed_frame(), FrameId::new(15));
    }

    #[test]
    fn test_prop_into_value() {
        let field = Versioned::new(FrameId::new(5), 42);
        let value = field.into_value();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_prop_value_alias() {
        let field = Versioned::new(FrameId::new(5), 42);
        assert_eq!(field.value(), &42);
        assert_eq!(field.get(), field.value());
    }

    #[test]
    fn test_prop_partial_eq_with_value() {
        let field = Versioned::new(FrameId::new(5), 42);
        assert_eq!(field, 42);
        assert_ne!(field, 10);
    }
}
