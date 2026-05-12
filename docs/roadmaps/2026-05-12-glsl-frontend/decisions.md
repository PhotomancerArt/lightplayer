#### Example Compatibility First

- **Decision:** Target the current example shaders first, not full filetest compatibility.
- **Why:** Examples represent the product path we need to hot-load on device. Filetests remain the
  evidence corpus for specific features.
- **Rejected alternatives:** Full GLSL compatibility; full filetest compatibility; tiny toy subset
  with no example coverage.
- **Revisit when:** Examples compile reliably and real playlists reveal recurring unsupported syntax.

#### Resumable From Day One

- **Decision:** Implement the synchronous compile API as a wrapper over `lps_glsl::CompileJob`.
- **Why:** Hot-loading on a single-core ESP32-C6 needs cooperative scheduling, and this constraint
  should shape the compiler pipeline from the start.
- **Rejected alternatives:** Monolithic compile first, scheduler later.
- **Revisit when:** Measurements show coarse yield units are too large.

#### Typed HIR

- **Decision:** Use compact typed HIR between GLSL parsing and LPIR lowering.
- **Why:** The examples require control flow, swizzles, lvalues, casts, loops, and out/inout calls;
  direct LPIR emission would become brittle.
- **Rejected alternatives:** Full AST-first compiler; direct LPIR-only frontend; Naga-like general
  shader IR.
- **Revisit when:** HIR starts accumulating target-independent features that do not serve LPIR
  lowering or future WGSL reuse.

#### Builtins As Tables

- **Decision:** Resolve GLSL builtins and LPFN functions through compact typed tables.
- **Why:** Source-injected declarations waste parse time and memory for code the product already
  knows.
- **Rejected alternatives:** LPFN/GLSL source prelude on embedded builds.
- **Revisit when:** A builtin requires semantics too complex for table-driven resolution.

#### Semantic WGSL Reuse

- **Decision:** Share diagnostics, type representation, builtin registry, HIR, and LPIR lowering
  with a possible future WGSL frontend, but keep syntax parsers separate.
- **Why:** GLSL and WGSL grammar differ enough that shared syntax abstractions would add cost now.
- **Rejected alternatives:** A generic parser abstraction; ignoring WGSL entirely.
- **Revisit when:** WGSL becomes a committed product feature.

#### Naga As Oracle

- **Decision:** Keep Naga as the host correctness oracle for supported `lps-glsl` features.
- **Why:** Naga's breadth is valuable for differential tests even if it is not the embedded runtime
  endpoint.
- **Rejected alternatives:** Removing Naga from host tests; embedded fallback to Naga.
- **Revisit when:** `lps-glsl` has enough independent test coverage to reduce oracle usage.

#### Filetest Target Naming

- **Decision:** Add `rv32lpn.q32` for lps-glsl plus the native RV32 backend.
- **Why:** A normal target name lets filetest summaries show Naga/native and lps-glsl/native side
  by side without a second CLI selector.
- **Rejected alternatives:** `--frontend`; long names like `rv32-glsl-native.q32`; replacing
  `rv32n.q32`.
- **Revisit when:** lps-glsl needs side-by-side validation on WASM or host JIT backends.
