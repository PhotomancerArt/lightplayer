//! [`PropValue`]: a value with frame-stamped change tracking — the runtime
//! change-tracking primitive used by the spine.
//!
//! Server-side `*Props` structs (e.g. `TextureProps`) hold one `Prop<T>`
//! per top-level field. Each `Prop<T>` records the [`FrameId`] at which
//! its value last changed; the sync layer diffs against a client's
//! `since_frame` to emit minimal field-level deltas. `T` is the natural
//! Rust type per field (`Prop<u32>`, `Prop<Vec<u8>>`, …); reflection
//! over [`WireValue`] is the wire / client view.

use crate::project::FrameId;

/// A value of type `T` plus the [`FrameId`] at which it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropValue<T> {
    value: T,
    changed_frame: FrameId,
}

impl<T> PropValue<T> {
    /// Create a new `Prop` with the given value and frame ID.
    pub fn new(frame_id: FrameId, value: T) -> Self {
        Self {
            value,
            changed_frame: frame_id,
        }
    }

    /// Get a reference to the value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the value.
    ///
    /// Note: this does NOT update the changed frame. Use [`Self::set`] or
    /// [`Self::mark_updated`] if you want to track the change.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Set the value and update the changed frame.
    pub fn set(&mut self, frame_id: FrameId, value: T) {
        self.value = value;
        self.changed_frame = frame_id;
    }

    /// Mark this `Prop` as updated without changing the value.
    ///
    /// Useful when the value was modified via [`Self::get_mut`] and you
    /// want to track the change.
    pub fn mark_updated(&mut self, frame_id: FrameId) {
        self.changed_frame = frame_id;
    }

    /// Get the frame ID when this `Prop` was last changed.
    pub fn changed_frame(&self) -> FrameId {
        self.changed_frame
    }

    /// Get a reference to the value (alias for [`Self::get`]).
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consume the `Prop` and return the value.
    pub fn into_value(self) -> T {
        self.value
    }
}

impl<T: PartialEq> PartialEq<T> for PropValue<T> {
    fn eq(&self, other: &T) -> bool {
        &self.value == other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prop_new() {
        let field = PropValue::new(FrameId::new(10), 42);
        assert_eq!(field.get(), &42);
        assert_eq!(field.changed_frame(), FrameId::new(10));
    }

    #[test]
    fn test_prop_set() {
        let mut field = PropValue::new(FrameId::new(5), 10);
        field.set(FrameId::new(20), 30);
        assert_eq!(field.get(), &30);
        assert_eq!(field.changed_frame(), FrameId::new(20));
    }

    #[test]
    fn test_prop_mark_updated() {
        let mut field = PropValue::new(FrameId::new(5), 10);
        *field.get_mut() = 20;
        field.mark_updated(FrameId::new(15));
        assert_eq!(field.get(), &20);
        assert_eq!(field.changed_frame(), FrameId::new(15));
    }

    #[test]
    fn test_prop_into_value() {
        let field = PropValue::new(FrameId::new(5), 42);
        let value = field.into_value();
        assert_eq!(value, 42);
    }

    #[test]
    fn test_prop_value_alias() {
        let field = PropValue::new(FrameId::new(5), 42);
        assert_eq!(field.value(), &42);
        assert_eq!(field.get(), field.value());
    }

    #[test]
    fn test_prop_partial_eq_with_value() {
        let field = PropValue::new(FrameId::new(5), 42);
        assert_eq!(field, 42);
        assert_ne!(field, 10);
    }
}
