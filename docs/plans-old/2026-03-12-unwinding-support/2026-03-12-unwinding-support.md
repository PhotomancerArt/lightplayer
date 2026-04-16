# Plan: Unwinding Support for OOM Recovery

**Date:** 2026-03-12
**Goal:** Try unwinding support in lightplayer to see if OOM recovery works and measure binary size
cost.

---

## Design

### Scope

- Implement stack unwinding via the `unwinding` crate for OOM recovery during shader compilation
- Targets: fw-emu and fw-esp32
- Record baseline binary sizes, implement changes, measure deltas
- Tests must pass; user does real-world test on ESP32

### File structure

```
lp-riscv/lp-riscv-emu-guest/
└── memory.ld                     # UPDATE: retain .eh_frame in ROM, add __eh_frame

lp-shader/lps-builtins-emu-app/
└── memory.ld                     # UPDATE: same (has own copy, uses lp-riscv-emu-guest script)

lp-fw/fw-esp32/
├── linker/
│   └── eh_frame_unwind.x         # NEW: .eh_frame in ROM + __eh_frame (overrides esp-hal's INFO)
└── .cargo/config.toml            # UPDATE: add -Teh_frame_unwind.x

lp-core/lp-engine/
├── Cargo.toml                    # UPDATE: unwinding dep (optional, no_std)
└── src/nodes/shader/runtime.rs   # UPDATE: catch_unwind around compile path

Cargo.toml                        # UPDATE: panic = "unwind" for release (RV32 packages)
lp-riscv/lp-riscv-emu-guest-test-app/
└── Cargo.toml                    # UPDATE: remove panic = "abort"
```

### Conceptual architecture

```
panic = "unwind" (workspace)  →  All crates emit .eh_frame + landing pads
                                          │
    ┌─────────────────────────────────────┼─────────────────────────────────────┐
    ▼                                     ▼                                     ▼
fw-emu (memory.ld)              fw-esp32 (eh_frame_unwind.x)           lp-engine
keeps .eh_frame in ROM          supplements esp-hal, puts              compile_shader
                                .eh_frame in ROM                       catch_unwind(...)
                                          │
                    ┌─────────────────────▼─────────────────────┐
                    │ unwinding: unwinder, fde-static,            │
                    │ personality, panic                         │
                    │ Reads __eh_frame, walks stack, runs Drop   │
                    └───────────────────────────────────────────┘
```

### Main components

- **unwinding crate**: Provides `catch_unwind`, `#[lang = eh_personality]`, panic runtime. Uses
  `__executable_start`, `__etext`, `__eh_frame` (fde-static) to locate unwind tables.
- **Linker scripts**: Emu keeps `.eh_frame` in ROM (was discarded). ESP32 supplemental script
  overrides esp-hal's `(INFO)` placement so `.eh_frame` is loadable in flash.
- **ShaderRuntime::compile_shader**: Wraps `glsl_jit_streaming` in `catch_unwind`; on panic (e.g.
  OOM), sets node to Error, server continues.
- **alloc_error_handler**: Panics on OOM (provided by allocator or we add); with `panic = "unwind"`
  this is caught by `catch_unwind`.

---

## Phases

### Phase 1: Record baseline sizes

**Scope:** Capture `size` output for fw-emu, lps-builtins-emu-app, fw-esp32 (release) before any
changes.

**Implementation Details:**

- Build fw-emu, lps-builtins-emu-app, fw-esp32 (release).
- Run `size` on each binary; append to Notes section or create
  `docs/plans/2026-03-12-unwinding-support-baseline.txt`.
- fw-esp32 binary: `target/riscv32imac-unknown-none-elf/release/fw-esp32`

**Validate:** Baseline recorded. Commands:

```bash
just build-rv32-builtins
just build-fw-emu  # or: cargo build -t riscv32imac-unknown-none-elf -p fw-emu --release
just build-fw-esp32  # or: cd lp-fw/fw-esp32 && cargo build -t riscv32imac-unknown-none-elf --release --features esp32c6
size target/riscv32imac-unknown-none-elf/release/fw-emu
size target/riscv32imac-unknown-none-elf/release/lps-builtins-emu-app
size target/riscv32imac-unknown-none-elf/release/fw-esp32
```

---

### Phase 2: Workspace panic strategy

**Scope:** Set `panic = "unwind"` for release profiles. Remove lp-riscv-emu-guest-test-app override.

**Code Organization Reminders:**

- Panic strategy must be consistent across all linked crates.

**Implementation Details:**

