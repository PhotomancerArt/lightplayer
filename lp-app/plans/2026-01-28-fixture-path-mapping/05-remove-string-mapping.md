# Phase 5: Remove String-Based Mapping Support

## Goal

Remove support for string-based mapping ("linear") and update runtime to use `MappingConfig` enum exclusively.

## Changes

### 1. Update Runtime init() Method

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Remove string-based mapping handling:

```rust
fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
    // ... existing code ...

    // Remove this code:
    // if config.mapping == "linear" || config.mapping.is_empty() {
    //     self.mapping = vec![MappingPoint { ... }];
    // } else {
    //     self.mapping = Vec::new();
    // }

    // Replace with direct MappingConfig handling (already done in Phase 4)
}
```

### 2. Update Runtime update_config() Method

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Remove string-based mapping handling from `update_config()`:

```rust
fn update_config(
    &mut self,
    new_config: Box<dyn NodeConfig>,
    ctx: &dyn NodeInitContext,
) -> Result<(), Error> {
    // ... existing code ...

    // Remove this code:
    // if fixture_config.mapping == "linear" || fixture_config.mapping.is_empty() {
    //     self.mapping = vec![MappingPoint { ... }];
    // } else {
    //     self.mapping = Vec::new();
    // }

    // Mapping generation already handled in Phase 4
}
```

### 3. Verify MappingConfig Usage

**File**: `lp-engine/src/nodes/fixture/runtime.rs`

Ensure all mapping generation uses `MappingConfig` enum:

- `generate_mapping_points()` should handle all `MappingConfig` variants
- No fallback to string-based mapping
- Error handling for unsupported config variants (if any)

## Success Criteria

- All string-based mapping code removed
- Runtime uses `MappingConfig` enum exclusively
- No references to "linear" string mapping
- Code compiles without errors
- Code formatted with `cargo +nightly fmt`
