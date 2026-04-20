# Phase 4 — Wire `ProfileSession` into `Riscv32Emulator` + run loop

Replace the existing `alloc_tracer: Option<AllocTracer>` field
on `Riscv32Emulator` with `profile_session: Option<ProfileSession>`,
and route the `SYSCALL_ALLOC_TRACE` syscall through the session
instead of poking the tracer directly.

After this phase, **the alloc collector path is fully alive
through the new infrastructure**, but `mem-profile`/`heap-summary`
CLI commands and `alloc_trace.rs` still exist as dead-but-compiling
code (cleaned up in phase 6).

Depends on phases 1 and 3.

## Subagent assignment

`generalPurpose` subagent. The trickiest phase — borrow-checker
fight in `run_loops::handle_syscall` — but bounded.

## Files to touch

```
lp-riscv/lp-riscv-emu/src/emu/emulator/state.rs       # field + builder/finalizer
lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs   # syscall dispatch
lp-riscv/lp-riscv-emu/src/emu/emulator/mod.rs         # if it re-exports things
```

Anything that constructs a `Riscv32Emulator` and currently chains
`.with_alloc_trace(...)` will be **temporarily broken**. Those
call sites are:

- `lp-cli/src/commands/mem_profile/handler.rs`
- `lp-fw/fw-tests/tests/alloc_trace_emu.rs`

**Strategy:** keep both `with_alloc_trace`/`finish_alloc_trace`
AND the new `with_profile_session`/`finish_profile_session`
during this phase. Internally `with_alloc_trace` constructs a
`ProfileSession` containing one `AllocCollector`; `finish_alloc_trace`
delegates to `finish_profile_session` and returns the alloc
collector's event count via the `event_count()` getter exposed
in phase 3. This keeps the build green for the old call sites
through phases 5 and 6, and lets us delete the wrappers in phase 6
once nothing calls them.

## Steps

### 1. `state.rs`: replace the field

Find the struct definition. Replace:

```rust
alloc_tracer: Option<AllocTracer>,
```

with:

```rust
profile_session: Option<ProfileSession>,
```

Update `Default` impl and any constructors accordingly.

### 2. `state.rs`: add new builder + finalizer

```rust
impl Riscv32Emulator {
    pub fn with_profile_session(
        mut self,
        trace_dir: PathBuf,
        metadata: &SessionMetadata,
        collectors: Vec<Box<dyn Collector>>,
    ) -> std::io::Result<Self> {
        self.profile_session = Some(
            ProfileSession::new(trace_dir, metadata, collectors)?
        );
        Ok(self)
    }

    pub fn finish_profile_session(&mut self) -> std::io::Result<u64> {
        match self.profile_session.as_mut() {
            Some(s) => s.finish(),
            None => Ok(0),
        }
    }
}
```

### 3. `state.rs`: keep old wrappers temporarily

```rust
impl Riscv32Emulator {
    /// Deprecated shim, removed in phase 6.
    pub fn with_alloc_trace(
        self,
        trace_dir: PathBuf,
        // ... existing args (heap_start, heap_size, symbols, etc.)
    ) -> std::io::Result<Self> {
        let metadata = SessionMetadata { /* assemble from args */ };
        let alloc = Box::new(AllocCollector::new(&trace_dir, heap_start, heap_size)?);
        self.with_profile_session(trace_dir, &metadata, vec![alloc])
    }

    /// Deprecated shim, removed in phase 6.
    pub fn finish_alloc_trace(&mut self) -> std::io::Result<u64> {
        // The alloc collector is the first (only) collector; pull its
        // event count before/after finish for the existing return value.
        // Easiest: have ProfileSession expose
        // `collector::<AllocCollector>()` or similar; OR just compute
        // it from heap-trace.jsonl line count. Pick whatever is least
        // invasive.
        self.finish_profile_session()
    }
}
```