- In workspace `Cargo.toml`:
    - Add `panic = "unwind"` to `[profile.release]`
    - Add `panic = "unwind"` to `[profile.release-emu]` if it inherits and we want unwind for fw-emu
- In `lp-riscv/lp-riscv-emu-guest-test-app/Cargo.toml`: Remove `[profile.release]` block (or change
  `panic = "abort"` to `panic = "unwind"`).
- Host crates (std) can use `panic = "unwind"`; std supports it.

**Validate:** `cargo build --release` (host) succeeds.
`cargo build -t riscv32imac-unknown-none-elf -p fw-emu --release` succeeds.
`cargo build -t riscv32imac-unknown-none-elf -p lp-riscv-emu-guest-test-app --release` succeeds.

---

### Phase 3: Emu linker scripts

**Scope:** Update lp-riscv-emu-guest/memory.ld to retain `.eh_frame` in ROM and add `__eh_frame`.
lps-builtins-emu-app uses lp-riscv-emu-guest's script, so no separate change needed.

**Implementation Details:**

- In `lp-riscv/lp-riscv-emu-guest/memory.ld`:
    - Remove `/DISCARD/ : { *(.eh_frame .eh_frame.*) }`
    - Add before the existing sections (after .rodata) or in a suitable place:
      ```ld
      . = ALIGN(8);
      PROVIDE(__etext = .);
      PROVIDE(__eh_frame = .);
      .eh_frame : { KEEP (*(.eh_frame)) *(.eh_frame.*) } > ROM
      ```
    - Ensure `__executable_start` exists (often = ORIGIN(ROM)); add if missing:
      `PROVIDE(__executable_start = ORIGIN(ROM));`
- Reference: stack-unwinding.md lines 82–88.

**Validate:** `cargo build -t riscv32imac-unknown-none-elf -p fw-emu --release` succeeds.
`cargo build -t riscv32imac-unknown-none-elf -p lps-builtins-emu-app --release` succeeds.
`readelf -S` on fw-emu shows `.eh_frame` section present.

---

### Phase 4: ESP32 linker supplement

**Scope:** Add supplemental linker script so `.eh_frame` is loadable in ROM with `__eh_frame`.
esp-hal's eh_frame.x uses `(INFO)` (non-loadable); we override to place in ROM.

**Implementation Details:**

- Create `lp-fw/fw-esp32/linker/eh_frame_unwind.x`:
  ```ld
  /* Override esp-hal's (INFO) .eh_frame: put in ROM for unwinding. */
  SECTIONS {
    .eh_frame : ALIGN(8) {
      PROVIDE(__eh_frame = .);
      KEEP(*(.eh_frame));
      KEEP(*(.eh_frame.*));
    } > ROTEXT
  }
  INSERT AFTER .rodata;
  ```
  (Adjust INSERT placement if needed; ROTEXT aliases ROM in linkall.x.)
- In `lp-fw/fw-esp32/.cargo/config.toml`, add a second `-T` for our script. Order: linkall.x first (
  from env/config), then our script. Current rustflags: `-C link-arg=-Tlinkall.x`. Add
  `-C link-arg=-Teh_frame_unwind.x` and ensure linker can find it (e.g. `-C link-arg=-L` or place in
  crate root; or use `-C link-arg=-T${path}` via build.rs).
- build.rs: add `println!("cargo:rerun-if-changed=linker/eh_frame_unwind.x");` and
  `cargo:rustc-link-search=native` for linker dir if needed.
- Alternative: use `-C link-arg=-Tlinker/eh_frame_unwind.x` with appropriate path relative to
  manifest.

**Validate:**
`cd lp-fw/fw-esp32 && cargo build -t riscv32imac-unknown-none-elf --release --features esp32c6`
succeeds. `readelf -S` shows `.eh_frame` with loadable type (not INFO).

---

### Phase 5: Unwinding + panic integration

**Scope:** Add unwinding crate, wire personality/panic. Adapt panic handlers for `panic = "unwind"`.
Add alloc_error_handler if missing.

**Code Organization Reminders:**

- Place unwinding dep in crate that needs catch_unwind (lp-engine) or in root crate (fw-emu,
  fw-esp32). Unwinding provides `#[lang = eh_personality]` and panic runtime when used; only one
  crate in the binary should provide it.
- The unwinding crate's `personality` + `panic` features provide the lang items. We need these in
  the *binary* crates (fw-emu, fw-esp32), not in libraries, because the panic handler and
  personality are binary-level.

**Implementation Details:**

- Add `unwinding` to fw-emu and fw-esp32 (binary crates that need unwind support):
  ```toml
  unwinding = { version = "0.2", default-features = false, features = ["unwinder", "fde-static", "personality", "panic"] }
  ```
