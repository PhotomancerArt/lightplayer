# Phase 3 — Consumer fan-out

`[sub-agent: yes, parallel: -]`

## Scope of phase

Update every consumer of the old
`CraneliftGraphics` / `NativeJitGraphics` types and every Cargo
feature that was forwarding the deleted `cranelift*` /
`native-jit` features to `lp-engine`. After this phase the
workspace builds end-to-end (host + RV32 + wasm32) with no backend
selection feature flags anywhere outside `lpfx-cpu` (M4c).

End state by crate:

- **`lp-server`**: defaults stripped to `["std"]` plus
  `panic-recovery`. No `cranelift*` / `native-jit` features.
  `pub use lp_engine::Graphics`. Doc-comment examples reference
  `lp_engine::Graphics`. Tests use `Graphics::new()`.
- **`lp-cli`**: `create_server.rs` and `tests/integration.rs` use
  `lp_server::Graphics`. No feature flags affected (lp-cli has no
  backend features).
- **`fw-emu`**: `cranelift` and `native-jit` features deleted.
  `default = []` (or just `[]` with feature block dropped). The
  `compile_error!`, `#[cfg(feature = "cranelift")]` /
  `#[cfg(feature = "native-jit")]` blocks in `main.rs` collapse to
  one unconditional `Graphics::new()`. The "[fw-emu] Shader
  backend:" log line becomes a single unconditional log call.
- **`fw-esp32`**: same as `fw-emu` for the cranelift / native-jit
  features and the cfg blocks. The outer `#[cfg(any(feature =
  "cranelift", feature = "native-jit"))]` block in `main.rs`
  collapses (it's currently inside the `#[cfg(feature = "server")]`
  branch — keep that gate; it's about whether the server module is
  even compiled, unrelated to backends).

**Out of scope:**

- Any `lp-engine` change (phase 2 is done).
- `lpvm-wasm` runtime / engine code (phase 1 is done).
- `lpfx-cpu` (M4c).
- Renaming or moving the firmware logging strings beyond the
  trivial collapse.
- `wasm32-unknown-unknown` host smoke / browser exec (phase 4 just
  `cargo check`s it).
- `docs/roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md`
  edits (phase 4).

## Code organization reminders

- One concept per file. Don't merge or split files; just edit them.
- Re-exports go near the top of `lib.rs` files (where they already
  live). Keep them sorted / grouped as they were.
- Mark any genuinely temporary scaffolding with a `TODO`. None
  expected here.

## Sub-agent reminders

- Do **not** commit. Phase 4 commits the whole plan.
- Do **not** expand scope. Don't refactor surrounding code, fix
  unrelated lints, or rename anything beyond what this phase
  specifies.
- Do **not** suppress warnings or add `#[allow(...)]`. Fix the
  underlying issue.
- Do **not** disable, `#[ignore]`, or weaken existing tests
  (including ones already marked `#[ignore]` for unrelated reasons —
  leave those exactly as they are).
- Do **not** touch `lpfx-cpu`. Its `cranelift` feature is unrelated
  to this swap.
- If anything is ambiguous or blocked, **stop and report** — do not
  improvise.
- Report back: files changed, validation output, and any deviations.

## Implementation details

### File 1 — `lp-core/lp-server/Cargo.toml`

Drop the four `cranelift*` and `native-jit` features and the
references to them in the `default` list. Keep `std` and
`panic-recovery`.

Replace the `[features]` block with:

```toml
[features]
default = ["std"]
# Panic recovery via catch_unwind (shader compilation, node render)
panic-recovery = ["lp-engine/panic-recovery"]
std = [
	"lp-shared/std",
	"lp-engine/std",
	"log/std"
]
```

Leave the `[dependencies]` and `[dev-dependencies]` blocks alone.

### File 2 — `lp-core/lp-server/src/lib.rs`

Replace the conditional re-exports of `CraneliftGraphics` /
`NativeJitGraphics` with a single unconditional re-export of
`Graphics`.

Before:

```rust
pub use error::ServerError;
#[cfg(feature = "cranelift")]
pub use lp_engine::CraneliftGraphics;
#[cfg(all(target_arch = "riscv32", feature = "native-jit"))]
pub use lp_engine::NativeJitGraphics;
pub use lp_engine::{LpGraphics, LpShader, ShaderCompileOptions};
pub use project::Project;
pub use project_manager::ProjectManager;
pub use server::{LpServer, MemoryStatsFn};
```

After:

```rust
pub use error::ServerError;
pub use lp_engine::{Graphics, LpGraphics, LpShader, ShaderCompileOptions};
pub use project::Project;
pub use project_manager::ProjectManager;
pub use server::{LpServer, MemoryStatsFn};
```

