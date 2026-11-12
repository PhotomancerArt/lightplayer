# Phase 6: Cleanup and finalization

## Goal

Remove temporary code, fix warnings, ensure all tests pass, and format code.

## Tasks

1. Review code for temporary code, TODOs, debug prints:
   - Search for `TODO`, `FIXME`, `XXX` comments
   - Remove any debug `println!` or `dbg!` statements
   - Remove any temporary workarounds

2. Fix all warnings:
   - Run `cargo build` in `lp-app` workspace
   - Fix any compiler warnings
   - Address unused imports, unused variables, etc.

3. Run all tests:
   - Run `cargo test` in `lp-app` workspace
   - Ensure all tests pass
   - Fix any failing tests

4. Format code:
   - Run `cargo +nightly fmt` on entire `lp-app` workspace
   - Verify formatting is consistent

5. Review code for clarity:
   - Ensure documentation is accurate
   - Verify naming is consistent
   - Check that error messages are clear

6. Verify final state:
   - Transport traits use `ClientMessage`/`ServerMessage` directly
   - `LocalMemoryTransport` is in `lp-shared` and is single-threaded
   - All callers use new transport API
   - No manual serialization/deserialization in callers

## Success Criteria

- [ ] No temporary code or TODOs remain
- [ ] All warnings are fixed
- [ ] All tests in `lp-app` pass
- [ ] Code is formatted with `cargo +nightly fmt`
- [ ] Code is clean and readable
- [ ] Documentation is accurate

## Style Notes

- Place helper utility functions **at the bottom** of files
- Place more abstract things, entry points, and tests **first**
- Run `cargo +nightly fmt` on all changes before committing
- Keep language professional and restrained
- Avoid overly optimistic language like "comprehensive", "fully production ready"
- Avoid emoticons
- Code is never done, never perfect, never fully ready, never fully complete
