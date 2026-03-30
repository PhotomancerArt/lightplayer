# JIT Filetest Segfault on Multiple Test Files

## Summary

Running multiple GLSL filetests with the JIT backend causes a segmentation fault. The crash occurs after all test results are printed, during process shutdown.

## Affected Command

```bash
scripts/glsl-filetests.sh 'array/*.glsl' --fix
```

## Symptoms

- Exit code 139 (SIGSEGV)
- All test output completes normally before crash
- Crash occurs after summary line, before clean process exit
- Single file tests pass; multiple files fail

## Reproduction

### Fails (multiple files with JIT)
```bash
scripts/glsl-filetests.sh 'array/constructor-explicit.glsl' 'array/constructor-inferred.glsl'
# Output shows all test results, then:
# scripts/glsl-filetests.sh: line 256: NNNNN Segmentation fault: 11
```

### Works (single file with JIT)
```bash
scripts/glsl-filetests.sh 'array/constructor-explicit.glsl'
# Completes cleanly, exit code 0 or 1
```

### Works (multiple files with WASM)
```bash
scripts/glsl-filetests.sh --target wasm.q32 'array/constructor-explicit.glsl' 'array/constructor-inferred.glsl'
# Completes cleanly
```

### Fails (multiple files even with single thread)
```bash
LP_FILETESTS_THREADS=1 scripts/glsl-filetests.sh 'array/constructor-explicit.glsl' 'array/constructor-inferred.glsl'
# Still segfaults
```

## Observed Facts

1. Crash happens **after** all test execution and output is complete
2. Crash happens during multi-file runs, not single-file runs
3. WASM backend is unaffected
4. Single-threaded mode (`LP_FILETESTS_THREADS=1`) still crashes
5. Exit code 139 indicates SIGSEGV (memory access violation)

## Hypotheses (Unverified)

### Hypothesis 1: JIT Module Drop (Most Likely)
Each test file creates a `Box<dyn GlslExecutable>` containing a `JitModule`. When the worker thread scope ends, these are dropped. The crash may occur in:
- `JITModule::drop()` (cranelift-jit internal cleanup)
- `AllocJitMemoryProvider::free_memory()` (custom memory provider)
- Cranelift's internal global state cleanup

Evidence:
- WASM works (no JIT modules involved)
- Crash is after test execution (when resources are freed)
- Prior art: `process_sync.rs` comment mentions "cranelift_jit finalization crashes"

### Hypothesis 2: Thread Cleanup
The crash may occur during worker thread exit, specifically:
- Thread-local storage destruction
- Stack unwinding with JIT code pointers in registers
- Channel sender/receiver cleanup race

Evidence against:
- Single-threaded mode still crashes

### Hypothesis 3: Global Static State
Cranelift JIT may maintain global state that gets corrupted after multiple module finalizations. The crash occurs when:
- Second JIT module is created (but output suggests tests ran)
- Process exit handlers run
- Global allocator state is corrupted

## Technical Context

### Execution Flow
```
main thread                    worker thread
     |                               |
     |---- Request (test file) ----->|
     |                               |
     |                          compile -> JitModule
     |                               |
     |                          run tests
     |                               |
     |                          JitModule dropped
     |                               |
     |<--- Reply (results) ----------|
     |                               |
     |---- shutdown() -------------->|
     |                          recv() fails, loop breaks
     |                          thread exits
     |                               |
join() waits for thread
CRASH HAPPENS HERE
```

### Relevant Code
- `lpir-cranelift/src/jit_module.rs` - `JitModule` struct and `build_jit_module()`
- `lpir-cranelift/src/jit_memory.rs` - `AllocJitMemoryProvider`
- `lp-glsl-filetests/src/runner/concurrent.rs` - Worker thread lifecycle
- `lpir-cranelift/src/process_sync.rs` - Codegen serialization (existing lock)

## Workarounds

### Option 1: Use WASM Backend (Recommended for bulk operations)
```bash
scripts/glsl-filetests.sh --target wasm.q32 'array/*.glsl' --fix
```

### Option 2: Iterate Single Files
```bash
for f in array/*.glsl; do
    scripts/glsl-filetests.sh "$f" --fix || true
done
```

### Option 3: Use Smaller Batches
```bash
scripts/glsl-filetests.sh 'array/phase/*.glsl' --fix
```

## Investigation Needed

To pinpoint the crash location:

1. **Run with lldb** to get backtrace:
   ```bash
   lldb -- scripts/glsl-filetests.sh 'array/*.glsl'
   # At lldb prompt: run, then after crash: bt
   ```

2. **Add instrumentation** in `jit_module.rs`:
   - Print in `JitModule` drop
   - Print in `AllocJitMemoryProvider::free_memory()`

3. **Test with process-per-file**:
   Modify runner to spawn a new process per test file instead of threads

4. **Check cranelift-jit fork**:
   Review any custom changes in `https://github.com/light-player/lp-cranelift` that might affect JIT memory management

## Impact

- **Severity**: Medium (workarounds exist, not blocking)
- **Affected**: Developer workflow with JIT backend and multiple files
- **Not Affected**: CI (uses WASM), single-file testing, embedded targets

## References

- Cranelift JIT fork: `https://github.com/light-player/lp-cranelift` (branch: main, version 0.127.0)
- Related comment in `lpir-cranelift/src/process_sync.rs`:
  > "Concurrent `cranelift_jit` finalization and/or object emission has produced process crashes (SIGSEGV)"
