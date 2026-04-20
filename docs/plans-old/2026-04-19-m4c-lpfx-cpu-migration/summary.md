### What was built

- `lpfx-cpu` rewritten as a thin shim over `lp-shader`: `CpuFxEngine`
  holds one shared `LpsEngine<LpvmBackend>`; `CpuFxInstance` wraps an
  `LpsPxShader` + `LpsTextureBuf` and renders via
  `LpsPxShader::render_frame`.
- LPVM backend selection is target-arch dispatched (no Cargo
  feature): `lpvm-native::rt_jit` on RV32, `lpvm-wasm::rt_browser` on
  wasm32, `lpvm-wasm::rt_wasmtime` on host. Lives in new
  `lpfx-cpu/src/backend.rs`. Same shape as M4b's `lp-engine`.
- `lpfx-cpu/src/render_cranelift.rs` deleted (the hand-rolled per-pixel
  Q32→unorm16 loop now lives inside `lp-shader`'s synthesised
  `__render_texture_<format>` function from M4a).
- `lpfx-cpu/src/compile.rs` shrunk to just the manifest input ↔
  uniform validator; the GLSL→LPIR pipeline is now `LpsEngine::compile_px`.
- `lpfx-cpu/Cargo.toml` drops the `cranelift` Cargo feature and the
  `lpvm-cranelift` / `lps-frontend` deps; gains target-gated
  `lpvm-native` (RV32) / `lpvm-wasm` (everything else); adds a
  `std` feature (default-on) that forwards to `lpfx`/`lp-shader`/`lps-shared`.
- `FxInstance` trait reshaped: `set_input` removed,
  `render(&mut self, time: f32)` becomes
  `render(&mut self, inputs: &FxRenderInputs<'_>)`. New
  `lpfx::FxRenderInputs<'a> { time: f32, inputs: &'a [(&'a str, FxValue)] }`
  in `lpfx/src/render_inputs.rs`.
- `FxEngine::create_texture` loses its `format` parameter
  (CPU is `Rgba16Unorm`-only today).
- New `lpfx::defaults_from_manifest(&FxManifest) -> Vec<(String, FxValue)>`
  helper in `lpfx/src/defaults.rs` — caller-side seeding now that the
  instance no longer caches per-input state.
- `lpfx::texture` shrunk to just `TextureId`; `CpuTexture` and
  `TextureFormat` (and their crate-root re-exports) deleted.
- `lpfx/Cargo.toml` gains a `std` feature (default-on) so downstream
  crates can forward `std` through it consistently with `lp-engine`'s
  pattern.
- `examples/noise.fx/main.glsl` migrated from
  `vec4 render(vec2 fragCoord, vec2 outputSize, float time)` to
  `vec4 render(vec2 pos)` with `outputSize` and `time` declared as
  `layout(binding = 0) uniform …` — matches M4a's shape for engine
  shaders.
- Roadmap milestone file gains a `## Status` section pointing at the
  archived plan (matches M4b's convention).
- Clippy `-D warnings` clean; rustfmt clean; cross-target validation
  green on host (build + test), `riscv32imac-unknown-none-elf` (check),
  and `wasm32-unknown-unknown` (check).

### Decisions for future reference

#### `lpfx-cpu` correctness tests use tiny textures (4×4)

- **Decision:** `noise_fx_renders_nonblack` and `noise_fx_default_inputs`
  both render at 4×4. The invariants ("renders some non-black pixels",
  "alpha non-zero from `render()`") don't care about resolution.
- **Why:** correctness tests should exercise the wiring (compile →
  uniforms → per-pixel kernel → texture writeback) at the smallest
  size that still proves it. Realistic-resolution work belongs in a
  perf suite, not the unit tests, and adds ~75 ms of host wall time
  per render at 256×256 with no extra coverage.
- **Background — wasmtime fuel budget:** while measuring this we
  characterised the host `lpvm-wasm` / wasmtime per-call fuel budget
  (`lpvm::DEFAULT_VMCTX_FUEL = 1_000_000`, set by
  `WasmLpvmInstance::prepare_call` before each `__render_texture_*`
  invocation). `noise.fx` burns ~1129 wasm-instructions per pixel,
  so 1M fuel only covers ~880 pixels of this shader (29×29 fits;
  32×32 traps). This is a host sandbox guard, not a real workload
  constraint, but the default is too low for realistic `lpfx`
  consumers (see follow-up below).
- **Rejected alternatives:** keeping the 29×29 / 16×16 sizes
  ("realistic-ish, fits under fuel"); bumping `DEFAULT_VMCTX_FUEL`
  inside this milestone (separate concern, see follow-up); keeping
  64×64 as originally proposed (over-tests the loop, hits the fuel
  cap, and adds wall-time for no extra coverage).
- **Revisit when:** a perf suite shows up for `lpfx-cpu` —
  realistic sizes belong there, not in `cargo test`.

#### Follow-up: raise `lpvm::DEFAULT_VMCTX_FUEL` for production `lpfx` use

- **Not done in M4c**, but worth noting: with `noise.fx` at
  ~1129 fuel/pixel, 1M fuel only covers ~880 pixels per call, and
  real `lpfx` consumers (incl. eventual `lp-engine` integration) will
  drive larger textures. The default should likely move into the
  ~10⁸ range — high enough that any plausible production frame fits
  with headroom, low enough that a true infinite loop still traps in
  well under a second on host. Ideal shape is probably a per-engine
  override on `LpsEngine` / `WasmOptions` rather than a global bump,
  so filetests can keep tighter bounds.
- **When:** any time before `lp-engine` starts pumping `lpfx-cpu`
  shaders at realistic resolution — sooner if anyone hits a confusing
  WASM trap during shader development.

#### Slice-of-name-keyed-pairs for `FxRenderInputs.inputs`

- **Decision:** `FxRenderInputs.inputs: &'a [(&'a str, FxValue)]` —
  caller passes per-render uniform values as a string-keyed slice.
- **Why:** matches `LpsPxShader::render_frame`'s shape (uniforms-per-call,
  no per-instance cache); zero per-call allocation overhead beyond the
  shader-side struct rebuild; no dep on `lps-shared` from `lpfx`.
