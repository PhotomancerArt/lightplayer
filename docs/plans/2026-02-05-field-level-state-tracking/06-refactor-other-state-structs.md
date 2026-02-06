# Phase 6: Refactor Other Node State Structs

## Scope of Phase

Refactor `TextureState`, `OutputState`, and `ShaderState` to use `StateField<T>` for all fields, following the same pattern as `FixtureState`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `TextureState`

Modify `lp-model/src/nodes/texture/state.rs`:

```rust
use crate::state::StateField;
use crate::project::FrameId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureState {
    pub texture_data: StateField<Vec<u8>>,
    pub width: StateField<u32>,
    pub height: StateField<u32>,
    pub format: StateField<String>,
}

impl TextureState {
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            texture_data: StateField::new(frame_id, Vec::new()),
            width: StateField::new(frame_id, 0),
            height: StateField::new(frame_id, 0),
            format: StateField::new(frame_id, String::from("RGBA8")),
        }
    }
}

// Implement Serialize and Deserialize similar to FixtureState
```

### 2. Update `OutputState`

Modify `lp-model/src/nodes/output/state.rs`:

```rust
use crate::state::StateField;
use crate::project::FrameId;
use crate::serde_base64;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputState {
    #[serde(
        serialize_with = "serde_base64::serialize",
        deserialize_with = "serde_base64::deserialize"
    )]
    pub channel_data: StateField<Vec<u8>>,
}

impl OutputState {
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            channel_data: StateField::new(frame_id, Vec::new()),
        }
    }
}

// Note: Need to handle base64 serialization with StateField
// May need custom serialization that unwraps StateField before base64 encoding
```

### 3. Update `ShaderState`

Modify `lp-model/src/nodes/shader/state.rs`:

```rust
use crate::state::StateField;
use crate::project::FrameId;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, PartialEq)]
pub struct ShaderState {
    pub errors: StateField<Vec<String>>,  // or whatever fields ShaderState has
    // Add other fields as needed
}

impl ShaderState {
    pub fn new(frame_id: FrameId) -> Self {
        Self {
            errors: StateField::new(frame_id, Vec::new()),
        }
    }
}
```

### 4. Implement Serialize for each

Follow the same pattern as `FixtureState`:
- Check `since_frame` (from context)
- Skip fields where `changed_frame <= since_frame`
- Include all fields for initial sync (`since_frame == FrameId::default()`)

### 5. Implement Deserialize for each

Follow the same pattern as `FixtureState`:
- Use helper struct with `Option<T>` fields
- Create default state
- Merge in provided fields

### 6. Handle special cases

- **OutputState**: Base64 serialization - need to unwrap `StateField` before encoding
- **TextureState**: Large `texture_data` field - ensure partial updates work correctly
- **ShaderState**: Check what fields it actually has

### 7. Update all code that creates these states

Search for places that create these state structs:

```bash
grep -r "TextureState {" lp-core/
grep -r "OutputState {" lp-core/
grep -r "ShaderState {" lp-core/
```

Update them to use `State::new(frame_id)` and set values via `StateField::set()`.

### 8. Update tests

Update any tests that create these state structs to use the new API.

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-model
cargo test nodes::texture::state
cargo test nodes::output::state
cargo test nodes::shader::state
cargo check
```

Fix any warnings or errors before proceeding. All state structs should now use `StateField<T>` for all fields.