### File 3 — `lp-core/lp-server/src/server.rs` (doc comments)

Two doc-comment example blocks reference
`lp_engine::CraneliftGraphics::new()`. Update both to
`lp_engine::Graphics::new()`. Lines: 66 and 124 in the current
file.

### File 4 — `lp-core/lp-server/tests/server_tick.rs`

```rust
use lp_server::{CraneliftGraphics, LpGraphics, LpServer};
```

→

```rust
use lp_server::{Graphics, LpGraphics, LpServer};
```

And:

```rust
let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
```

→

```rust
let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
```

### File 5 — `lp-core/lp-server/tests/stop_all_projects.rs`

Same pattern as `server_tick.rs`. Update import + constructor.

### File 6 — `lp-core/lp-server/tests/fs_version_tracking.rs`

Same pattern. Update import + constructor.

### File 7 — `lp-cli/src/server/create_server.rs`

```rust
use lp_server::{CraneliftGraphics, LpGraphics, LpServer};
```

→

```rust
use lp_server::{Graphics, LpGraphics, LpServer};
```

And:

```rust
let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
```

→

```rust
let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
```

Tests in this file at the bottom don't reference the type name; no
edits needed there.

### File 8 — `lp-cli/tests/integration.rs`

Replace the `lp_server::CraneliftGraphics::new()` call with
`lp_server::Graphics::new()`. The test is already
`#[ignore]`d — leave the `#[ignore]` exactly as it is.

### File 9 — `lp-fw/fw-emu/Cargo.toml`

Drop the `native-jit`, `cranelift`, and the cranelift sub-feature
forwards. Drop them from `default` too. The `alloc-trace` and
`test_unwind` features stay.

Replace the `[features]` block with:

```toml
[features]
default = []
alloc-trace = ["lp-riscv-emu-guest/alloc-trace"]
# Test command __test_unwind: runs catch_unwind test before server loop (for CI)
test_unwind = []
```

`lp-server`'s default features are already disabled (`default-features
= false`), so dropping `lp-server/native-jit` from `fw-emu` here
doesn't accidentally pull in a backend selector — there is none on
`lp-server` anymore. RV32 builds of `lp-engine` (transitively via
`lp-server`) automatically pick up `gfx::Graphics` over
`lpvm-native::rt_jit` per the target-arch dispatch from phase 2.

### File 10 — `lp-fw/fw-emu/src/main.rs`

Significant collapse. The whole "no backend selected" guard, the
two backend-specific imports, the two backend-specific log lines,
and the two backend-specific `let graphics = …` arms all collapse
to a single unconditional construction.

Replace the relevant blocks as follows.

Delete entirely (lines 13–16):

```rust
#[cfg(not(any(feature = "native-jit", feature = "cranelift")))]
compile_error!(
    "fw-emu: enable `native-jit` (default) or `cranelift` for the shader graphics backend"
);
```

Imports (lines 30–34) — replace:

```rust
#[cfg(feature = "cranelift")]
use lp_server::CraneliftGraphics;
#[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
use lp_server::NativeJitGraphics;
use lp_server::{LpGraphics, LpServer};
```

with:

```rust
use lp_server::{Graphics, LpGraphics, LpServer};
```

Log lines (lines 60–63) — replace:

```rust
    #[cfg(feature = "cranelift")]
    log::info!("[fw-emu] Shader backend: Cranelift (LPIR → lpvm-cranelift)");
    #[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
    log::info!("[fw-emu] Shader backend: native JIT (lpvm-native rt_jit)");
```

with:

```rust
    log::info!("[fw-emu] Shader backend: native JIT (lpvm-native rt_jit)");
```

Graphics construction (lines 115–118) — replace:

```rust
    #[cfg(feature = "cranelift")]
    let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
    #[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
    let graphics: Arc<dyn LpGraphics> = Arc::new(NativeJitGraphics::new());
