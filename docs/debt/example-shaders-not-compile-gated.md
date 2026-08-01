---
status: carried
since: 2026-07-29
logged: 2026-07-29
area: examples/**/*.glsl, CI (just check / build-ci), lps-filetests
related:
  - docs/defects/2026-07-29-uniform-struct-array-runtime-index.md
---
# Shipped example shaders are not compile-gated on non-host targets

**Shape** — nothing in CI compiles the GLSL under `examples/` for the
device- and browser-canonical shader targets. `examples_valid.rs` loads
every example as a project (which exercises the HOST backend only), and the
filetest suite covers `lp-shader/lps-filetests/filetests/**`, never
`examples/`. An example can therefore ship a construct that compiles on the
host and fails on `rv32n` / `rv32c` / `wasm` / `interp` — the targets that
actually run on device and in the browser sim.

The natural-looking guard does not work, which is what makes this
structural rather than one missing assertion. An engine-level render test
(load the example, tick, render, assert nonzero pixels) runs the host
backend by construction, so it cannot observe another target's lowering.
Asserting node runtime status does not help either: on the host there is no
error to report. The gate has to compile the example sources against the
other targets explicitly.

**Carrying cost** — the failure surfaces only when a human opens that
example in Studio. It presents as a runtime shader-compile error on a node
that otherwise mounted and "runs", which is easy to read as a preview
glitch rather than broken shipped content.

**Workarounds**
- When authoring example GLSL, copy the shape of an existing example that
  is already known-good on device rather than inventing one; the uniform
  struct-array idiom in `examples/events/shader.glsl` is the reference.
- After adding or editing an example shader, open it in the browser sim
  once and read the node's status — that is currently the only end-to-end
  check.

**Incident log**
- 2026-07-29 — first occurrence:
  [uniform-struct-array-runtime-index](../defects/2026-07-29-uniform-struct-array-runtime-index.md).
  The meteor example's render shader indexed a uniform struct array with a
  runtime value; the construct failed on 4 of 5 targets while every
  automated gate stayed green.

**Exit criteria** — extend the filetest runner (or add a small harness) to
compile every `examples/**/*.glsl` for `ALL_TARGETS`, run-free
(compile-only), and fail on any target that rejects a shipped example.
Compile-only keeps it cheap and needs no uniform values or expected
outputs. Note the filetest harness currently treats `compile-fail` as an
expected-failure category, so this gate must assert on it rather than
reuse that path.
