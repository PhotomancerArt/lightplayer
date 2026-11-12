# Design: Separate Crates Refactor

## Overview

Refactor the LightPlayer codebase to clearly separate responsibilities into distinct crates while maintaining backward compatibility. `lp-core-cli` continues to work exactly as it does now, but uses the new library structure internally.

## File Structure

```
lp-app/
├── crates/
│   ├── lp-util/                    # NEW: Shared utilities
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── fs/
│   │       │   ├── mod.rs
│   │       │   ├── lp-fs.rs        # MOVED from lp-core/fs/lp-fs.rs
│   │       │   └── lp-fs-mem.rs    # MOVED from lp-core/fs/lp-fs-mem.rs
│   │       └── log/
│   │           └── mod.rs          # MOVED from lp-core/log/mod.rs
│   │
│   ├── lp-api/                     # NEW: Client/server protocol
│   │   └── src/
│   │       ├── lib.rs
│   │       └── messages.rs         # NEW: ClientMsg, ServerMsg enums
│   │
│   ├── lp-core/                     # MODIFIED: Single-project runtime
│   │   └── src/
│   │       ├── lib.rs               # MODIFY: Remove fs, log, add lp-util dep
│   │       ├── api/
│   │       │   ├── mod.rs           # MODIFY: Remove broken command/message refs
│   │       │   └── messages.rs       # KEEP: MsgIn/MsgOut (single-project)
│   │       ├── app/
│   │       │   ├── lp_app.rs        # MODIFY: Remove create_default_project()
│   │       │   └── ...              # KEEP: Other app code
│   │       ├── fs/                  # REMOVE: Move to lp-util
│   │       ├── log/                 # REMOVE: Move to lp-util
│   │       └── ...                   # KEEP: nodes, project, runtime, etc.
│   │
│   └── lp-server/                   # NEW: Multi-project server library
│       └── src/
│           ├── lib.rs
│           ├── server.rs             # NEW: Server functionality
│           ├── project_manager.rs    # NEW: Multi-project management
│           ├── project.rs            # NEW: Project instance wrapper
│           └── error.rs              # NEW: Server-specific errors
│
└── apps/
    └── lp-core-cli/                 # MODIFIED: Uses lp-server library
        └── src/
            └── main.rs               # MODIFY: Use lp-server library instead of direct LpApp management
```

## Code Structure

### lp-util (NEW)

- **Purpose**: Shared utilities used by all crates
- **Contents**:
  - `LpFs` trait (filesystem abstraction) - moved from `lp-core/fs/`
  - `InMemoryFilesystem` implementation - moved from `lp-core/fs/`
  - Logging utilities - moved from `lp-core/log/`

### lp-api (NEW)

- **Purpose**: Client/server protocol message types
- **Contents**:
  - `ClientMsg` enum (filesystem ops, sync, debug queries)
  - `ServerMsg` enum (responses, file sync, debug data, logs)
  - Protocol serialization helpers

### lp-core (MODIFIED)

- **Purpose**: Single-project runtime (cleaned up)
- **Changes**:
  - Remove `fs/` module → depend on `lp-util` for `LpFs` trait
  - Remove `log/` module → depend on `lp-util` for logging
  - Remove `create_default_project()` from `LpApp`
  - Keep `api/messages.rs` with `MsgIn`/`MsgOut` (single-project focused)
  - Clean up `api/mod.rs` (remove broken `command`/`message` references)
  - `LpApp::load_project()` should fail if project doesn't exist (no auto-creation)

### lp-server (NEW - LIBRARY)

- **Purpose**: Multi-project server functionality (library crate)
- **Contents**:
  - `ProjectManager` - manages multiple project instances
  - `Project` - wraps `LpApp` instance with project metadata
  - `Server` - server functionality (can be used by binaries)
  - Translates between `lp-api` messages and `lp-core` messages
  - Project creation/loading/unloading logic
  - `create_project(name)` - creates filesystem structure and initializes `LpApp`

### lp-core-cli (MODIFIED)

- **Purpose**: Server binary (continues to work exactly as it does now)
- **Changes**:
  - Use `lp-server` library internally instead of directly managing `LpApp`
  - Keep all current functionality (GUI, file watching, etc.)
  - External behavior unchanged - this is an internal refactoring
  - Later: UI can move to `lp-cli` when it's built

## Key Design Decisions

1. **Backward Compatibility**: `lp-core-cli` continues to work exactly as before
2. **Incremental Migration**: Refactoring is internal - external API stays the same
3. **Clear Separation**: Each crate has a single, well-defined responsibility
4. **Library vs Binary**: `lp-server` is a library, `lp-core-cli` is the binary that uses it

## Dependencies

```
lp-core → lp-util (for LpFs trait, logging)
lp-server → lp-core, lp-api, lp-util
lp-core-cli → lp-server, lp-core, lp-api, lp-util
lp-client (future) → lp-api, lp-util
```
