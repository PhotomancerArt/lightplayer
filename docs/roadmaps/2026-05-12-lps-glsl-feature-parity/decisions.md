# Decisions

## Filetests Are the Primary Gate

Use the existing shader filetests as the compatibility driver. Add focused `lps-glsl` fixtures when debugging a feature, but graduate behavior into the broader category gates whenever practical.

## Naga Is a Reference, Not a Runtime Dependency

The existing Naga-backed path is the behavior reference during implementation. The `server-lps-glsl` firmware path must continue to build without Naga so size and compile-time benefits remain measurable.

## HIR Is the Frontend Boundary

Keep syntax parsing language-specific and keep semantic HIR sufficiently language-neutral that WGSL can later lower into the same layer or a close sibling. Do not build WGSL during this parity pass.

## Places Are a First-Class Concept

Represent readable/writable locations and writable call arguments explicitly. This is the shared mechanism for swizzle assignment, array indexing, struct fields, globals, uniforms, and `out`/`inout` writeback.

## Aggregate Layout Is Centralized

Do not scatter scalar lane counts, field offsets, array strides, and matrix column assumptions across parser, HIR, and lowering. Reuse `lps_shared::layout` and LPVM data/path helpers for byte layout, then derive a small `lps-glsl` shape/view layer for semantic checks, lane-flat lowering, slot-backed lowering, globals, uniforms, and call ABI choices.

## Existing Layout Logic Is the Authority

`lps-glsl` should not implement an independent std430 layout engine. It should consume `lps_shared::layout` for `LpsType` size/alignment/stride and use the LPVM data/path model as the reference for byte-backed aggregate access.

## Hybrid Aggregate Storage

Use lane-flat values where they are small and statically addressable, but allow slot-backed storage for aggregate locals, params, globals, uniforms, dynamic indexing, and pointer-like call boundaries. This keeps the fast path small without boxing us out of arrays of structs or aggregate `inout`.

## Pointer ABI Is a Stop-and-Design Boundary

If `lps-glsl` needs a new aggregate pointer ABI or aggregate return convention in LPIR/backend code, stop for a focused design pass instead of improvising inside the frontend. The archived aggregate roadmap is a useful reference, but the native GLSL frontend should adopt only the pieces it actually needs.

## Diagnostics Need Spans Before Recovery

Good first-error diagnostics are enough for parity work. Recovery and multi-error reporting are useful later, but source spans and line indicators should be present throughout.

## No Preprocessor for This Pass

The frontend does not need a GLSL preprocessor unless a product-critical filetest requires it. Avoid rebuilding GPU compiler infrastructure that LightPlayer does not use.

## Keep Files Small as Features Land

Refactor into concept-sized files when adding features makes current files bulky. Avoid a large up-front module migration that delays filetest progress.
