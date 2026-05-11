# LPIR Loop Continuing Section

## Design

### Scope

Add a `continuing` section to LPIR loops so that `Continue` can target
increment/condition code (like Naga/WASM `for` loops) rather than always
jumping to the loop body start.

### Current state

- `LoopStart { end_offset: u32 }` — only field is the exit offset.
- `Continue` in the interpreter jumps to `head + 1` (first op after `LoopStart`).
- `End` for a loop jumps to `head + 1` (re-enter body from the top).
- Builder: `push_loop()` / `end_loop()` — no continuing concept.
- Parser: `loop {` … `}` — no continuing label.
- Printer: prints `loop {` … `}` — no continuing label.
- Validator: checks break/continue/br_if_not are inside a loop; no
  continuing-specific validation.

### Proposed change

Extend `LoopStart` with `continuing_offset`:

```
LoopStart { continuing_offset: u32, end_offset: u32 }
```

Layout in the op stream:

```
LoopStart { continuing_offset, end_offset }   ← pc = loop_start
    ... body ops ...
continuing:                                    ← pc = continuing_offset
    ... continuing ops ...
    br_if_not v_cond                           ← optional break_if
End                                            ← jumps back to loop_start + 1
```

Semantics:
- When no continuing section exists: `continuing_offset = loop_start + 1`
  (body start), preserving current behavior exactly.
- `Continue` → jump to `continuing_offset` (instead of `head + 1`).
- `End` for a loop → jump to `head + 1` (body start, unchanged).
- `Break` / `BrIfNot` → unchanged (exit to `end_offset`).

### Questions

#### Q1: Text format for `continuing:`

The continuing label needs a text representation inside `loop { }`.

**Proposed:** A bare `continuing:` label on its own line, at the same indent
level as the loop body. The parser recognizes it and calls
`push_continuing()` on the builder.

```
loop {
  v0 = iadd_imm v0, 1
  v1 = ilt_s v0, v10
  br_if_not v1
  continuing:
  v2 = iadd_imm v2, 1
}
```

**Answer:** Yes, bare `continuing:` label. Parser calls `push_continuing()`.
Printer emits it only when `continuing_offset != loop_start + 1`. Existing
loops are unaffected.

#### Q2: Should `Continue` inside the continuing section be an error?

WASM prohibits branching to the loop header from within the continuing block.
Naga doesn't allow `Continue` in continuing either. We could validate this.

**Proposed:** Yes, validate that `Continue` does not appear lexically inside
the continuing section. The validator already walks the op stream with a
stack — it can track whether we're in a continuing section.

**Answer:** Yes. `Continue` targeting the enclosing loop is forbidden inside
that loop's continuing section. `Continue` targeting an *inner* loop (nested
inside continuing) is fine — it targets the inner loop, not the outer one.

#### Q3: Should `Break` inside continuing be allowed?

`break_if` at the end of the continuing section is the primary use case
(Naga emits this for `for` loop conditions). Explicit `Break` inside
continuing should also work — it's just an early exit.

**Proposed:** Allow `Break` and `BrIfNot` inside continuing (they work the
same as in the body). Only `Continue` is disallowed.

**Answer:** Yes, `Break` and `BrIfNot` in continuing are allowed (that's
how `break_if` works). Only `Continue` targeting the enclosing loop is
disallowed.

### Invariants

Full set of invariants the implementation must enforce:

#### Offset bounds (validator)

1. `continuing_offset >= loop_start + 1` — can't point before the body.
2. `continuing_offset < end_offset` — must point within the loop (End is
   at `end_offset - 1`, so `continuing_offset` can be at most
   `end_offset - 1`, pointing at End = empty continuing, which is
   equivalent to no continuing section).

These follow the same pattern as `IfStart`'s `else_offset`/`end_offset`.

#### Semantic default

3. `continuing_offset == loop_start + 1` means "no continuing section."
   `Continue` jumps to body start, preserving current behavior exactly.
   Builder default. Printer elides the `continuing:` label.

