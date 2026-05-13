# LightPlayer-Native GLSL Frontend

Date: 2026-05-13

Status: accepted, parity work in progress

## Summary

`lps-glsl` is a LightPlayer-specific GLSL compiler frontend. It parses authored
GLSL shader source, builds typed HIR, and lowers directly to LPIR for the
embedded runtime compiler.

The frontend exists because LightPlayer compiles shaders on the ESP32-C6 at
runtime. The compiler is part of the firmware product path, so the frontend must
fit embedded constraints: `no_std + alloc`, low flash cost, bounded heap use,
good compile latency, source-spanned diagnostics, and a path toward resumable
compilation.

## Runtime Pipeline

The intended product path is:

```text
project shader source
  -> lps-glsl
  -> LPIR
  -> lpvm-native
  -> executable RV32 code in RAM
```

`lps-glsl` owns the source-language part of that pipeline. It does not generate
machine code directly. Its output is LPIR plus metadata needed by the rest of
the LightPlayer shader runtime.

## Decision

LightPlayer will keep developing `lps-glsl` as the native frontend for the
embedded runtime path. The default firmware path must be able to compile shaders
without depending on the previous frontend stack.

This is a specialization decision. The frontend should be shaped around
LightPlayer's runtime compiler: one source language surface, one IR target,
firmware constraints, resumability, and diagnostics over authored shader files.

## Rationale

### The Compiler Runs On Device

LightPlayer does not compile shaders only on a developer workstation. A project
can load GLSL source on the ESP32-C6, compile it there, and execute the emitted
code directly.

That makes frontend cost visible in product behavior:

- app image size affects whether the firmware fits comfortably
- compile latency affects shader reload time
- heap use affects reliability
- dependency choices affect `no_std` portability
- monolithic compiler phases affect whether compilation can be scheduled across
  frames

### LightPlayer Targets One IR

`lps-glsl` lowers to LPIR. It does not need a general GPU shader IR, graphics
pipeline metadata, or support for unrelated backend formats.

This lets the frontend represent exactly the concepts needed by LightPlayer:

- values and writable places
- uniforms and texture inputs in the LightPlayer runtime model
- LPVM aggregate layout and data paths
- LPIR function calls and builtin lowering
- diagnostics over authored source text

### Resumability Is A Frontend Feature

The frontend exposes `CompileJob`, which can step through compilation under a
budget:

```text
lex -> index -> body/HIR -> lower -> done
```

The current resolution is stage-level. That is enough to establish the shape and
can be refined later if profiling shows a particular stage needs smaller
increments.

### Diagnostics Need Spans Early

The frontend can stop at the first error for now. It still needs source spans
throughout the pipeline. A firmware compiler that produces a useful line/span
diagnostic is much easier to use than one that only reports a generic parse or
lowering failure.

### The Product Language Is GLSL-Shaped, Not All Of GLSL

LightPlayer shaders use GLSL syntax and semantics, but the product does not need
every desktop GLSL feature. The compatibility contract is the repository's
filetests and examples, plus product-critical shader use cases.

The frontend intentionally avoids starting with:

- preprocessing
- full desktop GLSL compatibility
- GPU pipeline validation
- shader-stage metadata that LightPlayer does not consume
- fallback host compilation as the normal runtime path

## Prior Art And Reference Path

The previous LightPlayer GLSL frontend used Naga through `lps-frontend`. Naga is
a Rust shader translation and validation library for broad GPU shader compiler
use cases: parsing shader languages, validating shader modules, representing
GPU-oriented shader IR, and supporting multiple downstream graphics tooling
paths.

Naga was a reasonable first implementation. It gave LightPlayer a working GLSL
parser and validation layer while the LPIR and native RV32 compiler path were
being built. It remains useful to `lps-glsl` as prior art and as a behavior
reference:

- its supported behavior is captured by the existing filetests
- `rv32n.q32` keeps the old frontend visible beside `rv32lpn.q32`
- divergences can be classified against a known implementation
- the old firmware path gives a concrete size and compile-time baseline

The native frontend is not a rejection of that work. It is the next
specialization step: once the product compiler target is known, the frontend can
be shaped directly around LightPlayer's IR, layout model, diagnostics, memory
budget, and resumability needs.

## Measured Signal

The initial `lps-glsl` vertical slice compiled `examples/basic` on ESP32-C6 with
the same native backend:

| Path | App image | App partition | Shader bytes | Compile time |
| --- | ---: | ---: | ---: | ---: |
| Naga-backed frontend | `2,681,296` bytes | `85.24%` | `3922` | `578ms` |
| `lps-glsl` parity closure | `2,071,568` bytes | `65.85%` | `3922` | `195ms` |

This is not a claim of full desktop GLSL compatibility. It is a representative
measurement for the current LightPlayer shader surface after the parity push,
and it shows that frontend specialization can recover flash headroom and improve
interactive compile latency.

Full report:
[`docs/reports/2026-05-12-lps-glsl-frontend-experiment.md`](../reports/2026-05-12-lps-glsl-frontend-experiment.md).

## Compatibility Strategy

Filetests drive parity.

- `rv32n.q32` tests the older Naga-backed frontend with `lpvm-native`.
- `rv32lpn.q32` tests `lps-glsl` with `lpvm-native`.

Running both targets side by side keeps behavior differences visible. Each
difference should be classified as one of:

- an `lps-glsl` bug
- an older-path bug exposed by improved tests
- an intentional LightPlayer language difference
- an unsupported GPU feature outside the runtime shader surface

This makes the rewrite empirical rather than taste-driven.

## Design Principles

### Keep The Runtime Compiler Real

Do not solve size, dependency, or `no_std` problems by removing the runtime
compiler from firmware. The on-device compiler is the product path.

### Keep Naga Out Of The Default Firmware Path

The Naga-backed path can remain a reference target during parity work. The
default firmware path should stay independent so the size and compile-time
benefits remain measurable.

### Model Places Explicitly

GLSL features such as swizzle assignment, array indexing, struct fields,
uniforms, globals, and `inout` calls all depend on readable and writable
locations. The frontend should model places directly instead of encoding each
case as a lowering special case.

### Centralize Shape And Layout

Aggregate layout should come from LightPlayer's existing layout code. `lps-glsl`
should consume `lps_shared::layout` and LPVM data/path concepts rather than
implementing an independent layout engine.

### Keep WGSL Optional

A future WGSL frontend may be useful, but GLSL implementation should not be
distorted around it. The reusable layer is semantic: typed HIR, builtin
handling, place modeling, aggregate shape, and LPIR lowering. WGSL can target
that layer later or copy the stable pieces.

## Risks

- The frontend could drift into a general-purpose GLSL compiler if scope is not
  controlled by filetests and product shaders.
- Aggregate storage, pointers, and `inout` writeback can become fragile if place
  and layout concepts are not kept central.
- Feature parity will add code, so binary size should be remeasured after major
  milestones.
- Constant folding can hide runtime builtin costs in filetests unless tests use
  runtime values where needed.

## Current State

`lps-glsl` has proven the vertical slice and is being driven toward `rv32n.q32`
parity with filetests. The architectural goal is a small LPIR-directed frontend:
firmware-capable, resumable, source-spanned, and shaped around the LightPlayer
runtime rather than a general GPU compiler pipeline.
