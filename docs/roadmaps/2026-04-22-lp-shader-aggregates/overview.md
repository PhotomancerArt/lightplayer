# lp-shader Aggregates Roadmap — Overview

## Motivation / rationale

Domain work needs first-class GLSL **structs** — both as locals and across
function boundaries. Today `lp-shader` recognises `LpsType::Struct` enough
to handle uniform-block fields (`global/type-struct.glsl` already passes
on `jit.q32`), but the moment a user writes `Point p; … modifyPoint(p);`
the frontend errors out with "unsupported type."

Adding struct support is the immediate trigger, but the deeper problem is
that `lp-shader` has **two coexisting aggregate ABIs**:

- Arrays today pass `in` params as flat scalar VRegs (one VReg per element).
- The cleanest place to put structs is as pointer-to-slot.
- Arrays-of-structs would force a third hybrid path.

Picking a struct ABI without aligning the array ABI commits us to that
asymmetry permanently. This roadmap takes the surgery once: collapse all
aggregates onto a single pass-by-pointer ABI, add struct support against
that ABI, then layer arrays-of-structs and uniform-array-fields on top. The
optimisation pass that recovers tiny-aggregate perf is a separate,
deferrable milestone.

A bonus consequence: `LpvmDataQ32` (already in tree) is a typed std430
byte buffer with `as_ptr()` / `as_mut_ptr()`. Once the JIT call ABI is
"pass pointer to std430 buffer," the rust call boundary becomes uniform —
`LpvmDataQ32::from_value(ty, &value).as_ptr()` for any aggregate, no
flatten/unflatten code on the host side.

Pain points addressed:

- **Domain shaders can't compile today** because they need
  `struct Point { … }; void f(Point p);`.
- **`lpvm_abi::flatten_q32_arg` / `decode_q32_return`** carry growing
  aggregate-ABI complexity that will only get worse as more types land.
- **Backends (cranelift host JIT, cranelift RV32, wasm)** each need to
  reason about "many flat scalar args per aggregate" — a fragile,
  register-pressure-sensitive shape that breaks down for large structs and
  arrays-of-structs.

## Architecture / design

### Unified pass-by-pointer aggregate ABI

| Position           | ABI                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------ |
| `in T` aggregate   | Single `Pointer` arg → callee `Memcpy` into own slot (skip when read-only — M5)     |
| `inout`/`out` agg. | Single `Pointer` arg → callee writes through                                        |
| Aggregate return   | Hidden `Pointer` first arg (`sret`) → callee writes through                          |
| Scalar / vec / mat | **Unchanged** — flat scalar VRegs by value                                          |

Local aggregates: always slot-backed. "Aggregates are always data."

### Layering (frontend → IR → backends → host)

```
       ┌─────────────────────────────────────────┐
       │  GLSL  (Naga frontend)                  │
       │  Struct, Array, Array-of-Struct types   │
       └────────────────┬────────────────────────┘
                        │
                        ▼
  ┌─────────────────────────────────────────────────┐
  │  lps-frontend                                   │
  │  ┌───────────────────────────────────────────┐  │
  │  │  LowerCtx                                 │  │
  │  │    ArrayInfo  (extended: leaf may be      │  │
  │  │                LpsType::Struct in M3)     │  │
  │  │    StructInfo (M2: slot, std430 size,     │  │
  │  │                member_offsets, IR types)  │  │
  │  │    AggregateSlot (unified slot abstraction│  │
  │  │                   over both)              │  │
  │  └───────────────┬───────────────────────────┘  │
  │                  │                              │
  │  ┌───────────────▼─────────────┐                │
  │  │  lower_call (M1)            │                │
  │  │    in agg : Address(slot)   │                │
  │  │             → Pointer arg   │                │
  │  │    inout  : Address(slot)   │                │
  │  │             → Pointer arg   │                │
  │  │    return : alloc dest slot │                │
  │  │             → sret pointer  │                │
  │  └─────────────────────────────┘                │
  │                                                 │
  │  lower_struct (M2): zero_fill, member_load,     │
  │    member_store, memcpy, compose_into_slot,     │
  │    flatten_for_pointer_arg                      │
  │                                                 │
  │  lower_array (M1, M3): existing array machinery │
  │    extended for pointer-arg ABI and struct leaf │
  └─────────────────────────┬───────────────────────┘
                            │
                            ▼
       ┌─────────────────────────────────────────┐
       │  LPIR                                   │
       │  Memcpy / Load / Store / Copy / Call    │
       │  (no new ops)                           │
       └────┬───────────────┬──────────────┬─────┘
            │               │              │
            ▼               ▼              ▼
   ┌──────────────┐  ┌─────────────┐  ┌──────────┐
   │ lpvm-native  │  │ lpvm-       │  │ lpvm-    │
   │ (cranelift   │  │ cranelift   │  │ wasm     │
   │  host JIT)   │  │ (RV32 cross)│  │          │
   │              │  │             │  │          │
   │  Aggregate   │  │  Aggregate  │  │  Pointer │
   │  args = 1    │  │  args = 1   │  │  args =  │
   │  pointer     │  │  pointer    │  │  i32 into│
   │  each.       │  │  each.      │  │  linear  │
   │  sret hidden │  │  sret hidden│  │  memory. │
   │  arg per agg │  │  arg per agg│  │          │
   │  return.     │  │  return.    │  │          │
   └──────┬───────┘  └─────────────┘  └──────────┘
          │
          ▼
   ┌──────────────────────────────────────────┐
   │  Host call boundary (lpvm_abi.rs, M1)    │
   │                                          │
   │  Aggregates:                             │
   │    LpvmDataQ32::from_value(ty, &v)       │
   │      .as_ptr()           ← in arg        │
   │    LpvmDataQ32::new(ret_ty)              │
   │      .as_mut_ptr()       ← sret arg      │
   │    return data.to_value()?               │
   │                                          │
   │  Scalars / vec / mat: existing flat-     │
   │  scalar marshalling (unchanged)          │
   └──────────────────────────────────────────┘
```

