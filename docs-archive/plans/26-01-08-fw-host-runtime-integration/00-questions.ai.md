# Questions: FW-Host Runtime Integration

## Current State

- **lp-core runtime** is complete:
  - `ProjectRuntime` with `init()`, `update()`, `destroy()` methods
  - Node runtimes for Texture, Shader, Fixture, Output
  - Runtime requires `OutputProvider` trait to create LED outputs

- **fw-host** exists but runtime is not integrated:
  - `LightPlayerApp` creates `ProjectRuntime` but never calls `init()`
  - No update loop - runtime is never updated
  - `HostLedOutput` exists and implements `LedOutput` trait
  - No `OutputProvider` implementation for fw-host
  - LED visualization exists but shows static data (never updated from runtime)

- **Default project** exists:
  - Creates a simple gray shader (no animation)
  - Has texture (64x64 RGB8), shader, fixture, output configured
  - But runtime is never initialized or updated

## Goals

1. Hook up `ProjectRuntime` initialization in fw-host
2. Create `OutputProvider` implementation for fw-host
3. Set up update loop (driven by egui frame updates)
4. Sync LED output from runtime to visualization
5. Create animated demo scene showing various node capabilities
6. Ensure LEDs actually change and are visible

## Questions

1. **Update Loop Timing**: How should we drive the update loop?
   - Option A: Use egui's frame callback (`ctx.request_repaint()` + frame delta)
   - Option B: Use a separate timer thread
   - Option C: Use egui's `on_exit` or similar callback
   - **Suggested**: Option A - use egui's frame timing, call `request_repaint()` to keep loop running

2. **OutputProvider Implementation**: Where should `OutputProvider` live?
   - Option A: In `fw-host/src/app.rs` as a struct implementing the trait
   - Option B: Separate module `fw-host/src/output_provider.rs`
   - **Suggested**: Option B - keep it separate for clarity, similar to other trait implementations

3. **LED Output Synchronization**: How should we sync runtime LED output to visualization?
   - Option A: Runtime writes directly to `HostLedOutput` (shared Arc<Mutex>)
   - Option B: Copy from runtime output buffer to visualization buffer each frame
   - **Suggested**: Option A - runtime should write directly to the same `HostLedOutput` instance used for visualization

4. **Demo Scene Animation**: What kind of animation should we demonstrate?
   - Option A: Simple time-based color cycling (rainbow, etc.)
   - Option B: Moving patterns (waves, circles, etc.)
   - Option C: Multiple shaders/textures showing different effects
   - **Suggested**: Option B - a moving pattern (e.g., rotating color wheel or moving wave) that clearly shows animation

5. **Project Initialization**: When should runtime be initialized?
   - Option A: On app startup (in `init()`)
   - Option B: When project is loaded/created
   - Option C: On first frame update (lazy init)
   - **Suggested**: Option B - initialize when project is loaded/created, reinitialize if project changes
   - **DECISION**: Need higher-order construct `LPRuntime` in lp-core to manage project lifecycle

6. **Error Handling**: How should runtime errors be displayed?
   - Option A: Log to console/stderr
   - Option B: Show in egui debug panel
   - Option C: Both
   - **Suggested**: Option C - log for debugging, show status in debug panel for user visibility

## LPRuntime Architecture Questions

7. **LPRuntime Ownership**: What should `LPRuntime` own?
   - **DECIDED**: LPRuntime should own `ProjectConfig`, `ProjectRuntime`, and manage `OutputProvider`
   - Takes trait instances (like `OutputProvider`) as constructor parameters (firmware provides HAL)
   - Firmware main loop: `let mut runtime = LPRuntime::new(...hal_stuff...); loop { runtime.tick(time_diff) }`

8. **LPRuntime Constructor**: What should `LPRuntime` take as constructor parameters?
   - **DECIDED**: Takes a `Platform` struct that wraps platform-specific traits
   - `Platform { fs: Box<dyn Filesystem>, output: Box<dyn OutputProvider> }`
   - No Transport - firmware handles message I/O, passes messages to tick()
   - Composition over inheritance style, separation of concerns
   - Keep threading out of interfaces - all blocking, single-threaded
   - **DECIDED**: `Platform` lives in `lp-core/src/traits/platform.rs` (may reorganize later)

9. **Project Loading**: How should `LPRuntime` handle project loading?
   - **DECIDED**: LPRuntime handles project loading from filesystem
   - Load hard-coded "project.json" for now, eventually "config.json" with current project info
   - `load_project(path: &str) -> Result<(), Error>` - uses Platform's Filesystem
   - Goal: as much logic as possible in LPRuntime, firmware is thin wrapper

10. **Command Handling**: How should `LPRuntime` handle commands/messages?
    - **DECIDED**: LPRuntime handles command processing
    - `handle_command(cmd: Command) -> Result<(), Error>` - processes commands internally
    - LPRuntime reads from Transport and processes commands
    - Firmware is thin wrapper, LPRuntime contains the logic

11. **Timing and Message Handling**: How should timing and messages work?
    - **DECIDED**: `tick(delta_ms: u32, incoming_messages: &[MsgIn]) -> Result<Vec<MsgOut>, Error>`
    - Remove Transport from Platform - firmware handles message I/O
    - Messages passed into tick(), messages returned from tick()
    - Two enums: `MsgIn` (essentially `Command`) and `MsgOut` (responses, status updates)
    - Simpler, more explicit interface

12. **LPRuntime Location**: Where should `LPRuntime` live in lp-core?
    - **DECIDED**: `lp-core/src/runtime/lp_runtime.rs` - separate file, not in mod.rs
    - Lives alongside contexts, frame_time, lifecycle modules

13. **LPRuntime State**: What state should `LPRuntime` track?
    - **DECIDED**: Mostly just `Option<ProjectRuntime>` - config is part of project/node runtimes
    - Also track top-level errors as needed
    - Can infer loaded state from runtime being Some/None

