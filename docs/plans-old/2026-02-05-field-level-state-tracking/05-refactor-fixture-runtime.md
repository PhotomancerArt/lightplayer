# Phase 5: Refactor FixtureRuntime to Store State Directly

## Scope of Phase

Refactor `FixtureRuntime` to store `FixtureState` directly as a field, and update all code that modifies fixture state to use `StateField` methods. This is a significant refactor that changes how state is managed.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Update `FixtureRuntime` struct

Modify `lp-engine/src/nodes/fixture/runtime.rs`:

```rust
use lp_model::nodes::fixture::FixtureState;
use lp_model::FrameId;

pub struct FixtureRuntime {
    config: Option<FixtureConfig>,
    state: FixtureState,  // NEW: State stored directly
    // Implementation details (not in state):
    color_order: ColorOrder,
    mapping: Vec<MappingPoint>,
    transform: [[f32; 4]; 4],
    texture_width: Option<u32>,
    texture_height: Option<u32>,
    precomputed_mapping: Option<PrecomputedMapping>,
    brightness: u8,
    gamma_correction: bool,
}
```

### 2. Update `new()` method

```rust
impl FixtureRuntime {
    pub fn new() -> Self {
        Self {
            config: None,
            state: FixtureState::new(FrameId::default()),
            color_order: ColorOrder::Rgb,
            mapping: Vec::new(),
            transform: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            texture_width: None,
            texture_height: None,
            precomputed_mapping: None,
            brightness: 64,
            gamma_correction: true,
        }
    }
}
```

### 3. Update `init()` method

When initializing, set initial state values with the current frame ID:

```rust
fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
    // ... existing init logic ...
    
    // Set initial state values
    let frame_id = /* get from context or use FrameId::default() */;
    self.state.texture_handle.set(frame_id, texture_handle.map(|h| h.as_node_handle()));
    self.state.output_handle.set(frame_id, output_handle.map(|h| h.as_node_handle()));
    
    // ... rest of init ...
}
```

### 4. Update `update()` method

When updating `lamp_colors`, use `StateField::set()`:

```rust
fn update(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
    // ... existing update logic ...
    
    // Store lamp colors for state extraction
    self.lamp_colors.clear();
    self.lamp_colors.resize((max_channel as usize + 1) * 3, 0);
    
    // ... fill lamp_colors ...
    
    // Update state field
    self.state.lamp_colors.set(ctx.frame_id(), self.lamp_colors.clone());
    
    // ... rest of update ...
}
```

### 5. Update mapping cells update

When `mapping_cells` changes (derived from internal `mapping`), update the state:

```rust
// When mapping is regenerated:
let mapping_cells = convert_mapping_to_cells(&self.mapping);
self.state.mapping_cells.set(ctx.frame_id(), mapping_cells);
```

### 6. Update handle resolution

When handles are resolved, update state:

```rust
self.state.texture_handle.set(ctx.frame_id(), Some(texture_handle.as_node_handle()));
self.state.output_handle.set(ctx.frame_id(), Some(output_handle.as_node_handle()));
```

### 7. Remove old getter methods

Remove methods like `get_lamp_colors()`, `get_mapping()`, etc., since state is now directly accessible. Or keep them as convenience methods that delegate to `state.field.get()`.

### 8. Update state extraction in `ProjectRuntime`

In `lp-engine/src/project/runtime.rs`, update state extraction to just clone the state:

```rust
NodeKind::Fixture => {
    if let Some(runtime) = &entry.runtime {
        if let Some(fixture_runtime) = runtime.as_any().downcast_ref::<FixtureRuntime>() {
            NodeState::Fixture(fixture_runtime.state.clone())
        } else {
            // Fallback
            NodeState::Fixture(FixtureState::new(FrameId::default()))
        }
    } else {
        NodeState::Fixture(FixtureState::new(FrameId::default()))
    }
}
```

### 9. Update all references to old fields

Search for code that accesses `lamp_colors`, `mapping`, etc. directly and update to use `state.field.get()` or `state.field.set()`.

## Validate

Run the following commands to validate this phase:

```bash
cd lp-core/lp-engine
cargo test nodes::fixture
cargo check
```

Fix any warnings or errors before proceeding. The fixture runtime should now store state directly and update it via `StateField` methods.
