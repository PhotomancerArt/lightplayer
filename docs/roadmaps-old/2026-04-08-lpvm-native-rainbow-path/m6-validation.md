# Milestone 6: Cleanup, Validation, and Performance Comparison

**Goal**: Final validation across all milestones, cleanup of temporary scaffolding, documentation updates, and performance comparison report.

## Suggested Plan

`lpvm-native-validation-m6`

## Scope

### In Scope

- **Regression testing**: All filetests pass on `rv32lp.q32`
- **Performance validation**: FPS and memory metrics collected and compared
- **Code cleanup**: Remove debug prints, TODOs, temporary workarounds
- **Documentation**: Update design docs with final ABI, lessons learned
- **Cleanup decisions**: Remove or promote experimental features

### Out of Scope

- New features or optimizations (future roadmap)
- Graph coloring allocator (future optimization)
- Vector shuffle expansion (future LPIR work)

## Key Decisions

1. **Greedy allocator**: Keep for testing/comparison, not default
2. **ELF path**: Keep for host filetests, not used on device
3. **Feature flags**: `native-graphics` becomes user-selectable option
4. **Documentation**: Update `docs/design/native/overview.md` with final choices

## Deliverables

| Deliverable | Location | Description |
|-------------|----------|-------------|
| Full test pass | CI | All `rv32lp.q32` filetests green |
| Performance report | `docs/reports/2026-04-lpvm-native-comparison.md` | FPS, memory, compile-time RAM comparison |
| Code cleanup | All `lpvm-native/` | Remove TODOs, fix warnings |
| Documentation update | `docs/design/native/overview.md` | Final ABI, linear scan details |
| Cleanup validation | `docs/roadmaps/` | Roadmap completion notes |

## Dependencies

- All previous milestones complete and green
- CI infrastructure for `fw-emu` tests

## Estimated Scope

- **Lines**: ~200-400 (mostly deletions, doc additions)
- **Files**: 5-10 (cleanup across codebase)
- **Time**: 2-3 days

## Verification Commands

```bash
# Host filetests (correctness)
cargo test -p lps-filetests

# Emulator with native backend
cargo test -p fw-tests --features native-graphics --test scene_render_emu

# ESP32 build check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,native-graphics

# Memory and performance (manual)
cargo run -p fw-tests --features native-graphics --test alloc_trace_emu
```

## Success Criteria

1. All milestones integrated, no regressions
2. Rainbow shader runs correctly on both backends
3. Performance comparison report published
4. Documentation reflects final implementation
5. CI green on all validation commands
