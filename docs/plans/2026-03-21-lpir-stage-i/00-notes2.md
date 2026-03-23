# LPIR Stage I — Pre-spec review notes (00-notes2)

Follow-up to the “another look before getting started” review. Each section is
meant to be resolved (or consciously deferred) before or while writing
`docs/lpir/*.md`.

Work through **one section at a time**: update this file with decisions,
then fold outcomes into `00-design.md` and the spec chapters.

---

## 1. Naga surface area vs LPIR mapping (in / out / deferred)

**Context**  
LPIR is only useful as a middle-end if every Naga construct your GLSL pipeline
can produce has a defined lowering or an explicit “not supported” story.

**Analysis**  
The plan phases call for a full mapping table, but there is no consolidated
inventory of Naga `Expression` / `Statement` variants vs status. Gaps tend to
show up late (e.g. `Switch`, `ArrayLength`, dynamic `Access`, `Relational`,
derivatives, atomics, resources, workgroup builtins, subgroup ops). Stage I
should produce a **single table** with columns: variant, LPIR strategy,
deferred (which phase / why), or rejected (why).

**Suggested answer**  
- Add a subsection in the spec plan (and eventually `docs/lpir/08-glsl-mapping.md`):
  **“Naga coverage matrix”** — exhaustive enum of variants, each row tagged
  `supported` | `deferred` | `out of scope (v1)` with a one-line rationale.
- **v1 in-scope** is chosen explicitly (not “whatever wasm happens to do”); see
  Decision below.

**Decision** (approved direction)

**Baseline (revised)**  
LPIR v1 is **not** limited to what `lp-glsl-wasm` implements *today* on the
Naga path. It must support **local arrays with dynamic index** and **in/out
(and inout) style parameters** so the Naga → LPIR → {WASM, Cranelift} pipeline
can match the **capabilities** of the existing **`lp-glsl-cranelift` GLSL
frontend** (legacy AST), which already does runtime `imul`+`iadd`+`load`/`store`
for arrays and **`PointerBased`** LValues for out/inout.

**Important caveat**: `lp-glsl-cranelift` today does **not** consume Naga — it
uses the old `glsl` crate IR. “Align with Cranelift frontend” means **feature
parity** (arrays + pointer parameters), not “copy its IR.” The Naga lowering
fills the gap the WASM emitter currently has.

**Textures / images / derivatives / atomics / compute barriers** remain **out of
scope v1** (user OK with that).

**Arrays (v1 required in spec + lowering)**  
- Naga `Expression::Access` (and related chains) for **local** array data →
  lowering emits **bounds-checked** address computation and LPIR `load` /
  `store` (typically `slot` + `slot_addr` + offset, or equivalent).  
- Matrix/vector dynamic index rules follow Naga’s IR constraints (e.g. matrix
  dynamic index only behind pointer — mirror in lowering).

**In / out / inout (v1 required in spec + lowering)**  
- Callee parameters that are **out** or **inout** appear as **`i32` pointer**
`FunctionArgument` values in Naga; lowering passes them through as LPIR entry
or inner `func` parameters and uses `load`/`store` through those addresses (same
spirit as `PointerBased` in Cranelift).  
- Exact **caller** ABI (who allocates storage, stack vs linear memory) is tied
to **§2 entry ABI** — must be one shared story for WASM and Cranelift.

**Implementation lag**  
The **spec** can require arrays + in/out before the **WASM LPIR backend** catches
up to the old direct-from-Naga emitter; track backend parity separately from
spec completeness.

**Deliverable**: When writing `docs/lpir/08-glsl-mapping.md`, add a **Naga
coverage matrix** (appendix or first section): every `Expression` /
`Statement` variant in **naga 29** `ir::Expression` / `ir::Statement`, each row
tagged `supported` | `deferred` | `out of scope`, with one-line notes.

### Expressions — matrix (research + revised v1 intent)

**Column “wasm today”** = direct `lp-glsl-wasm` Naga path without LPIR.
**Column “v1 LPIR spec”** = what we commit to document and lower from Naga.

