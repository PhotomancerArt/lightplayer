# Phase 1: Create Central Filesystem Module Structure

## Goal

Create a central, well-organized filesystem abstraction module in `lp-core/src/fs/`.

## Tasks

1. Create `lp-core/src/fs/` directory structure:

   ```
   fs/
   ├── mod.rs
   ├── trait.rs              # Filesystem trait (move from traits/filesystem.rs)
   └── memory.rs             # In-memory filesystem implementation (placeholder for now)
   ```

2. Move `traits/filesystem.rs` to `fs/trait.rs`:

   - Update module exports
   - Keep existing trait methods for now

3. Update `lp-core/src/lib.rs`:

   - Add `pub mod fs;`
   - Re-export filesystem trait: `pub use fs::trait::Filesystem;`

4. Update all imports:

   - Change `use crate::traits::Filesystem` to `use crate::fs::Filesystem`
   - Update `fw-host` imports

## Success Criteria

- Filesystem module structure exists in `lp-core/src/fs/`
- Filesystem trait moved to central location
- All imports updated and code compiles
- No warnings (except unused code that will be used later)
