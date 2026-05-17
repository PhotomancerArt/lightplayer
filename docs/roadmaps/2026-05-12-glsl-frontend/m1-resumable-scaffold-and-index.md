# Milestone 1 - Resumable Scaffold and Top-Level Index

## Title and Goal

Create the experimental frontend crate, filetest seam, resumable job skeleton, token tape, and
top-level index without changing the production default.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m1-resumable-scaffold-and-index/`

## Scope

In scope:

- Add `lp-shader/lps-glsl` as `no_std + alloc`.
- Add a synchronous API implemented as a loop over a resumable `lps_glsl::CompileJob`.
- Add lexer/token tape with spans and basic source map support.
- Add top-level scanning for uniforms, global constants, function signatures, and function body
  spans.
- Add `rv32lpn.q32` as a filetest target that routes through `lps-glsl` and `lpvm-native`.
- Add host unit tests for tokenization and top-level indexing of all current example shaders.

Out of scope:

- Full function body lowering.
- Embedded scheduler integration.
- Replacing the production compile path.

## Key Decisions

- Resumability exists from day one, even if the first yield units are coarse.
- Filetests expose lps-glsl as the explicit `rv32lpn.q32` target so it can run beside `rv32n.q32`.
- Naga remains the default frontend and correctness oracle.

## Deliverables

- New crate skeleton.
- `CompileJob` API and synchronous compile wrapper.
- Token, span, and diagnostic foundations.
- Example index tests proving every current example can be scanned.
- Filetest target seam that can run `rv32n.q32` and `rv32lpn.q32` in the same invocation.

## Dependencies

- Existing `lps-filetests` runner and `lps-frontend` path.
- Existing `lpir` and `lps-shared` APIs.

## Execution Strategy

Small plan. The implementation is bounded, but it touches workspace wiring, CLI plumbing, and a new
crate API, so a short plan should pin the public surface before coding.
