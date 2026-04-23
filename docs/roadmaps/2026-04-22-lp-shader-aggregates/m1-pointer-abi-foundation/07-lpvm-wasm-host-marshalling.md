# P7 — `lpvm-wasm`: host marshalling for aggregate ABI

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q5 records the decision: bump
the exported `$sp` shadow stack from the host).
Depends on: P1 (LPIR sret marker), P3 (frontend).
Parallel with: P4 (`lpvm-cranelift`), P5 (`lpvm-native`),
P6 (`lpvm-emu`).

## Scope of phase

Update the wasm host runtimes (`rt_wasmtime` and `rt_browser`) so
aggregate args / returns marshal through pointers in WASM linear memory,
allocated on the module's existing shadow stack via the exported `$sp`
global. Codegen for the wasm callee already lowers pointer params and
memory ops naturally — this phase is purely host-side.

Concretely:

- For aggregate args: the host bumps `$sp` down by `sized + padding`,
  writes the bytes via `Memory::data_mut(...)`, passes
  `Val::I32(ptr)`. After the call, restore `$sp`.
- For aggregate returns: same alloc + `Val::I32(ptr)` as the hidden
  first arg; after the call, read the bytes back and decode via
  `LpvmDataQ32::from_bytes`.
- The WASM "shadow stack" already has helpers in
  `lp-shader/lpvm-wasm/src/emit/memory.rs` (`SHADOW_STACK_BASE`,
  `FRAME_ALIGN`, `emit_shadow_prologue`, `emit_shadow_epilogue`). The
  host uses the same `$sp` global the prologue/epilogue manipulate.

**Out of scope:**

- WASM codegen — pointer params already work; aggregate Memcpy ops
  already lower.
- Other backends.
- Filetest CHECK rewrites (P9).

## Code organization reminders

- Keep changes scoped to `lp-shader/lpvm-wasm/src/rt_wasmtime/` and
  `lp-shader/lpvm-wasm/src/rt_browser/`.
- Mirror the same shape across both runtimes; share helpers if the
  current code already shares them.
- Place new helpers near the existing aggregate-marshalling ones in
  `marshal.rs`.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lpvm-wasm/`.
- Do **not** suppress warnings or add `#[allow(...)]`.
- Do **not** weaken or `#[ignore]` tests.
- If the module's `$sp` global isn't exported (or has a different
  name), **stop and report** — adding the export is a codegen change
  outside this phase's scope.
- If the bump-allocator strategy interacts badly with concurrent calls
  (e.g. host runs two calls in parallel into the same instance), stop
  and report. M1's contract is single-threaded host-driven calls; don't
  invent a new concurrency model.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Inventory

- `lp-shader/lpvm-wasm/src/emit/memory.rs` — confirm shadow-stack
  globals (`$sp`), `SHADOW_STACK_BASE`, `FRAME_ALIGN`. Confirm `$sp` is
  exported (named `__lp_sp` or similar). If not exported, **stop and
  report**.
- `lp-shader/lpvm-wasm/src/rt_wasmtime/marshal.rs` and
  `rt_wasmtime/instance.rs` — current host marshalling (which today
  flattens aggregates into multiple `Val`s via `lpvm_abi::flatten_q32_arg`).
- `lp-shader/lpvm-wasm/src/rt_browser/marshal.rs` and
  `rt_browser/instance.rs` — same shape, different runtime.

### 2. Shadow-stack alloc helper

Add a helper that bumps `$sp` and returns the allocated guest pointer:

```rust
// rt_wasmtime/marshal.rs (sketch)

pub(crate) struct ShadowFrame {
    saved_sp: i32,
}

pub(crate) fn shadow_frame_open(
    store: &mut Store<HostCtx>,
    inst: &Instance,
) -> wasmtime::Result<ShadowFrame> {
    let sp = inst.get_global(&mut *store, "__lp_sp")
        .ok_or_else(|| anyhow!("module missing exported $sp"))?;
    let cur = sp.get(&mut *store).i32().unwrap();
    Ok(ShadowFrame { saved_sp: cur })
}

pub(crate) fn shadow_alloc(
    store: &mut Store<HostCtx>,
    inst: &Instance,
    size: u32,
    align: u32,
) -> wasmtime::Result<i32> {
    let sp = inst.get_global(&mut *store, "__lp_sp").unwrap();
    let cur = sp.get(&mut *store).i32().unwrap();
    let aligned = (cur - size as i32) & !(align as i32 - 1);
    sp.set(&mut *store, Val::I32(aligned))?;
    Ok(aligned)
}

pub(crate) fn shadow_frame_close(
    store: &mut Store<HostCtx>,
    inst: &Instance,
    frame: ShadowFrame,
) -> wasmtime::Result<()> {
    let sp = inst.get_global(&mut *store, "__lp_sp").unwrap();
    sp.set(&mut *store, Val::I32(frame.saved_sp))
}
```

