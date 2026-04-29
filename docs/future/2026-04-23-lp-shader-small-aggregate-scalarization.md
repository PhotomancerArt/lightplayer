# Future idea: scalarize small aggregates in the GLSL → LPIR frontend

## Status: not planned (note only)

We intentionally keep aggregates **memory-shaped** in the frontend: stack slots,
hidden `sret` pointers, `Memcpy` for by-value `in` parameters, and uniform
loads through VMContext. That keeps one coherent story across structs, arrays,
and arrays-of-structs, and matches the std430 mental model for data layout.

This document records a **possible** follow-on: for **small** aggregates, lower
to **multiple scalar/vector VRegs** (and multi-return LPIR) instead of a single
slot, where it is legal and profitable.

No commitment to build this; it is easy to underestimate ABI and correctness
surface area.

## What “scalarization” would mean

**Today (simplified):**

- Struct / array locals → `SlotId` + `AggregateInfo`, member access → `Load`/`Store` + offsets.
- By-value aggregate parameters → caller passes a pointer; callee often copies into a slot (or, after M5 read-only `in`, loads through that pointer).
- Aggregate return → `sret`-style hidden pointer; callee `Memcpy`s the result.

**Hypothetical optimization:**

- For types under a **size / complexity threshold** (e.g. a `vec2`, a struct of
  two floats, maybe a `vec4`), represent the value as **`N` flat `IrType`
  lanes** in `local_map` / `arg_vregs` / return tuple, with **no slot** for the
  aggregate as a whole.
- `AccessIndex` / member reads become **no-op projection** (reuse the right
  VRegs) or cheap shuffles, not memory traffic.
- Calls that pass or return such values might use **multi-value** calling
  conventions on backends that support them, or still spill to a tiny stack
  temporary only at boundaries—policy TBD.

## Where it could apply

| Site | Benefit | Pain |
| ---- | ------- | ---- |
| **Locals** | Avoid slot + load/store for hot temporaries | Must not regress correctness for assignments, copies, and equality |
| **By-value `in` parameters** | Avoid `Memcpy` / slot when not read-only-elided | Must align with host marshalling and every backend’s argument ABI |
| **Returns** | Avoid `sret` + memcpy for tiny structs | Return ABI already delicate; multi-return must match Cranelift / native / wasm |
| **Call arguments** | Avoid materializing slot for rvalue struct literal | Interaction with `store_lps_value_into_slot` and tail calls |

Arrays are a harder fit than “flat” structs: only **very** small fixed arrays
(e.g. 2–4 scalars) would be candidates, and dynamic indexing usually forces
memory anyway.

## Why we are not doing this now

- **Uniform lowering:** One layout authority (`LpsType`, std430) and slot-based
  aggregates keeps uniform, private, and parameter paths easier to reason about.
- **Correctness first:** Member offsets, padding, and nested aggregates already
  consumed significant design time; scalarization duplicates similar decisions in
  “value” vs “memory” form.
- **ABI sprawl:** `lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`, and host
  marshalling would each need a clear rule for “this function returns 3× F32 in
  registers” vs sret—easy to get subtly wrong across targets.
- **Diminishing returns:** M5-style “read-only `in`” removes one memcpy; backend
  optimizers may already fold some stack traffic once shapes are stable.

## When it might become worth revisiting

- Profiles show **struct copy / sret / small slot traffic** in hot shaders after
  other wins (inlining, M5, etc.) are exhausted.
- A **narrow** contract: e.g. “scalarize only `struct { float; float; }` and
  `vec2` locals in non-address-taken paths” with explicit opt-in or file-level
  flag.
- **Cranelift / WASM multi-value** maturity makes multi-return less exotic.

## Related work

- **M5** (`m5-readonly-in-optimisation.md`): reduces `Memcpy` for read-only
  by-value `in` aggregates without changing the “one pointer in, slot or not”
  model.
- Roadmap “small-struct register-return fast path” called out as **out of scope**
  for aggregate milestones—this note is that idea in slightly broader form.

## Revisit checklist (if someone picks this up)

- Define **exact** type and escape predicates (address taken, passed to opaque
  call, written through pointer).
- Specify **per-backend** lowering for multi-value returns and wide arguments.
- Prove **bit-identical** against current lowering on the filetest corpus.
- Add perf benchmarks on **real** shaders, not only microcases.
