# Native Runtime Memory Savings Notes

## Scope of work

Plan a focused pass to reduce runtime memory use in the native shader compile and JIT execution path, especially for on-device warm-up / background compilation on ESP32-C6.

Primary focus:
- reduce transient compile-time heap on the native backend hot path
- reduce retained post-compile runtime heap for JIT modules
- keep host/filetest/debug workflows working, but move rich debug retention out of the default runtime path where possible
- preserve the on-device GLSL -> LPIR -> native code product path

Out of scope for this plan unless needed as an enabler:
- changing shader language semantics
- replacing the native backend architecture
- major latency-oriented frontend slicing work beyond note-taking
- removing useful host-side debug capabilities from filetests / emu / asm tools

## Current state

### Recent measured result

A targeted ESP32 stress harness for incremental compile on `examples/basic` produced two useful measurements:

1. Before dropping per-function intermediates, peak used heap reached about `237,620` bytes near the end of backend compilation.
2. After changing `FunctionCompileState` / `NativeCompileJob` to release per-function intermediates immediately after function finalization, peak used heap dropped to about `109,528` bytes.

This strongly suggests that retained intermediate backend state was the dominant transient memory problem.

### Backend compile state

Relevant files:
- `lp-shader/lpvm-native/src/compile.rs`
- `lp-shader/lpvm-native/src/compile/function_job.rs`
- `lp-shader/lpvm-native/src/compile/module_job.rs`

The native backend currently compiles functions through explicit stages:
- const fold
- lower
- peephole
- regalloc
- emit
- debug/finalize

Recent work already improved memory by dropping per-function intermediates once a `CompiledFunction` is produced and pushed into `completed_functions`.

### Runtime debug payload is still built into compiled functions

`CompiledFunction` currently contains:
- `code: Vec<u8>`
- `relocs: Vec<NativeReloc>`
- `debug_lines: Vec<(u32, Option<u32>)>`
- `debug_info: FunctionDebugInfo`

Source:
- `lp-shader/lpvm-native/src/compile.rs`

`compile_function_debug_sections(...)` always builds structured debug sections from optimized IR, lowered vinsts, emitted code, alloc output, and ABI context, then stores them in `CompiledFunction.debug_info`.

This is likely appropriate for host-side debugging and filetests, but it is suspicious on the default firmware hot path.

### JIT link path still builds module debug info even when runtime does not use it

Source:
- `lp-shader/lpvm-native/src/rt_jit/compiler.rs`

`link_compiled_module_jit(...)` currently:
1. iterates all compiled functions
2. clones each `func.debug_info`
3. builds a `ModuleDebugInfo`
4. returns `(JitBuffer, entry_offsets, ModuleDebugInfo)`

But the JIT engine currently ignores that returned debug info:
- `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- `lp-shader/lpvm-native/src/rt_jit/compile_job.rs`

This suggests a likely unnecessary runtime allocation and clone path.

### JIT runtime module retains full IR and module signature

Source:
- `lp-shader/lpvm-native/src/rt_jit/module.rs`
- `lp-shader/lpvm-native/src/rt_jit/engine.rs`
- `lp-shader/lpvm-native/src/rt_jit/compile_job.rs`

`NativeJitModuleInner` currently stores:
- `ir: LpirModule`
- `meta: LpsModuleSig`
- `buffer: JitBuffer`
- `entry_offsets: BTreeMap<String, usize>`
- `options`
- `isa`

`direct_call(...)` uses retained `ir` and `meta` to derive function ABI and counts lazily by name.

This is convenient, but likely keeps a large amount of metadata resident after compile. It looks like a good candidate for replacement with a smaller retained per-entry summary.

### Debug-oriented runtime paths that likely should stay

Some debug retention is still legitimately useful outside the firmware hot path:
- `rt_emu` keeps `ModuleDebugInfo` and exposes `debug_info()` for emulator debugging and filetests
- `debug_asm.rs` explicitly enables `NativeCompileOptions { debug_info: true, .. }`

So the likely direction is not “delete debug support”, but rather “make runtime JIT opt out by default, while host/debug tools opt in”.

### Potential remaining duplicate buffer path

`rt_jit/compiler.rs` still appears to hold at least these stages in sequence:
- `CompiledFunction.code` per function
- linked module `linked.code`
- final `JitBuffer::from_code(linked.code)`

It is not yet clear whether this is a major peak contributor after the intermediate-release fix, but it remains a credible savings candidate.

## Open questions

### 1. Should runtime JIT retain any structured debug info by default?

Context:
- the user explicitly said debug data should not remain on the hot path
- the current JIT runtime path appears to build `ModuleDebugInfo` and function-level debug structures even though the JIT engine discards them

Suggested answer:
- No. The default JIT runtime path should not build or retain structured debug sections by default.
- Rich debug info should be opt-in for filetests, emulator workflows, assembly/debug tools, or explicit profiling/debug modes.

### 2. Is `debug_lines` still worth keeping in the default runtime compile result?

Context:
- `debug_lines` is smaller than full `FunctionDebugInfo`, but it is still debug-oriented metadata
- `debug_asm` and disassembly tools use it; runtime JIT may not need it

Suggested answer:
- Probably not on the default runtime JIT path.
- Keep it available behind the same debug-oriented opt-in as structured debug sections, unless a concrete runtime consumer exists.

### 3. Should `NativeJitModuleInner` stop retaining full `LpirModule` and `LpsModuleSig`?

Context:
- these are likely resident-memory costs after compile
- `direct_call(...)` appears to be the main reason they are retained

Suggested answer:
- Yes. Replace full retained IR/meta with a compact per-entry runtime summary sufficient for `direct_call`, instantiation, and signatures.
- Preserve any trait/API requirements carefully; if `signatures()` must still return a full `LpsModuleSig`, we may need a smaller intermediate step or a lightweight retained form.

### 4. Should link/JIT-image duplication be part of this plan or deferred?

Context:
- duplicate code buffers are a plausible transient-memory win
- but the first two savings above are clearer and lower-risk

Suggested answer:
- Include it in the plan as a measured optimization phase after debug-retention trimming.
- We should instrument and verify whether the remaining peak is meaningfully affected before doing invasive buffer-streaming changes.

### 5. Should latency work on the frontend `58ms` spike be in scope here?

Context:
- user said it would be nice to break up the `58ms` frontend slice, but is not especially worried as long as it stays under about `100ms`
- this plan request is specifically about gathering memory/runtime savings

Suggested answer:
- No. Record it as adjacent follow-up work, but keep this plan focused on memory and retained-runtime overhead.

## Notes from the user

- On-device GLSL JIT is the product; do not gate it out or move compile off-device.
- This work is in service of future playlist / visual warm-up behavior.
- Memory is likely the real constraint, more than CPU.
- Debug info should not be kept on the hot runtime path; it should be for filetests and host-side work.
- Latency is secondary here; the main ugly outlier is a `58ms` frontend step, but the user is not asking to prioritize that in this plan.

## Candidate savings gathered so far

1. Already done: release backend per-function intermediates immediately after function finalization.
2. Stop building / cloning `FunctionDebugInfo` and `ModuleDebugInfo` on the default JIT runtime path.
3. Stop retaining `debug_lines` on the default JIT runtime path unless a concrete runtime consumer appears.
4. Trim `NativeJitModuleInner` retained metadata by replacing `ir + meta` with a smaller runtime summary where possible.
5. Investigate and possibly reduce transient duplicate code/image buffers during link -> JIT buffer creation.
6. Add targeted validation/profiling so each claimed savings can be measured on ESP32 stress harnesses and, where useful, in emulator/profile tooling.