#### Continue restriction (validator)

4. If `continuing_offset > loop_start + 1`, then `Continue` must not
   appear at any position in `[continuing_offset, end_offset - 1)` that
   targets this loop. Targeting a nested inner loop is fine — the
   validator tracks nesting depth and only checks the innermost loop.

   Rationale: Continue-to-continuing would skip the body forever
   (infinite loop in the continuing section). WASM and Naga both
   prohibit this.

#### Break / BrIfNot in continuing

5. `Break` and `BrIfNot` are allowed in the continuing section. They
   exit the loop, same as in the body. This is how `break_if` works.

#### Interpreter semantics

6. `Continue` → jump to `continuing_offset` of the innermost loop
   (instead of `head + 1`). When no continuing section,
   `continuing_offset == head + 1`, so behavior is identical.
7. `End` for a loop → jump to `head + 1` (body start, unchanged).
   After continuing runs, `End` re-enters the body for the next
   iteration.
8. `Break` / `BrIfNot` → jump to `end_offset` (unchanged).

#### Builder

9. `push_loop()` emits `LoopStart { continuing_offset: 0, end_offset: 0 }`.
   `end_loop()` patches `end_offset` and, if `push_continuing()` was
   never called, patches `continuing_offset = loop_start + 1`.
10. `push_continuing()` patches `continuing_offset` to the current
    position on the open `LoopStart`. Panics if called outside a loop
    or called twice.

#### Parser / Printer

11. Parser: `continuing:` line inside a loop body calls
    `push_continuing()`.
12. Printer: emits `continuing:` when `continuing_offset > loop_start + 1`.
    Elided otherwise (existing loops print identically).

### File structure

```
lpir/src/
├── op.rs                  # UPDATE: LoopStart gains continuing_offset
├── interp.rs              # UPDATE: Ctrl::Loop gains continuing, Continue reads it
├── builder.rs             # UPDATE: push_continuing(), end_loop() patches default
├── parse.rs               # UPDATE: recognize `continuing:` line
├── print.rs               # UPDATE: emit `continuing:` when offset differs
├── validate.rs            # UPDATE: offset bounds + Continue-in-continuing check
└── tests/
    ├── interp.rs          # UPDATE: for-loop-style tests with continuing
    ├── roundtrip.rs       # UPDATE: roundtrip test with continuing
    └── validate.rs        # UPDATE: validation error tests
```

## Phases

### Phase 1: Op + Interpreter

Add `continuing_offset` to `LoopStart`, update `Ctrl::Loop` and `Continue`
in the interpreter. All existing tests must still pass (offset defaults to
`head + 1`).

**op.rs:**
```rust
LoopStart {
    continuing_offset: u32,
    end_offset: u32,
},
```

**interp.rs:**
```rust
enum Ctrl {
    If { merge: usize },
    Loop { head: usize, continuing: usize, exit: usize },
    SwitchArm { end: usize, merge: usize },
}
```

`LoopStart` handler: push `Ctrl::Loop { head: pc, continuing: *continuing_offset as usize, exit: *end_offset as usize }`.

`Continue` handler: find innermost `Ctrl::Loop`, jump to `continuing` instead of `head + 1`.

**Validate:** `cargo test -p lpir` — all existing tests pass.

### Phase 2: Builder + Parser + Printer

**builder.rs:**

Add `continuing_set: bool` to `BlockEntry::Loop`. Add `push_continuing()`.
Update `end_loop()` to patch default `continuing_offset = start_idx + 1`
when `push_continuing()` was not called.

```rust
Loop {
    start_idx: usize,
    continuing_set: bool,
},
```

`push_continuing()`: find open `Loop` on block stack, set
`continuing_offset` on the `LoopStart` op to `self.body.len()`, set
`continuing_set = true`. Panic if not in a loop or called twice.

`push_loop()`: emit `LoopStart { continuing_offset: 0, end_offset: 0 }`.

