# Phase 4: Refactor FixtureState to Use StateField

## Scope of Phase

Refactor `FixtureState` to use `StateField<T>` for all fields. This is the first real state struct to be converted, serving as a template for other node types.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `lp-model/src/nodes/fixture/state.rs`

Modify `FixtureState` to use `StateField<T>`:

```rust
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::nodes::handle::NodeHandle;
use crate::state::StateField;

/// Mapping cell - represents a post-transform sampling region
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MappingCell {
    /// Output channel index
    pub channel: u32,
    /// Center coordinates in texture space [0, 1] (post-transform)
    pub center: [f32; 2],
    /// Sampling radius
    pub radius: f32,
}

/// Fixture node state - runtime values
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureState {
    /// Lamp color values (RGB per lamp)
    pub lamp_colors: StateField<Vec<u8>>,
    /// Post-transform mapping cells (sampling regions)
    pub mapping_cells: StateField<Vec<MappingCell>>,
    /// Resolved texture handle (if fixture has been initialized)
    pub texture_handle: StateField<Option<NodeHandle>>,
    /// Resolved output handle (if fixture has been initialized)
    pub output_handle: StateField<Option<NodeHandle>>,
}

impl FixtureState {
    /// Create a new FixtureState with default values
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            lamp_colors: StateField::new(frame_id, Vec::new()),
            mapping_cells: StateField::new(frame_id, Vec::new()),
            texture_handle: StateField::new(frame_id, None),
            output_handle: StateField::new(frame_id, None),
        }
    }
}
```

### 2. Implement custom Serialize

Add custom serialization that skips unchanged fields. Use the approach validated in Phase 3:

```rust
impl Serialize for FixtureState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Get since_frame from serialization context
        // TODO: Implement proper context passing (from Phase 3)
        let since_frame = /* get from context */;
        let is_initial_sync = since_frame == FrameId::default();
        
        let mut state = serializer.serialize_struct("FixtureState", 4)?;
        
        if is_initial_sync || self.lamp_colors.changed_frame() > since_frame {
            state.serialize_field("lamp_colors", self.lamp_colors.value())?;
        }
        if is_initial_sync || self.mapping_cells.changed_frame() > since_frame {
            state.serialize_field("mapping_cells", self.mapping_cells.value())?;
        }
        if is_initial_sync || self.texture_handle.changed_frame() > since_frame {
            state.serialize_field("texture_handle", self.texture_handle.value())?;
        }
        if is_initial_sync || self.output_handle.changed_frame() > since_frame {
            state.serialize_field("output_handle", self.output_handle.value())?;
        }
        
        state.end()
    }
}
```

### 3. Implement Deserialize

Add custom deserialization that handles partial JSON:

```rust
impl<'de> Deserialize<'de> for FixtureState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FixtureStateHelper {
            lamp_colors: Option<Vec<u8>>,
            mapping_cells: Option<Vec<MappingCell>>,
            texture_handle: Option<Option<NodeHandle>>,
            output_handle: Option<Option<NodeHandle>>,
        }
        
        let helper = FixtureStateHelper::deserialize(deserializer)?;
        
        // For deserialization, we need a frame_id, but we don't have one in JSON
        // Use current frame or default - this will be updated when merged with existing state
        let frame_id = FrameId::default();
        
        let mut state = FixtureState::new(frame_id);
        
        if let Some(val) = helper.lamp_colors {
            state.lamp_colors.set(frame_id, val);
        }
        if let Some(val) = helper.mapping_cells {
            state.mapping_cells.set(frame_id, val);
        }
        if let Some(val) = helper.texture_handle {
            state.texture_handle.set(frame_id, val);
        }
        if let Some(val) = helper.output_handle {
            state.output_handle.set(frame_id, val);
        }
        
        Ok(state)
    }
}
```

### 4. Update all code that creates FixtureState

Search for places that create `FixtureState`:

```bash
grep -r "FixtureState {" lp-core/
```

Update them to use `FixtureState::new(frame_id)` and then set values via `StateField::set()`.

### 5. Update tests

Update any tests that create `FixtureState` to use the new API:

```rust
let mut state = FixtureState::new(FrameId::new(1));
state.lamp_colors.set(FrameId::new(2), vec![255, 0, 0]);
```

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-model
cargo test nodes::fixture::state
cargo check
```

Fix any warnings or errors before proceeding. All code that uses `FixtureState` must be updated to work with `StateField<T>` fields.