- In fw-emu: add `extern crate unwinding;` in main.rs (or in lp-riscv-emu-guest which provides the
  entry). The unwinding crate auto-provides `#[lang = eh_personality]` and panic support when
  linked.
- lp-riscv-emu-guest has a custom `#[panic_handler]`. With `panic = "unwind"`, the panic handler is
  only used for *double* panics (panic during unwind). For normal panics, control goes to
  catch_unwind. We need to either:
    - Keep lp-riscv-emu-guest's handler for double-panic (it will run if panic happens during
      unwind)
    - Or use unwinding's panic-handler feature. The unwinding crate with `panic` provides
      `begin_panic`; we still need a `#[panic_handler]` for the case when there is no catch_unwind (
      e.g. panic in main). The standard flow: `panic!` → `begin_panic` → personality → unwinder
      walks to catch_unwind or to panic_handler if none.
- For lp-riscv-emu-guest: The existing `#[panic_handler]` will be invoked when unwinding completes
  without finding a catch_unwind (e.g. panic in _lp_main). We need the unwinding crate linked so
  that `begin_panic` and the personality exist. Add unwinding to fw-emu (it pulls in
  lp-riscv-emu-guest as dep). lp-riscv-emu-guest can keep its panic_handler for the "reached top
  without catch" case.
- fw-esp32: Replace esp-backtrace with unwinding. Remove `esp-backtrace` panic-handler feature or
  use a compatibility layer. The unwinding crate with `panic-handler` feature provides a full
  handler but depends on libc for printing. For no_std esp32, we use `personality` + `panic` (no
  panic-handler) and keep a minimal `#[panic_handler]` that runs on double-panic or when no
  catch_unwind exists. Check unwinding docs: for baremetal, use `personality` and `panic`; the panic
  handler is still needed—unwinding's panic does not include a default panic_handler. We need to
  provide `#[panic_handler]` that calls `unwinding::panic::begin_panic` or similar. Actually: with
  `panic = "unwind"`, rustc expects a different flow. The `#[panic_handler]` is called when the
  panic cannot be unwound (no matching catch_unwind). So we keep a panic_handler that aborts or
  reports. For fw-emu, lp-riscv-emu-guest's handler does that. For fw-esp32, esp-backtrace provides
  it. The key: we need the unwinder and personality. unwinding with `personality` and `panic`
  provides those. The existing panic_handlers will work for the "abort" case (double panic or panic
  with no catch). So we just need to add the unwinding crate so the unwinder and personality are
  linked in.
- alloc_error_handler: Check if linked_list_allocator or esp-alloc provides it. If not, add in
  fw-emu and fw-esp32:
  ```rust
  #[alloc_error_handler]
  fn on_alloc_error(layout: Layout) -> ! {
      panic!("OOM: alloc {} bytes align {}", layout.size(), layout.align());
  }
  ```

**Validate:** `cargo build -t riscv32imac-unknown-none-elf -p fw-emu --release` and fw-esp32 build
succeed. No duplicate lang item errors. Run fw-emu in emulator; normal boot works.

---

### Phase 6: catch_unwind in ShaderRuntime

**Scope:** Wrap the compile path in `catch_unwind`. On Err (panic), set node to Error state.

**Code Organization Reminders:**

- Keep the closure minimal; capture only what's needed. Use `AssertUnwindSafe` if the closure
  captures mutable refs and unwinding requires it.

**Implementation Details:**