| Naga `Expression` | wasm today | v1 LPIR spec | Notes |
|-------------------|------------|--------------|--------|
| `Literal` | partial | **supported** (partial) | `F32`, `I32`, `U32`, `Bool`. Wider/abstract literals **deferred** or **OOS**. |
| `Constant` | yes | **supported** | |
| `Override` | no | **deferred** | Pipeline constants. |
| `ZeroValue` | partial | **supported** (partial) | Extend as needed for array/matrix stack slots. |
| `Compose`, `Splat`, `Swizzle` | yes | **supported** | |
| `AccessIndex` | yes | **supported** | |
| `Access` | **no** | **supported** | **v1 required**: local arrays + dynamic index → bounds check + `load`/`store`. |
| `FunctionArgument` | yes | **supported** | Includes **pointer** args for out/inout. |
| `GlobalVariable` | no | **deferred**\* | \*Uniform/storage globals: use **§2 ABI** (pointer into uniform blob) unless promoted later. |
| `LocalVariable` | yes | **supported** | |
| `Load` | local only | **supported** (expand) | **v1**: `Load` through **local** pointers *and* through **function-argument** pointers (in/out). |
| `Binary`, `Unary` | yes | **supported** | `LogicalAnd`/`Or`: flat evaluation in LPIR unless lowering preserves short-circuit. |
| `Select`, `As` | yes | **supported** | `As` from vector: first component for scalar cast (match current behavior unless spec’d otherwise). |
| `CallResult` | yes | **supported** | |
| `Math` | partial | **supported** (partial) | WASM today: Mix, SmoothStep, Step, Round, Abs, Min, Max only. **v1 spec**: enumerate rest as `mathcall` + emitter errors until implemented. |
| `Relational` | no | **deferred** | `any`/`all`/etc. |
| `ImageSample`, `ImageLoad`, `ImageQuery` | no | **out of scope v1** | |
| `Derivative` | no | **out of scope v1** | |
| `AtomicResult` (+ `Statement::Atomic`) | no | **out of scope v1** | |
| `ArrayLength` | no | **deferred** | Runtime-sized arrays / buffer length. |
| `WorkGroupUniformLoadResult`, subgroup, ray query, cooperative | no | **out of scope v1** | |

### Statements — matrix (research + revised v1 intent)

| Naga `Statement` | wasm today | v1 LPIR spec | Notes |
|------------------|------------|--------------|--------|
| `Emit` | yes | **supported** | |
| `Block` | yes | **supported** | Flatten in LPIR. |
| `If`, `Loop`, `Break`, `Continue`, `Return` | yes | **supported** | |
| `Store` | local only | **supported** (expand) | **v1**: `Store` to **local** slots and through **pointer** args (inout), not only scalar locals. |
| `Call` | yes | **supported** | User + LPFX. |
| `Switch` | **no** | **deferred** | Lower to chained `if` or reserve `switch` op. |
| `Kill` | **deferred** | **deferred** | |
| `ControlBarrier`, `MemoryBarrier` | — | **out of scope v1** | |
| `ImageStore` | — | **out of scope v1** | |
| `Atomic`, `WorkGroupUniformLoad`, ray/subgroup | — | **out of scope v1** | |

**Policy**

- **Spec v1** = matrix **“v1 LPIR spec”** column, not WASM emitter completeness.
- **`Access` + pointer `Load`/`Store`** are **non-optional** for v1 (arrays +
  in/out). WASM LPIR backend work must converge there.
- **Deferred** rows (`Switch`, `Kill`, more `MathFunction`, `GlobalVariable`
  uniforms story) get milestones in implementation plans.
- Re-run when bumping **naga** minor versions.

### Spec notes — dynamic indexing and `out` / `inout` (no new IR ops)

**Recorded for `docs/lpir/03-memory.md` + `05-calls.md` / `08-glsl-mapping.md`:**

1. **Dynamic array (and similar) indexing**  
   No second dynamic operand on `load`/`store`. The **base** VReg holds the
   **full byte address** (`slot_addr` + `imul` index × stride + `iadd`, plus
   bounds checks in lowering). The **`offset` literal** on `load`/`store` stays
   **compile-time** — usually **`0`** when the address is already folded, or a
   small constant for a field within a struct element. This matches WASM
   (`memarg.offset` constant + one dynamic address) and Cranelift (one address
   `Value` + load immediate offset).

