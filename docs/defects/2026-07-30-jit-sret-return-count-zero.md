# Defect: the JIT's cached sret return count was zero

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `lpvm-native` JIT runtime (`rt_jit/module.rs`) — **device path, both ISAs**
- **Found by:** writing the ESP32-S3 hardware corpus' sret case (M1)
- **Reachability:** latent — no production caller today. See "Why nobody hit it".

## Symptom

None observed in the wild. This was found by reading, while working out why a
corpus case for aggregate returns could not be written against `call_q32`.

Had it been reached: `NativeJitInstance::call_q32` on any function returning
more than two scalars (every `vec3`, `vec4`, `mat*`) would return an **empty**
vector, after the callee wrote its result words **past the end of a one-word
heap allocation**.

## Cause

`NativeJitEntryInfo` derived two fields from sources that disagree:

```rust
ret_count: ir_func.return_types.len(),   // 0 for an sret function
is_sret:   func_abi.is_sret(),           // true
```

`FunctionBuilder::finish` **asserts** that sret functions have empty
`return_types` — the results leave through the caller-provided pointer rather
than in registers, so there is nothing to declare. The real count lives in the
ABI, computed from the signature's return type
(`ReturnMethod::Sret { word_count }`, exposed as `FuncAbi::sret_word_count()`),
and it is the same number the emitter builds the prologue from.

So for a `vec4` entry, `ret_count` was `0` while the callee wrote `4` words.

Two consequences, in `rt_jit/instance.rs`:

- **`invoke_flat`** sizes its buffer `let n_buf = n_ret.max(1)` — a **one-word**
  `Vec` receiving four words, a 12-byte heap overflow — then
  `sret_buf.truncate(n_ret)` discards everything and returns `[]`.
- **`call_direct`** validates `out.len() != handle.ret_count`, so a caller
  passing a correctly-sized four-word buffer is **rejected**, while a caller
  passing an empty one passes validation and is then written through.

## Why nobody hit it

Nothing calls these APIs on an aggregate-returning entry today. Shader entry
points go through `call_render_texture` / `call_render_samples`, which size
their own buffers; the only `call_q32` callers in the tree are `lpvm-wasm`
tests, on a different engine.

It becomes reachable the moment device code calls a `vec3`/`vec4`-returning
function through `call_q32` — and those returns are ubiquitous in real GLSL.
The S3 app-layer work is exactly where that would have happened, which is a bad
place to meet a heap overflow.

## The contrast that confirms it

`rt_emu` — the host emulator engine, in the same crate — has **always** been
correct. `run_emulator_call` sizes the sret buffer from the return type:

```rust
let struct_size = if uses_sret {
    if ir_func.sret_arg.is_some() {
        lps_shared::type_size(rt, LayoutRules::Std430)   // from the RETURN TYPE
    } else {
        ir_func.return_types.len() * 4
    }
} else { 0 };
```

It special-cases `sret_arg.is_some()` precisely because `return_types` is empty
there. The JIT simply never learned the same lesson. This is why the bug was
invisible to every host-side test including the 851-file filetest corpus: those
run on `rt_emu`, and `rt_jit` only exists on `riscv32` / `xtensa` targets.

## Fix

`ret_count` now comes from the ABI, falling back to `return_types` only for
non-sret functions:

```rust
ret_count: func_abi
    .sret_word_count()
    .map_or_else(|| ir_func.return_types.len(), |w| w as usize),
```

`sret_word_count()` already existed and was already unit-tested
(`Some(16)` for `mat4`, `Some(4)` for `vec4`). Nothing new was invented.

## Guards

- `lpvm-native/tests/xt_corpus_hard_cases.rs::sret_return_word_count_does_not_come_from_return_types`
  pins the invariant — asserts that for an sret function the two sources
  **disagree**, and that the ABI's is the four-word answer. Anyone
  "simplifying" `ret_count` back gets a failure that explains itself.
- `lpvm_native::xt_corpus`'s `sret_vec4_return` case now covers risk item 6 of
  the S3 hardware corpus, verified on silicon: `f(7) -> [7,8,9,10]`,
  `f(-1) -> [-1,0,1,2]`.

## Class

**`split-source-of-truth`** — one fact (how many words this function returns)
derived two ways, both used, nothing checking they agree. `ret_count` came from
the IR and `is_sret` came from the ABI, and no test asserted they described the
same function.

`xtensa-sret-pointer-clobber` (same day, PR #194) is **the same bug**:
`FuncAbi::allocatable` computed the withheld sret register and `RegPool`
ignored it. In both cases the producer had a passing unit test asserting its
own opinion; the untested thing was the hand-off. Two in one day is an
architecture signal, not a coincidence.

It is filed here rather than under `config-masked-defect` because that class
describes *how a defect survived*, not what it was — and the masking mechanisms
genuinely differ. Theirs survived a register-layout accident (rv32's argument
registers and pool are disjoint; Xtensa's overlap). This one survived a **`cfg`
boundary**: `rt_jit` compiles only for riscv32/xtensa, so no host test could
execute it, including the 851-file filetest corpus, which runs on `rt_emu`.

See the note in `README.md` on collapsing the two axes if a fourth lands.

## Postscript: the guard was not running either

The tests added alongside this fix (`xt_corpus_goldens`,
`xt_corpus_hard_cases`) initially ran **nowhere** — `test-xt-host` used a
`--test` allowlist they were never added to, and a file-level
`#![cfg(feature = "emu-xt")]` meant that under any other feature set they
compiled to an empty file and reported success having executed nothing.

The allowlist has been removed rather than extended: it had to be maintained to
stay correct, and the cost of forgetting was silence — the same property that
made this defect invisible. `validate-xtensa` is separately gaining a
minimum-test-count assert, so a job cannot pass while running nothing.

Third instance of file-level-cfg-hides-tests in this repo (see
`fw-esp32-harnesses-rotted-uncompiled` and the `stories` note on
`test-studio-host`), which makes it a pattern rather than three mistakes.