Exact signature of the old `with_alloc_trace` may differ — match
whatever the existing call sites use. The point is: existing
callers compile unchanged.

### 4. `run_loops.rs`: rewire `SYSCALL_ALLOC_TRACE`

Find the existing match arm. Replace its body with the dispatch
shown in the design doc:

```rust
SYSCALL_ALLOC_TRACE => {
    if let Some(session) = self.profile_session.as_mut() {
        let mut ctx = EmuCtx {
            pc: self.pc,
            regs: &self.regs,
            cycle_count: self.cycle_count,
            instruction_count: self.instruction_count,
            memory: &self.memory,
        };
        let action = session.dispatch_syscall(
            &mut ctx,
            SYSCALL_ALLOC_TRACE,
            &syscall_info.args.map(|a| a as u32),
        );
        match action {
            SyscallAction::Pass => { /* fall through */ }
            SyscallAction::Handled => {
                self.regs[Gpr::A0.num() as usize] = 0;
                return Ok(StepResult::Continue);
            }
            SyscallAction::Halt(HaltReason::Oom { size }) => {
                return Ok(StepResult::Oom(OomInfo { size, pc: self.pc }));
            }
        }
    }
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
}
```

#### Borrow-checker note

`self.profile_session.as_mut()` borrows `self` mutably; building
`EmuCtx` then borrows `&self.pc`, `&self.regs`, `&self.memory`
immutably. This is the aliasing problem the design doc mentions.

Two options, pick whichever is least disruptive:

a. **Destructure-then-build**: pull the needed fields into
   locals before grabbing the session.

   ```rust
   let pc = self.pc;
   let regs_ptr = &self.regs as *const _;        // careful
   ```

   (Raw pointers are gross; prefer option (b).)

b. **Split borrow via fields**: rewrite as

   ```rust
   let Self {
       ref mut profile_session,
       pc, regs, cycle_count, instruction_count, ref memory, ..
   } = *self;
   if let Some(session) = profile_session.as_mut() {
       let mut ctx = EmuCtx { pc, regs, cycle_count, instruction_count, memory };
       ...
   }
   ```

   This works because Rust's borrow checker tracks fields of
   `self` separately when destructured.

c. **Helper on `EmuCtx`**: add a constructor
   `EmuCtx::from_emu_view(pc, regs, cycle, ic, mem)` and inline
   the call. Same effect as (b), cleaner.

If options (b)/(c) still fight the borrow checker because of how
`run_loops` is structured (e.g. there's a `Vec<u32>::from(args)`
call that borrows `self.memory`), consider extracting the syscall
dispatch into a free function:

```rust
fn dispatch_alloc_syscall(
    profile_session: &mut Option<ProfileSession>,
    pc: u32, regs: &[i32; 32], cycle_count: u64, instruction_count: u64, memory: &Memory,
    args: &[u32],
) -> SyscallAction { ... }
```

and call it as `dispatch_alloc_syscall(&mut self.profile_session, self.pc, &self.regs, ..., &args)`.
This is the cleanest if (b) doesn't compile.

### 5. Verify call sites

`lp-cli/src/commands/mem_profile/handler.rs` and
`lp-fw/fw-tests/tests/alloc_trace_emu.rs` should still compile
without modification because of the shim builders kept in step 3.

## Validation

```bash
cargo check -p lp-riscv-emu
cargo build -p lp-riscv-emu
cargo test -p lp-riscv-emu

# Existing end-to-end test still passes via the new infrastructure
cargo test -p fw-tests --test alloc_trace_emu

# Existing CLI command still works
cargo run -p lp-cli -- mem-profile examples/basic
ls profiles/  # should now show output here? — depends on shim
              # behavior. If it goes to traces/ still, that's because
              # the OLD CLI handler hard-codes traces/. Phase 5 fixes.
```

## Out of scope for this phase

- New `lp-cli profile` command (phase 5).
- Deleting old code (phase 6).
- Renaming files (phases 6–7).