- **Rejected alternatives:** keeping `set_input` + per-instance
  uniform cache (would have required a public per-uniform setter on
  `LpsPxShader`, widening that crate's API for one consumer);
  `BTreeMap`-backed inputs (forces an allocation, no clear win over
  a slice).
- **Revisit when:** GLSL `layout(binding = N)` uniform slot/binding
  addressing comes online (currently ignored across the workspace —
  every uniform in `noise.fx` uses `layout(binding = 0)`, syntactically
  present, semantically unused). The slice-of-name-keyed-pairs shape
  was chosen knowing it'll be replaced by slot-indexed addressing
  when that work starts.

#### `CpuFxEngine` is non-generic; engine is 1-to-1-to-1

- **Decision:** `CpuFxEngine` holds one concrete `LpsEngine<LpvmBackend>`
  where `LpvmBackend` is a target-arch type alias. No
  `CpuFxEngine<E>` generic, no `from_engine(LpsEngine<E>)` constructor.
- **Why:** mirrors M4b's `lp_engine::Graphics` shape exactly; engines
  are 1-to-1-to-1 by design (one `CpuFxEngine` owns one `LpsEngine`,
  which owns one `LpvmEngine`); a generic escape hatch is additive
  and we don't need it now.
- **Rejected alternatives:** `CpuFxEngine<E: LpvmEngine = WasmtimeEngine>`
  (the roadmap's pre-M4b draft shape — adds a non-trivial type
  parameter to the public API for no concrete consumer); a `cranelift`
  Cargo feature (directly contradicts M4b's model).
- **Revisit when:** `lp-engine` starts consuming `lpfx-cpu` and wants
  to share its `LpsEngine<E>` instance — at that point add
  `CpuFxEngine::from_engine(LpsEngine<E>)` plus an optional generic
  parameter, both additive.

#### `lpfx` and `lpfx-cpu` keep `#![no_std]` with a `std` feature

- **Decision:** kept `#![no_std]` + `extern crate alloc` on both
  crates; added a `std` Cargo feature (default-on) that forwards to
  `lp-shader`/`lps-shared`/`lpfx`.
- **Why:** `lp-engine` is expected to consume `lpfx` soon, and
  `lp-engine` builds on `riscv32imac-unknown-none-elf` for firmware
  (`fw-emu`, `fw-esp32`); `lpfx` and `lpfx-cpu` must stay
  RV32-buildable. The `std` feature mirrors `lp-engine`'s own
  pattern so the per-crate forwarding is consistent across the
  workspace.
- **Rejected alternatives:** dropping `no_std` entirely (initial
  suggestion — would lock `lpfx-cpu` out of the firmware integration
  path); `#[cfg_attr(target_arch = "riscv32", no_std)]` (more moving
  parts than just keeping `no_std` always-on with a forwarded `std`
  feature).
- **Revisit when:** never, unless the firmware integration plan is
  abandoned.
