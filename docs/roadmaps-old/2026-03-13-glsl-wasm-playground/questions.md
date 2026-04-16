# GLSL → WASM Playground: Open Questions

Goal: A web page that compiles GLSL shader source to WASM and runs it in the
browser, rendering output to a canvas. This validates the GLSL → WASM
compilation path before integrating it into the larger LightPlayer web app.

## Questions

### Q1: What is the scope of GLSL support for the initial playground?

**Context**: The full GLSL compiler supports scalars (int, uint, float, bool),
vectors (vec2/3/4), matrices (mat2/3/4), arrays, user functions, all control
flow (if/else, for, while, do-while, break, continue), builtins
(trig, noise, color), and lpfx functions.

**Suggestion**: Start with the subset needed to run the `basic/rainbow.shader`
example: scalars, vectors, basic arithmetic, control flow, and a small set
of builtins (sin, cos, mix, vec4 constructor). Add matrix and array support
later. This keeps the initial WASM codegen tractable while producing a
visible result.

**Answer**: Focus on end-to-end infrastructure first: refactoring, crate
structure, linking, build tooling. The rainbow shader is the concrete target
— implement exactly the GLSL features it requires (scalars, vec2/3/4,
arithmetic, comparisons, if/else, user functions, const bool, swizzle,
out parameters, builtins: clamp, abs, mod, fract, floor, exp, cos, sin,
smoothstep, mix, atan, min, and lpfx: lpfx_worley, lpfx_fbm,
lpfx_psrdnoise). Adding more compiler features later is easy once the
architecture is solid. This is enough work for one roadmap.

---

### Q2: Q32 or float for the WASM path?

**Context**: The device uses Q32 (Q16.16 fixed-point) exclusively. The host
JIT supports both Q32 and float. Q32 faithfulness matters for shader
debugging ("does my shader produce the same output on device?"). Float is
simpler to implement in WASM (just use f32 instructions directly).

**Suggestion**: Implement Q32 first. The whole point is debugging device
behavior, and Q32 is what runs on the device. WASM i32 operations map
directly to Q32 arithmetic (they're the same thing — i32 add, i32 mul with
shift, etc.). The builtins (sin, sqrt, noise) are already implemented as
Q32 i32 operations in lps-builtins.

**Answer**: Q32 first, but the WASM codegen must use the same pluggable
NumericMode architecture as the Cranelift backend. The decimal system
(Q32 vs float) is a core design axis of lps. The WASM backend gets
NumericMode support from the start — implement Q32Strategy now, add
FloatStrategy later. Same compiler config, same options, different backend.

---

### Q3: How should builtins be provided to the shader WASM module?

**Context**: Shaders call builtins like `__lp_q32_sin`, `lpfx_snoise`, etc.
In the JIT path, these are linked via `symbol_lookup_fn`. In WASM, the
options are:

- **WASM imports**: Shader module imports builtins; host provides them from
  a separate builtins WASM instance at instantiation time.
- **Static linking**: Bundle builtins into the shader WASM module at
  compile time.
- **Host (JS/Rust) trampolines**: Builtins implemented in the host app,
  called via WASM imports.

**Suggestion**: WASM imports from a precompiled builtins module.
lps-builtins compiles to its own `.wasm` binary once at build time.
When instantiating a shader, pass the builtins module's exports as the
import object. This mirrors the JIT path's architecture and keeps shader
modules small.

**Answer**: WASM imports from a precompiled builtins module. lps-builtins
compiles to a `.wasm` binary once at build time. Shader modules declare
builtins as imports. At instantiation, the builtins module's exports are
provided as the import object. Type-safe linking at instantiation time.
Same pattern for filetests (via wasmtime) and browser execution.

---

### Q4: What does the playground UI look like?

**Context**: This is a developer-facing test tool, not the final LightPlayer
app. It needs to be useful for validating the WASM compilation path.

**Suggestion**: Minimal two-pane layout:

- Left: GLSL source editor (CodeMirror or plain textarea for V0)
- Right: canvas showing rendered output (simulated LED strip or 2D texture)
- Below: compiler output (errors, timing, WASM module size)
- A "compile & run" button (auto-compile on change later)

**Answer**: Minimal two-pane layout. Textarea for source, canvas for output,
compiler output panel below, compile button. Plain HTML + minimal raw JS
to bootstrap and wire things up. No framework.

---

### Q5: What rendering model for the canvas?

**Context**: The shader signature is
`main(vec2 fragCoord, vec2 outputSize, float time) → vec4`.
On device, the engine calls this per-pixel and writes to a texture buffer.
The texture is then mapped to LEDs via fixtures.

For the playground, we need to visualize the output.

**Suggestion**: Render a 2D grid on a `<canvas>` element. Call the shader
for each pixel (x,y) in the grid, get the vec4 color, draw it. Animate
with requestAnimationFrame, passing elapsed time. Grid size configurable
(e.g. 256x64 default, simulating a LED matrix). This is simple and shows
the shader output directly.