2. **`out` / `inout` / `in` pointer parameters**  
   Already covered: parameters are **`i32` addresses**; callee uses **`load` /
   `store`** (and **`memcpy`** when useful) at byte offsets. Caller allocates
   storage (`slot` + `slot_addr` or runtime memory) and passes the base **`i32`
   into `call`**. No extra opcodes — only normative prose in the spec + GLSL→
   LPIR mapping. **Where** that memory lives for the **entry function** is an
   **ABI / §2** question, not an IR extension.

---

## 2. Exported entry ABI (parameters, memory, uniforms)

**Context**  
`entry func @shader_main(...)` marks the runtime entry point, but the **meaning**
of parameters (uniform block pointer, time, scratch base, output buffer, etc.)
is currently illustrated only by informal examples (“context pointer”).

**Analysis**  
Without a written ABI, WASM and Cranelift emitters can each pick compatible
**local** lowering but disagree at the **host boundary** (argument order,
types, who allocates scratch). That breaks “validate on WASM, ship on device.”

**Suggested answer**  
- Define a **small, versioned “LPIR entry convention”** document section (fits
  in `05-calls.md` or a short `10-abi.md` chapter if you prefer separation):
  - Parameter order and types for the **default** LightPlayer shader entry
    (even if v1 is a single profile).
  - Whether **scratch / LPFX** base is an argument, a linker constant, or
    implied by the runtime.
  - That **uniforms / globals** appear as `i32` pointers + offsets agreed with
    the embedder, or as explicit entry parameters — pick one default and allow
    profiles later.
- State explicitly: anything not listed is **embedder-defined** and must be
  duplicated in both backends for a given product.

**Decision**:

1. **Versioned profile — `wasm-lp-v0` (matches `lp-glsl-wasm` today)**  
   Capture this in the eventual `05-calls.md` (or `10-abi.md`) subsection
   **“WASM LightPlayer embedding”** so Cranelift and WASM emitters can claim
   the same host boundary.

2. **Entry point vs. visibility**  
   - **`entry func`** marks the **runtime entry point** — the function the
     LightPlayer host invokes as the shader. **At most one** per module.  
   - **All functions** in the module are **visible and callable** by the host
     in JIT / test contexts. This is an **emitter concern**: the WASM
     emitter exports every function (filetests call helpers by name); the
     Cranelift JIT exposes all symbols. `entry` is semantic intent, not
     access control.  
   - **Current WASM**: Every **named user function** in the Naga pipeline's
     `functions` list becomes a **WASM export**. `main` is not part of that
     list (`extract_functions` skips `main` and `lpfx_*`).  
   - **Inner `call`**: Still `call @name`; lowering uses **function index**
     (WASM) or symbol linkage (Cranelift).

3. **Parameters and return (WASM values)**  
   - Parameters and return types are **flattened** to `f32` / `i32` **val
     types** per GLSL type and **`FloatMode`** (same as
     `glsl_type_to_wasm_components`).  
   - **No implicit** uniform/context/scratch arguments in `wasm-lp-v0` — the
     host passes only what the shader signature declares.  
   - **Future profile** (e.g. `wasm-lp-v1`): optional first-parameter
     **`i32` context** (uniform block base, global table, or packed handles)
     once Naga globals/uniforms are wired; document parameter order there.

4. **Memory (LPFX path)**  
   - When the module uses LPFX builtins: **import** `env.memory` with
     **minimum 2 pages** (see `emit_module_inner`).  
   - **LPFX `out` pointer scratch**: base **`LPFX_SCRATCH_BASE = 65536`** (byte
     offset in linear memory); allocator grows upward from there inside the
     emitter. **Not** a WASM function argument — **convention + memory
     import**.  
   - Host must provide memory that satisfies minimum size; scratch placement
     is fixed for this profile unless the profile version bumps.

5. **Uniforms / globals**  
   - **v0 Naga path**: uniform-backed globals are largely **deferred**; ABI
     does not yet mandate a uniform layout. Anything the embedder adds is
     **embedder-defined** until a versioned profile lists concrete parameter
     or memory layout.  
   - When implemented, prefer **one agreed `i32` base** (entry param or
     fixed offset) + **offsets** in linear memory over ad hoc per-backend
     rules.

6. **Cranelift parity**  
   - Same **logical** flattening of parameters/results as today’s
     `GlslExecutable` / WASM runner: host calls **by export name** with
     `GlslValue` lists mapped to scalar components.  
   - Scratch / stack for locals stays **target-specific**; only the **observable**
     contract (export names, val shapes, memory import + scratch base when
     LPFX) needs to match `wasm-lp-v0` for “validate on WASM, ship on device.”