```

with:

```rust
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
```

### File 11 — `lp-fw/fw-esp32/Cargo.toml`

Drop the `native-jit`, `cranelift`, and cranelift sub-feature
forwards. Drop `native-jit` from `default`. The `server`,
`esp32c6`, `memory_fs`, all `test_*`, `ser-write-json` features
stay.

Replace the `default` line and remove the four backend-related
feature stanzas. Resulting `[features]` block:

```toml
[features]
default = ["esp32c6", "server"]
esp32c6 = [
    "esp-backtrace/esp32c6",
    "esp-bootloader-esp-idf/esp32c6",
    "esp-rtos/esp32c6",
    "esp-hal/esp32c6",
]
server = [
    "lp-model",
    "lp-server",
    "lp-shared",
    "ser-write-json",
]  # Enable server dependencies (lp-server, lp-shared, lp-model)
test_rmt = []  # Test RMT driver with simple patterns (no pipeline)
test_dither = []  # Test DisplayPipeline (use with default features; server brings lp-shared)
test_gpio = []  # Test GPIO pin toggle
test_usb = []  # Test USB serial communication
test_json = ["server"]  # Validates ser-write-json on device (sends Heartbeat via OUTGOING_SERVER_MSG)
memory_fs = []  # Use in-memory filesystem instead of persistent flash
test_oom = []   # Allocate until OOM, verify catch_unwind recovers (for panic recovery validation)
```

In `[dependencies]`, also drop the `# Graphics backend: enable
native-jit (default) or cranelift via features — do not pin
cranelift here` comment above the `lp-server` line; replace with:

```toml
# Graphics backend is selected automatically by target architecture
# (RV32 → lpvm-native::rt_jit on this firmware). No Cargo feature.
lp-server = { path = "../../lp-core/lp-server", default-features = false, features = ["panic-recovery"], optional = true }
```

The rest of `[dependencies]` is unchanged.

### File 12 — `lp-fw/fw-esp32/src/main.rs`

Mirror the fw-emu collapse.

Delete entirely (lines 22–28):

```rust
#[cfg(all(
    feature = "server",
    not(any(feature = "native-jit", feature = "cranelift"))
))]
compile_error!(
    "fw-esp32: enable `native-jit` (default) or `cranelift` for the shader graphics backend"
);
```

Imports (lines 110–114) — replace:

```rust
#[cfg(feature = "cranelift")]
use lp_server::CraneliftGraphics;
#[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
use lp_server::NativeJitGraphics;
use lp_server::{LpGraphics, LpServer};
```

with:

```rust
use lp_server::{Graphics, LpGraphics, LpServer};
```

Log lines (lines 221–224 inside the
`#[cfg(not(any(feature = "test_rmt", …)))]` block) — replace:

```rust
        #[cfg(feature = "cranelift")]
        log::info!("[fw-esp32] Shader backend: Cranelift (LPIR → lpvm-cranelift)");
        #[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
        log::info!("[fw-esp32] Shader backend: native JIT (lpvm-native rt_jit)");
```

with:

```rust
        log::info!("[fw-esp32] Shader backend: native JIT (lpvm-native rt_jit)");
```

Backend selection block (lines 315–322) — the outer
`#[cfg(any(feature = "cranelift", feature = "native-jit"))]` wrap
disappears entirely (it was guarding the LpServer construction on
"some backend selected"; backends are now always present). Replace:

```rust
        #[cfg(any(feature = "cranelift", feature = "native-jit"))]
        {
            esp_println::println!("[INIT] Creating LpServer instance...");
            let time_provider_rc = Rc::new(Esp32TimeProvider::new());
            #[cfg(feature = "cranelift")]
            let graphics: Arc<dyn LpGraphics> = Arc::new(CraneliftGraphics::new());
            #[cfg(all(feature = "native-jit", not(feature = "cranelift")))]
            let graphics: Arc<dyn LpGraphics> = Arc::new(NativeJitGraphics::new());
            let mut server = LpServer::new(
```

with (drop the outer cfg block braces and indent one level less,
keep the rest of the body):

```rust
        esp_println::println!("[INIT] Creating LpServer instance...");
        let time_provider_rc = Rc::new(Esp32TimeProvider::new());
        let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
        let mut server = LpServer::new(
```

Be careful with the matching closing brace of the outer
`#[cfg(any(...))]` block — find it (a `}` at the end of the
LpServer-construction-and-loop section) and remove it along with
the opening `{`. Re-indent the body so the surrounding
`#[cfg(not(any(test_*)))]` block stays well-formed.

## Validate

```bash
# Host: full host workspace except RV32-only crates.
cargo build --workspace \
  --exclude fw-esp32 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app

cargo test --workspace \
  --exclude fw-esp32 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app

# RV32 firmware crates.
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Wasm32 (lp-engine and any consumer that wants to target wasm later).
cargo check -p lp-engine --target wasm32-unknown-unknown
```

If any of the above fails, **stop and report**. The most likely
failure modes:

1. A `--features cranelift` reference left in CI scripts or in some
   `tests/*` file the grep missed.
2. The `fw-esp32` outer `#[cfg(any(feature = "cranelift", …))]`
   collapse breaking brace matching — re-read `main.rs` end-to-end
   if `cargo check -p fw-esp32` complains about mismatched braces.
3. A consumer crate that picks up `lp-server` via some other path
   and was relying on the `cranelift` default feature being set.

Do **not** start adding `#[allow(...)]`s or feature compatibility
shims to make a stubborn build pass. Stop and report instead.