**Answer**: 2D grid on a `<canvas>`. Call the shader WASM per-pixel, write
to ImageData, draw with putImageData. Animate with requestAnimationFrame.
Hardcoded 64x64 grid, scaled up 2-4x in the canvas for visibility.
Improving the debug UI is later work.

---

### Q6: Where does the GLSL → WASM compilation happen?

**Context**: The compiler frontend (parse + semantic analysis) needs to run
somewhere. Options:

- **In the browser**: Compile lps-frontend + lps-wasm to WASM.
  The compiler itself runs in the browser. Zero server dependency.
- **On a server**: Send GLSL source to a backend, get WASM bytes back.
  Simpler browser code but requires a server.

**Suggestion**: In the browser. The whole point of this exercise is proving
the all-in-browser compilation path. lps-frontend is no_std and should
compile to WASM. lps-wasm (the new crate) is designed for WASM from
the start.

**Answer**: In the browser. The compiler (lps-frontend + lps-wasm)
compiles to WASM and runs client-side. Zero server dependency.

---

### Q7: What web framework for the playground?

**Context**: We've decided on Leptos for the eventual LightPlayer app. But
the playground is a simpler, developer-facing tool.

**Suggestion**: Plain HTML + vanilla JS + Rust/WASM for the compiler. No
framework. The playground is a single page with a textarea, a canvas, and
some buttons. Using Leptos here would add complexity and coupling before
the framework decision is validated. Keep it simple: a static HTML file
that loads the compiler WASM module via wasm-bindgen.

Later, the Leptos app can reuse the lps-frontend and lps-wasm
crates directly (they're Rust).

**Answer**: Plain HTML + vanilla JS + Rust/WASM. No framework. Ideally a
single .html file (or minimal project directory). This is not the app —
it's a proof of concept that demonstrates the compilation path works.
Minimal structure. The Leptos app later imports the same Rust crates
directly.

---

### Q8: Build tooling for the playground?

**Context**: The playground needs to compile Rust to WASM and bundle it
with HTML/JS. Options:

- **wasm-pack**: Rust → WASM with wasm-bindgen glue, outputs npm package.
- **Trunk**: Leptos's recommended bundler, heavier setup.
- **Manual**: cargo build --target wasm32-unknown-unknown + wasm-bindgen CLI.

**Suggestion**: wasm-pack. It's the standard for "Rust library exposed to
JS" which is exactly what the compiler is. Outputs a pkg/ directory with
.wasm + JS glue. The HTML page loads it with a `<script type="module">`.
Simple, well-documented, no framework dependency.

**Answer**: wasm-pack. Standard tool for Rust → WASM with JS glue. Just
recipes to build and serve: `just playground-build`, `just playground-serve`.

---

### Q9: What is the testing strategy?

**Context**: We need confidence that the WASM codegen produces correct
results. The Cranelift backend has extensive filetests.

**Suggestion**: Two layers:

1. **Cross-validation tests** (Rust, runs in CI): Compile the same GLSL
   source with both Cranelift (Q32) and WASM backends. Execute both. Compare
   output values. These run as normal `cargo test` in the workspace.
2. **Visual validation** (browser): The playground itself. Load a known
   shader, verify it looks right. Manual for now.

The cross-validation tests are the important ones. They can reuse the
existing filetest GLSL sources.

**Answer**: Two layers:

1. **Filetests via wasmtime** (Rust, cargo test): Modularize the existing
   filetest architecture to support multiple runtimes. All tests are
   applicable for all targets — the "target riscv32.q32" directive is
   obsolete and should be ignored. What varies per target is which tests
   are expected to pass/fail (via per-target `[expect-fail]` annotations),
   since backends will be at different stages of development. Expectations
   (output values) may also vary per target later (e.g. float vs Q32),
   but that's not needed yet.
   wasmtime is a test-only dependency (not shipped). Builtins loaded as
   WASM imports, same as in the browser.
   This is significant infrastructure but essential — it's the same
   pattern that validated the rv32 compiler.
2. **Visual validation** (browser): The playground itself. Manual for now.

---

### Q10: Crate naming and placement?

**Context**: The compiler split produces three crates:

- lps-frontend (shared parser + semantic)
- lps-compiler (Cranelift backend, refactored)
- lps-wasm (WASM backend, new)

The playground itself is a separate build artifact.

**Suggestion**:

- `lp-shader/lps-frontend/` — new crate
- `lp-shader/lps-compiler/` — existing, refactored
- `lp-shader/lps-wasm/` — new crate
- `lp-app/playground/` — the web playground (wasm-pack project)

All in the existing workspace, with lps-wasm and playground excluded
from default-members (WASM targets).

**Answer**:

- `lp-shader/lps-frontend/` — new: parser, semantic, types, errors
- `lp-shader/lps-cranelift/` — renamed from lps-compiler: Cranelift
  backend (depends on frontend)
- `lp-shader/lps-wasm/` — new: WASM codegen backend (depends on frontend)
- `lp-app/playground/` — web playground (wasm-pack project)

Symmetric naming: lps-cranelift and lps-wasm are parallel backends.
lps-wasm and playground excluded from default-members.
Filetest infrastructure extended in existing lps-filetests with
wasmtime-based runner.