7. **Multi-return from entry** (see §8)  
   - Allowed when the target supports it (WASM **multi-value** is already
     used in this stack). If a future host needs a single packed result,
     that becomes a **new profile** or **out-param pointer**, not an IR
     change.

---

## 3. Abstract `load` / `store` vs WASM linear memory and Cranelift memory

**Context**  
LPIR `load`/`store` are typed and use `base + offset`, but WASM needs memory
index, alignment, and offset encoding; Cranelift needs stack vs heap vs
`VMContext` and `MemFlags`.

**Analysis**  
The IR can stay abstract, but emitters need a **shared lowering contract** or
they will diverge (e.g. different alignment, different memory, wrong offsets
for LPFX scratch).

**Suggested answer**  
- In `03-memory.md` (or emitter appendix), specify **target lowering
  contracts**:
  - **WASM**: default `memory 0`; natural alignment (4 for `f32`/`i32`);
    `MemArg.offset` = static LPIR offset + optional base from pointer value;
    document when pointers are **i32 indices into linear memory** vs absolute.
  - **Cranelift**: stack slots for `slot`/`slot_addr`; linear-memory-like
    regions for runtime scratch; same **logical** layout as WASM where the
    pipeline requires parity.
- Add: **Endianness** — little-endian for scalar loads/stores (matches WASM
  and typical embedded targets).

**Decision**:

1. **LPIR stays abstract** — `slot`, `slot_addr`, `load`, `store`, `memcpy`
   have the semantics already written in `02-memory-and-calls.md`. The spec
   does not mention shadow stacks, WASM linear memory, or Cranelift stack
   slots. Those are emitter implementation details.

2. **Lowering contract (normative for emitters, not for the IR spec)**:
   - **Alignment**: 4-byte natural alignment for `f32`/`i32` (WASM `MemArg
     { align: 2 }` = 2^2; Cranelift default alignment). Unaligned access is
     target-defined (already stated).
   - **Endianness**: Little-endian for scalar loads/stores (WASM mandates it;
     typical embedded targets match).

3. **WASM `slot`/`slot_addr` lowering — shadow stack with elision**:
   - A mutable WASM global `$sp` (i32) serves as the shadow stack pointer
     into linear memory. Functions with **any `slot` declaration** emit a
     prologue (decrement `$sp` by frame size, save frame base in a local)
     and epilogue (restore `$sp`).
   - `slot_addr ssN` → `frame_base + static_offset_of(ssN)` (an `i32.add`).
   - `load`/`store` on slot-derived addresses → WASM `i32.load`/`f32.load`/
     `i32.store`/`f32.store` on `memory 0` with `MemArg { offset: <static>,
     align: 2, memory_index: 0 }`.
   - **Elision**: Functions with **no `slot` declarations** emit no
     prologue/epilogue and need no memory import. The WASM emitter only
     emits the `$sp` global and `memory` import/definition if at least one
     function in the module has slots.
   - **LPFX scratch** becomes ordinary slots in the function's shadow stack
     frame — no global `LPFX_SCRATCH_BASE`. Each function's LPFX out-pointer
     temporaries are part of its frame size. Fully reentrant, no global
     state.
   - **Threading**: `$sp` is per-instance. Host parallelism uses separate
     WASM instances (each with its own `$sp` and memory). No special
     handling needed.

4. **Cranelift `slot`/`slot_addr` lowering**:
   - `slot` → `create_sized_stack_slot` (native, hardware-stack-backed).
   - `slot_addr` → `stack_addr` (returns native pointer).
   - `load`/`store` → Cranelift `load`/`store` with `MemFlags::trusted()`.
   - No shadow stack; Cranelift handles frame layout natively.

5. **Scalars (no slots)**:
   - Plain VRegs → WASM locals / Cranelift SSA variables. No memory ops.
   - Only values that need an **address** (arrays, out/inout pointers, LPFX
     scratch) go through `slot`/`slot_addr`/`load`/`store`.

---

## 4. Recursion and call depth

**Context**  
User functions use `call`. GLSL may restrict recursion; WASM/Cranelift still
need a stack model if recursion is allowed.

