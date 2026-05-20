# M1: Load-Only Memory Instrumentation

## Goal

Make project load memory observable without conflating it with first frame,
output open, shader compilation, or render pipeline setup.

## Work

- Emit `lp_perf::EVENT_PROJECT_LOAD` begin/end around the actual project load
  path.
- Add an `lp-cli profile` mode that stops at project-load end.
- Record final free heap, peak free heap, allocation count, deallocation count,
  realloc count, and live bytes at load end.
- Capture baseline profiles for `examples/basic` and `examples/button-sign`.
- Add optional pressure knobs only if needed to make load behavior visible on
  host or emulator.

## Deliverables

- A `load` or `project-load` profile mode.
- Checked-in docs with the first baseline numbers and command examples.
- A short list of biggest allocation sites if allocation tracing can expose
  them cheaply.

## Validation

```bash
cargo run -p lp-cli -- profile examples/basic --collect alloc --mode project-load
cargo run -p lp-cli -- profile examples/button-sign --collect alloc --mode project-load
```

Run the existing shader-pipeline validation only if code changes cross into the
load/runtime path.

## Implementation Strategy

Small plan. This is a narrow instrumentation change, but it should be landed
first because every later milestone needs trustworthy numbers.
