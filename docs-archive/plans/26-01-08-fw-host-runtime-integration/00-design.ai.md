# Design: FW-Host Runtime Integration

## Overview

Integrate `lp-core` runtime with `fw-host` to create a fully functional demo showing animated LED output. This involves creating a high-level `LpApp` in `lp-core` that manages project lifecycle and message handling, then hooking it up to `fw-host` with an update loop and animated demo scene.

## File Structure

```
lp-core/src/
├── app/
│   ├── mod.rs                      # NEW: Module exports
│   ├── lp_app.rs                   # NEW: LpApp - main entry point
│   ├── platform.rs                 # NEW: Platform struct
│   └── messages.rs                 # NEW: MsgIn/MsgOut enums
└── runtime/
    ├── contexts.rs                 # EXISTING: Render contexts
    ├── frame_time.rs              # EXISTING: FrameTime
    ├── lifecycle.rs               # EXISTING: NodeLifecycle trait
    └── ...                        # Other runtime stuff (unchanged)

fw-host/src/
├── output_provider.rs              # NEW: HostOutputProvider implementation
├── app.rs                          # MODIFY: Use LpApp instead of ProjectRuntime
└── main.rs                         # MODIFY: Set up update loop with tick()
```

## Code Structure

### lp-core/src/app/platform.rs

```rust
pub struct Platform {
    pub fs: Box<dyn Filesystem>,
    pub output: Box<dyn OutputProvider>,
}
```

Wraps platform-specific trait implementations. Firmware provides these, LpApp uses them.

### lp-core/src/app/messages.rs

```rust
pub enum MsgIn {
    UpdateProject { project: ProjectConfig },
    GetProject,
    Log { level: LogLevel, message: String },
}

pub enum MsgOut {
    Project { project: ProjectConfig },
    // Future: status updates, errors, etc.
}
```

Message types for communication. `MsgIn` is essentially the existing `Command` enum. `MsgOut` for responses and status updates.

### lp-core/src/app/lp_app.rs

```rust
pub struct LpApp {
    platform: Platform,
    runtime: Option<ProjectRuntime>,
    // top-level errors as needed
}

impl LpApp {
    pub fn new(platform: Platform) -> Self;
    pub fn load_project(&mut self, path: &str) -> Result<(), Error>;
    pub fn tick(&mut self, delta_ms: u32, incoming: &[MsgIn]) -> Result<Vec<MsgOut>, Error>;
    // Internal: handle_command, process_messages, etc.
}
```

Main application entry point. Owns `ProjectRuntime`, manages project lifecycle, handles messages.

### fw-host/src/output_provider.rs

```rust
pub struct HostOutputProvider {
    // Manages HostLedOutput instances, maps output IDs to instances
}

impl OutputProvider for HostOutputProvider {
    fn create_output(&self, config: &OutputNode) -> Result<Box<dyn LedOutput>, Error>;
}
```

Creates `HostLedOutput` instances for runtime. Manages mapping from output configs to LED output handles.

### fw-host/src/app.rs

```rust
pub struct LightPlayerApp {
    lp_app: LpApp,
    // Keep: filesystem, transport, led_output for UI/debugging
    // Remove: project, runtime (now in LpApp)
}
```

Thin wrapper around `LpApp`. Handles UI-specific concerns (debug panel, LED visualization).

### fw-host/src/main.rs

```rust
// Main loop:
// - Track frame time
// - Calculate delta_ms
// - Read messages from transport
// - Call lp_app.tick(delta_ms, messages)
// - Handle outgoing messages
// - Update UI
// - request_repaint() to keep loop running
```

Sets up egui update loop, drives `LpApp::tick()` each frame.

## New Types

- `Platform` - wraps `Filesystem` and `OutputProvider` trait objects
- `LpApp` - main application entry point, manages project lifecycle
- `MsgIn` - incoming messages (commands)
- `MsgOut` - outgoing messages (responses)
- `HostOutputProvider` - `OutputProvider` implementation for fw-host

## New Methods

- `LpApp::new(platform: Platform) -> Self` - constructor
- `LpApp::load_project(path: &str) -> Result<(), Error>` - load project from filesystem
- `LpApp::tick(delta_ms: u32, incoming: &[MsgIn]) -> Result<Vec<MsgOut>, Error>` - main update loop
- `HostOutputProvider::new(...) -> Self` - constructor (manages LED outputs)

## Modified Components

- `LightPlayerApp` - uses `LpApp` instead of managing `ProjectRuntime` directly
- Default project - update shader to rotating color wheel animation
- Main loop - calls `tick()` each frame with delta_ms and messages

## Demo Scene

Update default project shader to a rotating color wheel:
- Use `time` parameter to rotate hue
- Position-based angle calculation
- Smooth animation that clearly demonstrates the system working