(Use whatever the existing global name is — `__lp_sp`, `lp_sp`, or
`$sp`. Search `emit/memory.rs` to confirm.)

Bump direction: down (high → low) is the convention used by most
shadow-stack runtimes; if the existing prologue/epilogue uses the
opposite direction, match that.

### 3. Aggregate arg marshalling

For each aggregate `in` arg (host has an `LpvmDataQ32`):

```rust
let bytes = data.as_bytes();
let size = bytes.len() as u32;
let align = data.alignment() as u32;
let ptr = shadow_alloc(&mut store, &inst, size, align)?;
let mem = inst.get_memory(&mut store, "memory").unwrap();
mem.data_mut(&mut store)[ptr as usize..(ptr as usize + size as usize)]
    .copy_from_slice(bytes);
call_args.push(Val::I32(ptr));
```

For `out`/`inout` aggregates: alloc + (for `inout`) write + after the
call read back into the host's `LpvmDataQ32`.

### 4. Aggregate return (sret)

```rust
let size = lps_shared::layout::type_size(&ret_lps_ty, LayoutRules::Std430) as u32;
let align = lps_shared::layout::type_alignment(&ret_lps_ty, LayoutRules::Std430) as u32;
let ret_ptr = shadow_alloc(&mut store, &inst, size, align)?;
// Insert ret_ptr as the hidden first arg (after vmctx if present).
call_args.insert(/* index-of-first-user-arg */, Val::I32(ret_ptr));

// Run the call. The wasm callee writes the return value into [ret_ptr..ret_ptr+size).

let mem = inst.get_memory(&mut store, "memory").unwrap();
let bytes = &mem.data(&store)[ret_ptr as usize..(ret_ptr as usize + size as usize)];
let dest = LpvmDataQ32::from_bytes(ret_lps_ty.clone(), LayoutRules::Std430, bytes.to_vec())?;
```

Wrap arg+ret marshalling in `shadow_frame_open` / `shadow_frame_close`
so all bumped allocations are released on call exit (single restore of
`$sp`).

### 5. Trigger source

Replace any "scalar return count > N → flatten across multiple Vals"
heuristic with "the LPIR signature has sret → allocate one i32 ptr
arg". The `ImportDecl::sret` and `IrFunction::sret_arg` markers (P1)
are the source of truth.

### 6. Browser runtime

Mirror the same logic in `rt_browser/`. The browser runtime uses
`web_sys` / wasm-bindgen rather than wasmtime; the global / memory
access APIs differ but the conceptual flow (open frame → alloc →
write → call → (for sret) read → close frame) is identical. Share
helpers where structure permits.

### 7. Tests

If the wasm runtime crates have unit/integration tests using small
hand-built modules or compiled GLSL, add a round-trip:

- A wasm module exporting a function with `in float[4] -> float[4]`
  (sret), implementing element-wise `* 2.0`.
- Host call with `LpvmDataQ32([1,2,3,4])` → assert returned
  `LpvmDataQ32([2,4,6,8])`.

If the existing harness can't easily build that yet, add a TODO and
defer to P9.

## Validate

```
cargo check -p lpvm-wasm
cargo test  -p lpvm-wasm
just test-glsl
```

Filetest-level wasm.q32 failures may exist until P9.

## Done when

- Shadow-stack alloc helper exists and is used by both
  `rt_wasmtime` and `rt_browser`.
- Aggregate args use `Val::I32(ptr)` after writing bytes to memory.
- Aggregate returns alloc on the shadow stack, are passed as sret,
  and are decoded via `LpvmDataQ32::from_bytes`.
- Trigger source for sret is the LPIR marker (no heuristic).
- `cargo test -p lpvm-wasm` is green.
- `just check` is green for this crate.
- No new `#[allow(...)]`; no `#[ignore]`d tests.