### Crate / file impact summary

```
lp-shader/
├── lps-shared/                       # (small helpers if missing)
├── lps-frontend/
│   ├── lower_ctx.rs                  # M1: AggregateSlot, pointer-arg path
│   │                                 # M2: StructInfo, struct_map
│   │                                 # M3: ArrayInfo leaf=Struct
│   ├── lower_call.rs                 # M1: NEW (or extracted from lower.rs)
│   ├── lower_array.rs                # M1: pointer-ABI rewrite of `in` path
│   │                                 # M3: struct-leaf extensions
│   ├── lower_struct.rs               # M2: NEW
│   ├── lower_expr.rs                 # M2: AccessIndex/Compose/Load for structs
│   ├── lower_stmt.rs                 # M2: whole-struct & member stores
│   ├── lower_access.rs               # M2: store-through-pointer for structs
│   ├── lower.rs                      # M4: load_lps_value_from_vmctx array recursion
│   └── (M5)                          # readonly_in_scan.rs (NEW)
├── lpir/                             # (no changes — all ops already exist)
├── lpvm/src/lpvm_abi.rs              # M1: aggregate cases collapse to LpvmDataQ32
├── lpvm-native/, lpvm-cranelift/,    # M1+: per-milestone calling-convention
│   lpvm-wasm/                        #     validation (per Q7)
└── lps-filetests/                    # filetest @unimplemented markers toggle
                                     # off per milestone acceptance gate
```

## Alternatives considered

- **Two ABIs (arrays flat-scalar, structs by-pointer).** Rejected: forces
  a 3-way fork when arrays-of-structs land, splits backend codegen, splits
  host marshalling.
- **Hybrid by size (small flat, large by-pointer, threshold).** Rejected:
  arbitrary threshold, every call site is type-conditional, perf gain
  evaporates as soon as types mix.
- **Flat-scalar for everything (arrays + structs + arrays-of-structs).**
  Rejected: huge arg lists for non-trivial aggregates (`Particle ps[8]`
  = 88 scalar args), breaks down on RV32 with only 8 arg registers, and
  fights cranelift's natural register allocation.
- **Always-`sret` returns vs small-aggregate-register-return fast path.**
  Roadmap picks always-`sret` for uniformity. Small-struct register-return
  is a real perf win for `Point` / `vec3`-returning functions; explicitly
  deferred to a follow-up perf roadmap.
- **Struct-only plan, defer array migration.** Rejected by the user during
  question iteration: "we need to align array with whatever we do."

## Risks

- **R1 — Cranelift RV32 calling convention for hidden `sret` args.**
  Cranelift handles `sret` natively on x86_64/aarch64; the RV32 backend
  is less battle-tested for our shape. Mitigation: M1 acceptance includes
  round-tripping a struct return on `rv32c.q32` end-to-end.
- **R2 — Existing array filetests rely on flat-scalar IR shape.** Some
  IR-shape `CHECK:` lines in `lps-filetests/` filetests assert against
  the current calling-convention shape. M1 will need to rewrite those
  CHECK lines along with the calling-convention change. Mitigation: bulk
  filetest update is an explicit M1 sub-task with a known shape.
- **R3 — Host-ABI behaviour change for `lpvm_abi::flatten_q32_arg` /
  `decode_q32_return`.** Any existing rust caller that uses the
  flat-scalar API for arrays needs migration to `LpvmDataQ32`. Mitigation:
  scan call sites; should be limited to `lp-shader` internals + a small
  number of test harnesses. Update them in M1.
- **R4 — Tiny-aggregate perf regression in domain workloads** between M1
  and M5. Every aggregate call pays an alloc-slot + Memcpy that today's
  flat-scalar arrays don't. Mitigation: M5 (read-only-`in` optimisation)
  erases this for the common `in` case; explicit benchmarking in M6
  confirms domain workload deltas are acceptable. If they aren't, M5
  priority lifts.
- **R5 — Naga `LocalVariable` shape for "in" struct params.** Naga emits
  `Statement::Store { pointer: LocalVariable, value: FunctionArgument }`
  at function entry to copy arg into local. Today
  `scan_param_argument_indices` aliases the local to arg VRegs; with
  pointer ABI the local becomes a slot, and the entry Store becomes a
  `Memcpy` from arg-pointer into local-slot. Mitigation: this is exactly
  what arrays already do today (`!is_array_val` filter excludes them
  from aliasing); M1 generalises that to all aggregates.
- **R6 — `Compose` of a struct rvalue at a call site**
  (`circle_area(Circle(Point(0,0), 2.0))`) requires materialising a
  temporary slot, filling it via `Compose`, taking its address. The
  temp-slot machinery is straightforward but it's "newish" code — temp
  slots for aggregate rvalues don't exist today. Mitigation: M2 includes
  this as a named sub-task.

## Scope estimate

6 milestones, ~3-5 phases each.

- **M1** is the heaviest (touches frontend, IR call lowering, host ABI,
  three backends, and the array filetest corpus).
- **M2–M4** are layered features on M1's foundation; each is roughly
  half of M1's surface area.
- **M5** is a focused optimisation pass with measurable perf delta.
- **M6** is sweep + docs.
