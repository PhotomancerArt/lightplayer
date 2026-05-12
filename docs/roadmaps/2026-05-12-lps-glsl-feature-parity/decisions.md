# Decisions

## Filetests Are the Primary Gate

Use the existing shader filetests as the compatibility driver. Add focused `lps-glsl` fixtures when debugging a feature, but graduate behavior into the broader category gates whenever practical.

## Naga Is a Reference, Not a Runtime Dependency

The existing Naga-backed path is the behavior reference during implementation. The `server-lps-glsl` firmware path must continue to build without Naga so size and compile-time benefits remain measurable.

## HIR Is the Frontend Boundary

Keep syntax parsing language-specific and keep semantic HIR sufficiently language-neutral that WGSL can later lower into the same layer or a close sibling. Do not build WGSL during this parity pass.

## Lvalues Are a First-Class Concept

Represent assignment targets and writable call arguments explicitly. This is the shared mechanism for swizzle assignment, array indexing, struct fields, and `out`/`inout` writeback.

## Diagnostics Need Spans Before Recovery

Good first-error diagnostics are enough for parity work. Recovery and multi-error reporting are useful later, but source spans and line indicators should be present throughout.

## No Preprocessor for This Pass

The frontend does not need a GLSL preprocessor unless a product-critical filetest requires it. Avoid rebuilding GPU compiler infrastructure that LightPlayer does not use.

## Keep Files Small as Features Land

Refactor into concept-sized files when adding features makes current files bulky. Avoid a large up-front module migration that delays filetest progress.