- In `lp-core/lp-engine/src/nodes/shader/runtime.rs`, in `compile_shader`:
    - For `cfg(not(feature = "std"))`: use `unwinding::panic::catch_unwind`. Add `unwinding`
      dependency to lp-engine with `optional = true` and a feature like `unwind` or enable when
      `std` is disabled.
    - Wrap the `glsl_jit_streaming` call and state updates in a closure. The closure returns
      `Result<(), Error>`; we need to return the executable and update self. The pattern:
        - Extract `glsl_source` and `options` (they're already local).
        -
      `match catch_unwind(AssertUnwindSafe(|| { glsl_jit_streaming(glsl_source, options) })) { Ok(Ok(executable)) => { ... }, Ok(Err(e)) => { ... }, Err(_) => { /* panic (OOM) */ ... } }`
    - On panic (Err from catch_unwind): set compilation_error, clear executable, set state.error,
      return Err.
- lp-engine needs unwinding only for no_std. Add:
  ```toml
  unwinding = { version = "0.2", optional = true, default-features = false }
  ```
  and a feature that enables it when building for no_std. Simpler: make unwinding a required dep
  when not(std), optional when std. Use:
  ```toml
  [target.'cfg(not(feature = "std"))'.dependencies]
  unwinding = { version = "0.2", default-features = false }
  ```
  Actually lp-engine doesn't have a "std" feature that we control per-target; fw-esp32 uses
  default-features = false. So when fw-esp32 builds lp-engine, std is off. So we can add unwinding
  as a dep that's only used when alloc is available (which it is). Add unwinding to lp-engine,
  always, but only call catch_unwind on no_std. Or: use std::panic::catch_unwind when std,
  unwinding::panic::catch_unwind when no_std. That requires conditional compilation.
- Simplest: add unwinding to lp-engine unconditionally. When std, we could use std's catch_unwind,
  but that would require linking std. For lp-engine with default-features = false (no std), we use
  unwinding. So: add unwinding to lp-engine, and use it. But lp-engine is also used by lp-cli (host)
  with std—there we have std::panic::catch_unwind. So we need:
  `#[cfg(feature = "std")] use std::panic::catch_unwind; #[cfg(not(feature = "std"))] use unwinding::panic::catch_unwind;`.
  And only wrap in catch_unwind when no_std (since OOM recovery is for embedded). When std, we can
  skip the wrap (or use std's for consistency). For the experiment, wrapping on both is fine; std's
  catch_unwind works on host.
- Actually the plan is to try unwinding on embedded. On host, OOM is less critical. We could add a
  feature "oom-recovery" to lp-engine that enables the catch_unwind path, and enable it for fw-emu
  and fw-esp32. That way we don't change host behavior and don't add unwinding to host builds. So:
  lp-engine feature "oom-recovery" or "unwind", when set, uses catch_unwind. fw-emu and fw-esp32
  enable that feature. unwinding dep is optional, enabled by the feature.
- Add to lp-engine Cargo.toml:
  ```toml
  [features]
  oom-recovery = ["unwinding"]
  unwinding = { version = "0.2", optional = true, default-features = false }
  ```
- fw-emu and fw-esp32 depend on lp-engine. They get lp-engine through lp-server (fw-emu) or
  directly (fw-esp32). Need to enable oom-recovery: lp-server depends on lp-engine; fw-emu depends
  on lp-server. So we'd need lp-server to expose the feature or fw-emu to pass it through. Easier:
  fw-emu and fw-esp32 add lp-engine as direct dep with the feature? Or lp-server has a feature that
  enables lp-engine's oom-recovery. Check: fw-emu uses lp-server, not lp-engine directly. lp-server
  uses lp-engine. So we need lp-server to propagate the feature. Add to lp-server:
  `lp-engine = { path = "...", features = ["oom-recovery"] }` when we want it. Or a feature "
  oom-recovery" on lp-server that enables it for lp-engine. This gets complex. Simpler: add
  unwinding directly to lp-engine, make it a default dependency for no_std. When lp-engine is built
  without std (for fw-emu/fw-esp32), we add unwinding. When with std (for lp-cli), we use std's
  catch_unwind. So: always add unwinding to lp-engine. When std, use std::panic::catch_unwind. When
  not std, use unwinding::panic::catch_unwind. That way we only need the unwinding crate when
  building for no_std. But unwinding would still be a dep—cargo might build it for std too. Let me
  check—if we have `unwinding = { optional = true }` and enable via feature for no_std only, then
  for std builds unwinding isn't built. So: `unwinding = { optional = true }` and a feature that's
  enabled by the no_std targets. The problem is features are typically from the top-level crate.
  fw-emu and fw-esp32 are the top-level. So fw-emu could have a feature "oom-recovery" that gets
  passed to lp-server and down. Cargo features are unified: if any dep needs a feature, it's on. So
  we'd have fw-emu with feature "oom-recovery", and lp-server depends on lp-engine with
  `features = ["oom-recovery"]` (forwarded). This is doable. For simplicity in the plan: Add
  unwinding to lp-engine as optional, feature "oom-recovery". lp-server adds dependency
  `lp-engine = { ..., features = ["oom-recovery"] }` when lp-server has a feature "oom-recovery".
  fw-emu and fw-esp32 enable lp-server (or lp-engine) with oom-recovery. Actually the simplest is:
  lp-engine has optional unwinding. fw-emu and fw-esp32 pass the feature when they depend on
  lp-engine. But fw-emu doesn't depend on lp-engine directly—it uses lp-server. So we need lp-server
  to depend on lp-engine with oom-recovery. We could add oom-recovery as default for lp-server when
  building for no_std—that's hard. Easier: add a feature to fw-core or a new crate. Actually, the
  simplest: make unwinding a non-optional dep of lp-engine when default-features = false. So
  whenever lp-engine is built without std, it has unwinding. So:
  `unwinding = { version = "0.2", default-features = false }` as a dep, and we have a
  `[target.'cfg(not(feature = "std"))'.dependencies]` section. In Cargo, we can't do target-based
  deps like that easily. We use features: `unwinding = { optional = true }` and
  `oom-recovery = ["unwinding"]`. The workspace or the binary crates enable it. So fw-emu enables
  oom-recovery in lp-server. lp-server has
  `lp-engine = { path = "...", features = ["oom-recovery"] }` under a feature. So we add
  `[features] oom-recovery = []` to lp-server, and when that's enabled, we pass it to lp-engine. So:
  lp-server Cargo.toml:
  `lp-engine = { path = "...", default-features = false, features = ["oom-recovery"] }` — but that
  would always enable it. We want it only for fw-emu and fw-esp32. So lp-server has
  `lp-engine = { path = "...", default-features = false }` and we need a way to add oom-recovery.
  The standard pattern: lp-server has a feature `oom-recovery = ["lp-engine/oom-recovery"]` (weak
  dependency feature). So when fw-emu sets `lp-server = { features = ["oom-recovery"] }`, lp-engine
  gets oom-recovery. Good.
- Implementation: lp-engine has `oom-recovery = ["unwinding"]`, `unwinding = { optional = true }`.
  lp-server has `oom-recovery = ["lp-engine/oom-recovery"]`. fw-emu:
  `lp-server = { features = ["oom-recovery"] }`. fw-esp32:
  `lp-engine = { features = ["oom-recovery"] }` or however lp-engine is pulled in. Need to check
  fw-esp32 deps—it has lp-engine directly for the server feature. So fw-esp32:
  `lp-engine = { ..., features = ["oom-recovery"] }`.
- In compile_shader, use:
  ```rust
  #[cfg(feature = "oom-recovery")]
  use unwinding::panic::catch_unwind;
  ``` 
  And wrap when the feature is on. When off, keep current behavior (no catch).

**Validate:** `cargo test -p lp-engine` passes. Build fw-emu and fw-esp32 with oom-recovery. Run a
scene in emulator; shader compiles. (OOM test: reduce heap and verify recovery—can be manual.)

---

### Phase 7: Build, measure deltas, validate

**Scope:** Rebuild all targets. Run `size`, record deltas. Run full test suite. User does ESP32
real-world test.

**Implementation Details:**

- Rebuild fw-emu, lps-builtins-emu-app, fw-esp32 (release).
- Run `size` on each, compare to baseline.
- Update plan Notes with delta table.
- Run `just test` or `cargo test`.
- Document commands for user to flash and test on ESP32.

**Validate:** All builds succeed. Size deltas recorded. Tests pass. User reports ESP32 test result.

---

### Phase 8: Cleanup & validation

**Scope:** Remove TODOs, debug prints, fix warnings, formatting.

**Implementation Details:**

- `cargo +nightly fmt`
- `just check` or `cargo clippy`
- Grep for TODO, FIXME, dbg!, println! in modified files
- Fix any warnings

**Validate:** `just check`, `just ci` (or equivalent) passes.

---

## Notes

### Baseline binary sizes (release) — before unwinding

| Binary                   | text      | data   | bss     | dec       |
|--------------------------|-----------|--------|---------|-----------|
| fw-emu                   | 1,301,600 | 32     | 524,848 | 1,826,480 |
| lps-builtins-emu-app | 53,344    | 8      | 524,552 | 577,904   |
| fw-esp32                 | 1,588,236 | 23,272 | 323,224 | 1,934,732 |

### Size deltas after unwinding (fw-emu)

|                | text              | dec              |
|----------------|-------------------|------------------|
| With unwinding | 1,448,660         | 1,973,540        |
| Delta          | +147,060 (+11.3%) | +147,060 (+8.0%) |

### fw-esp32 build blocked

fw-esp32 fails to build with nightly with error: `multiple candidates for rmeta dependency alloc` (
hashbrown + rustc-std-workspace-alloc vs build-std alloc). Needs investigation. fw-emu builds
successfully.

### ESP32 eh_frame findings

- esp-hal `eh_frame.x` uses `(INFO)` = non-allocatable → not loaded into flash
- `__eh_frame` symbol not provided
- Supplemental script must put `.eh_frame` in ROM and add `PROVIDE(__eh_frame = .);`
