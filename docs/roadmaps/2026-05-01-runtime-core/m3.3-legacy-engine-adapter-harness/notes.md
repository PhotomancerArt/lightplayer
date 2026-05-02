# M3.3: Legacy Engine Adapter Harness

## Purpose

Build the adapter and parity-test foundation M4 needs before porting the full
legacy shader -> fixture -> output behavior.

M4 should start with a known graph-construction path, fixture demand-root
strategy, output flush strategy, and old-vs-new comparison harness.

## Working Scope

- Build a source-loaded legacy project into a core `Engine` skeleton.
- Establish adapter-node boundaries for shader, fixture, output, and texture
  compatibility.
- Add parity harnesses that can run old `LegacyProjectRuntime` and the new
  engine path over the same source fixture.
- Prove ordering and caching with thin or dummy adapters before moving the full
  runtime behavior.

## Handoff To M4

M4 should be able to replace thin adapters with real legacy behavior and drive
`just demo` on the core engine stack without changing graph construction,
product ownership, or wire projection fundamentals.
