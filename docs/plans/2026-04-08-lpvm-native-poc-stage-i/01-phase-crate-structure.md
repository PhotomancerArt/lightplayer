## Phase 1: Crate Structure

### Scope

Create `lp-shader/lpvm-native/` directory, `Cargo.toml`, and empty skeleton files for all modules. Register as workspace member in `default-members`. No implementation yet — just file layout that compiles with `todo!()` stubs.

### Code organization

- Place `lib.rs` with `pub mod` declarations first
- Empty module files at bottom (utility convention per rules)

### Implementation details

**`Cargo.toml`:**
- `name = "lpvm-native"`
- `#![no_std]` + `alloc` in `lib.rs`
- deps: `lpir`, `lpvm`, `lps-shared`

**`lib.rs` skeleton:**
```rust
#![no_std]
extern crate alloc;

pub mod error;
pub mod types;
pub mod vinst;
pub mod lower;
pub mod regalloc;
pub mod isa;
pub mod module;
pub mod instance;
pub mod engine;

// Re-exports at top
pub use engine::{NativeEngine, NativeCompileOptions};
```

**Other files:** Empty with `// TODO(phase-2): implement`

### Workspace registration

Add to workspace root `Cargo.toml`:
- `members`: `"lp-shader/lpvm-native"`
- `default-members`: include same

### Tests

```bash
cargo check -p lpvm-native
```

Must pass with no warnings (allow `dead_code` for skeleton).