**Analysis**  
If recursion is allowed in IR, you need either bounded depth, explicit stack
in linear memory, or a **well-formedness rule** forbidding cyclic call graphs
for v1.

**Suggested answer**  
- **v1**: Declare **acyclic call graph** (or “no recursion”) as a
  well-formedness rule unless you already rely on recursion in shaders.
- If you later allow recursion: specify **stack discipline** (shadow stack in
  linear memory + entry setup) in the ABI section.

**Decision**:

1. **Recursion is allowed in LPIR** — no well-formedness rule against cyclic
   call graphs. The IR does not restrict the call graph topology.

2. **GLSL version target**: The pipeline uses **GLSL 4.50 core** (`#version
   450 core` in `lp-glsl-naga`). GLSL 4.50 allows recursion (only GLSL ES
   1.00 forbids it). Record the target GLSL version in the LPIR spec
   preamble or the Naga lowering chapter so it's clear which language
   features the lowering must handle.

3. **Stack overflow is implementation-defined termination** — not undefined
   behavior. The program does not silently corrupt; it terminates with an
   error. Concrete mechanisms:
   - **WASM**: shadow stack exhausts linear memory → trap. Wasmtime fuel
     budget (`consume_fuel`, default 1M instructions) is an additional
     execution bound.
   - **Cranelift native**: hardware stack overflow → OS signal / fault.
   - **Cranelift emulator**: `max_instructions` (default 10K) and fixed
     `stack_size` (default 64KB) bound execution.

4. **The LPIR spec should state**:
   - Recursion (direct and indirect) is valid IR.
   - Stack overflow from unbounded recursion is **implementation-defined
     termination**: the host or runtime terminates execution with a
     diagnostic. The IR does not specify a depth limit.
   - Embedders **should** enforce execution bounds (fuel, instruction count,
     stack size) appropriate to their environment.

5. **Lowering policy** (not an IR rule): The Naga → LPIR lowering *may*
   warn on statically-detected deep or unbounded recursion, but does not
   reject it. The current Cranelift frontend's static recursion rejection
   is a frontend policy that predates LPIR and does not carry forward as
   an IR constraint.

---

## 5. `mathcall` semantics vs fast math / relaxed GPU libm

**Context**  
You prioritized performance and GPU-like behavior; `mathcall` covers `sin`,
`pow`, etc. WASM and Cranelift may not match each other or the GPU unless you
pin rules.

**Analysis**  
IEEE strict vs fast-math changes bitwise results and validation-vs-device
comparisons. This is orthogonal to LPIR op shapes.

**Suggested answer**  
- In `06-mathcall.md`, add a **MathCall semantic class**:
  - **Default (v1)**: “**Relaxed / implementation-defined** except as noted”
    — document that WASM and device may differ slightly; filetests use
    tolerances where needed.
  - Optional future: `strict_math` module or emitter flag for tighter
    behavior.
- List any MathFuncs that **must** match between backends (e.g. `fmin`/`fmax`
  NaN rules if you care).

**Decision**:

1. **Default: relaxed / implementation-defined**. MathCall results are
   implementation-defined within the range of "reasonable" libm behavior.
   WASM (browser libm) and device (Cranelift / builtins) may differ by
   small amounts. Filetests use tolerances where needed.

2. **No strict-math mode in v1**. Future: a `strict_math` module flag or
   emitter option could tighten semantics for specific MathFuncs if
   cross-backend bitwise reproducibility becomes a requirement.

3. **`fmin` / `fmax` NaN propagation**: follows IEEE 754-2019 `minimum` /
   `maximum` (NaN-propagating) on targets that support it, otherwise
   implementation-defined. Not pinned across backends in v1.

4. **Document in `06-mathcall.md`**: add a "Semantic precision" section
   stating the relaxed default and that cross-backend tolerance is expected
   for transcendentals. Core arithmetic ops (`fadd`, `fmul`, etc.) are
   IEEE 754 exact (not relaxed).

---

## 6. Naming and diagram consistency (minor)

**Context**  
`ieq_imm` appears in phase docs but not in the big op diagram; `br_if_not`
differs from WASM’s “branch if non-zero” naming.

**Analysis**  
Pure documentation hygiene; prevents spec drift.

**Suggested answer**  
- Refresh `00-design.md` ASCII op list to include **`ieq_imm`** (and any other
  `_imm` forms you freeze).
