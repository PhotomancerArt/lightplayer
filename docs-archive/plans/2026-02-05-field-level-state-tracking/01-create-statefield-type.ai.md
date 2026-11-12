# Phase 1: Create StateField<T> Type and Basic Implementation

## Scope of Phase

Create the `StateField<T>` wrapper type that tracks values and their change frames. This is the foundation for all field-level state tracking.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create `lp-model/src/state/mod.rs`

Create a new module for state-related types:

```rust
pub mod state_field;

pub use state_field::StateField;
```

### 2. Create `lp-model/src/state/state_field.rs`

Implement `StateField<T>`:

```rust
use crate::project::FrameId;

/// A field in node state that tracks when it was last changed
///
/// Used to implement field-level change tracking for efficient state synchronization.
/// Each field stores its value and the frame ID when it was last modified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateField<T> {
    value: T,
    changed_frame: FrameId,
}

impl<T> StateField<T> {
    /// Create a new StateField with the given value and frame ID
    pub fn new(frame_id: FrameId, value: T) -> Self {
        Self {
            value,
            changed_frame: frame_id,
        }
    }

    /// Get a reference to the value
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the value
    ///
    /// Note: This does NOT update the changed_frame. Use `set()` or `mark_updated()`
    /// if you want to track the change.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Set the value and update the changed frame
    pub fn set(&mut self, frame_id: FrameId, value: T) {
        self.value = value;
        self.changed_frame = frame_id;
    }

    /// Mark this field as updated without changing the value
    ///
    /// Useful when the value was modified via `get_mut()` and you want to track the change.
    pub fn mark_updated(&mut self, frame_id: FrameId) {
        self.changed_frame = frame_id;
    }

    /// Get the frame ID when this field was last changed
    pub fn changed_frame(&self) -> FrameId {
        self.changed_frame
    }

    /// Get a reference to the value (alias for `get()`)
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Consume the StateField and return the value
    pub fn into_value(self) -> T {
        self.value
    }
}

// Implement common traits for StateField
impl<T: PartialEq> PartialEq<T> for StateField<T> {
    fn eq(&self, other: &T) -> bool {
        &self.value == other
    }
}
```

### 3. Update `lp-model/src/lib.rs` or appropriate module file

Add the state module:

```rust
pub mod state;
```

### 4. Add tests

Add tests to `state_field.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_field_new() {
        let field = StateField::new(FrameId::new(10), 42);
        assert_eq!(field.get(), &42);
        assert_eq!(field.changed_frame(), FrameId::new(10));
    }

    #[test]
    fn test_state_field_set() {
        let mut field = StateField::new(FrameId::new(5), 10);
        field.set(FrameId::new(20), 30);
        assert_eq!(field.get(), &30);
        assert_eq!(field.changed_frame(), FrameId::new(20));
    }

    #[test]
    fn test_state_field_mark_updated() {
        let mut field = StateField::new(FrameId::new(5), 10);
        *field.get_mut() = 20;
        field.mark_updated(FrameId::new(15));
        assert_eq!(field.get(), &20);
        assert_eq!(field.changed_frame(), FrameId::new(15));
    }

    #[test]
    fn test_state_field_into_value() {
        let field = StateField::new(FrameId::new(5), 42);
        let value = field.into_value();
        assert_eq!(value, 42);
    }
}
```

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-model
cargo test state_field
cargo check
```

Fix any warnings or errors before proceeding.