`end_loop()`: if `!continuing_set`, patch `continuing_offset = start_idx + 1`.

**parse.rs:**

In `parse_stmt_line`, before the `line == "}"` check, add:
```rust
if line == "continuing:" {
    fb.push_continuing();
    return Ok(());
}
```

**print.rs:**

Track `loop_start_pc` in `Block::Loop`. When printing ops, after advancing
past `LoopStart`, check if the current `pc` equals `continuing_offset` for
the current loop — if so, emit `continuing:` at body indent level before
printing the op.

Concretely: change `Block::Loop` to store the `LoopStart` pc. In
`print_op_at`, before handling the current op, check if the top of the
stack is a `Block::Loop { start_pc }` and the current pc equals the
`continuing_offset` from `body[start_pc]` and that offset differs from
`start_pc + 1`. If so, emit `continuing:`.

**Tests:** Add a roundtrip test: parse text with `continuing:`, print,
verify output matches.

**Validate:** `cargo test -p lpir`

### Phase 3: Validator + Interpreter Tests

**validate.rs:**

Update `StackEntry::Loop` to carry `continuing_offset` and `loop_start`:
```rust
Loop { loop_start: usize, continuing_offset: u32 },
```

Offset bounds check when pushing `LoopStart`: deferred to `End` (we don't
know `end_offset` at push time — it's already set by the builder). Instead
check at the point we push: `continuing_offset >= loop_start + 1`. At `End`,
when popping a `Loop`, check `continuing_offset < end_offset`.

Continue-in-continuing check: in the existing
`Op::Continue | Op::Break | Op::BrIfNot` match arm, when checking
`Op::Continue` specifically: find the innermost `StackEntry::Loop` on the
stack. If `continuing_offset > loop_start + 1` and `i >= continuing_offset`,
emit an error.

**tests/interp.rs — new tests:**

`interp_loop_continuing_for`: Classic for-loop pattern (sum i=0..n with
continuing increment).

`interp_loop_continuing_break_if`: Loop with continuing + br_if_not for
the condition check.

`interp_loop_continuing_nested`: Nested loops where inner has continuing.

`interp_loop_continuing_continue_in_body`: Continue inside an if in the
body skips to continuing.

**tests/validate.rs — new tests:**

`validate_err_continue_in_continuing`: Continue in the continuing section
of the enclosing loop.

`validate_ok_continue_in_nested_loop_in_continuing`: Continue in an inner
loop inside the continuing section (allowed).

**Validate:** `cargo test -p lpir`

### Phase 4: Cleanup & validation

Grep the git diff for TODOs, debug prints, temporary code. Remove them.

Run `cargo +nightly fmt -p lpir`.
Run `cargo test -p lpir`.
Run `cargo clippy -p lpir`.

Fix all warnings, errors, and formatting issues.

Move plan to `docs/plans-done/`.

Commit with conventional commit format.

## Notes

### Interpreter bugs found and fixed

Two pre-existing bugs in the interpreter were exposed by the continuing
section work (they were latent because existing tests never triggered them):

1. **End popping the Loop frame.** The End handler for a loop was popping
   the Ctrl::Loop frame before jumping to head+1. Since head+1 is past the
   LoopStart (which pushes the frame), the loop frame was never re-created
   on subsequent iterations. This worked before because all existing loops
   ended with an explicit `continue` (which doesn't pop), so End was never
   reached via fall-through. Fix: End for a loop jumps to head+1 without
   popping. The frame stays for the next iteration and gets cleaned up by
   Break/BrIfNot.

2. **Continue not cleaning up intervening frames.** Continue was finding the
   innermost Loop on the stack (via iter().rev()) but not popping
   intervening If/SwitchArm frames. This caused stale frames to accumulate.
   When Continue jumped to the continuing section and execution reached the
   loop's End, the stale If frame was on top of the stack, causing End to
   match the If instead of the Loop. Fix: Continue pops intervening
   non-Loop frames before jumping, keeping the Loop frame itself.