- One sentence under control flow: **`br_if_not v`**: exit innermost loop when
  `v == 0` (equivalent to WASM `br_if` on `v` after `i32.eqz` if needed).

**Decision**:

1. **`_imm` variants** added to the ASCII op diagram in `00-design.md`:
   `iadd_imm`, `isub_imm`, `imul_imm`, `ishl_imm`, `ishr_s_imm`,
   `ishr_u_imm`, `ieq_imm`.

2. **`br_if_not`** clarified in `00-design.md` control flow section: exits
   the innermost loop when `v == 0`; WASM lowering is `i32.eqz` + `br_if`.
   The "not" naming matches the loop-guard idiom.

---

## 7. Roadmap drift (Stage III Q32 transform vs Q32 in emitters)

**Context**  
`docs/roadmaps/2026-03-21-lpir/stage-iii.md` (and parts of IV/V) still describe
an LPIR→LPIR Q32 transform and `lpir/src/q32.rs`.

**Analysis**  
Implementers following the roadmap will build the wrong pipeline and duplicate
work already rejected in design.

**Suggested answer**  
- **Rewrite Stage III** to match current architecture: either retitle to
  “Emitter Q32 strategies / shared tables” or merge into Stage V with a clear
  note that **no `q32.rs` in `lpir`**.
- Sweep stage IV/V for “transform output” wording and replace with
  “mode-aware emitter.”

**Decision**:

1. **Stage III rewritten** — was "Q32 Transform (LPIR → LPIR)", now
   "Interpreter + Validation Hardening." No `lpir/src/q32.rs`. Q32
   expansion lives in each backend's emitter.

2. **Stage IV** — removed Q32 transform references; lowering emits
   float-agnostic ops (`fadd`, `fconst.f32`); Q32 handled by the
   mode-aware WASM emitter in Stage V.

3. **Stage V** — removed Q32 transform dependency and "post-transform"
   wording. The emitter is mode-aware: float mode → `f32.*` instructions;
   Q32 mode → inline i64 expansion. Pipeline is
   `GLSL → Naga → LPIR → WASM emitter [Q32 inside] → run`.

---

## 8. Multi-return + StructReturn (reminder)

**Context**  
Already documented: no IR cap; emitter errors if target cannot represent arity.

**Analysis**  
No new blocker; ensure **entry function** return convention is consistent with
multi-return (host may expect a single packed struct vs many values).

**Suggested answer**  
- In entry ABI (item 2), state whether the **entry function** is restricted to
  **scalar or small tuple** returns, or how large multi-return is surfaced to
  the host (pointer out-param vs multi-value).

**Decision**:

1. **Multi-return is a first-class IR feature** — no special restriction on
   entry vs other functions. Since all functions are callable by the host
   (§2), multi-return calling conventions must work everywhere, not just
   for internal `call`.

2. **ABI is a `GlslExecutable` concern** — each backend's `GlslExecutable`
   trait implementation handles the concrete multi-return ABI (WASM
   multi-value, Cranelift StructReturn, etc.). The IR spec states no cap;
   emitters error if the target can't represent the arity.

3. **Cranelift StructReturn** — noted as future work in the roadmap. When
   the Cranelift backend is migrated to LPIR, its `GlslExecutable` must
   handle large multi-return (e.g. `mat4` → 16× `f32`) via StructReturn
   or equivalent. This is a known implementation task, not a spec issue.

---

## Summary checklist

Use this when closing out 00-notes2:

- [x] Naga coverage matrix — §1 direction set (arrays + in/out in scope, textures/atomics deferred); full table deferred to `08-glsl-mapping.md`
- [x] Entry ABI written (parameters, scratch, uniforms/globals) — §2 `wasm-lp-v0`
- [x] WASM + Cranelift memory lowering contract + endianness — §3 shadow stack + elision
- [x] Recursion / call-graph rule for v1 — §4 allowed, impl-defined termination on overflow
- [x] MathCall semantic class (relaxed vs strict path) — §5 relaxed default, no strict in v1
- [x] Diagram + `br_if_not` wording aligned — §6 `_imm` in diagram, `br_if_not` clarified
- [x] Roadmap stages III–V reconciled with Q32-in-emitter — §7 rewritten
- [x] Entry return convention vs multi-return documented — §8 GlslExecutable handles ABI
