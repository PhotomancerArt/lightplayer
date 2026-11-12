# Shader Probes And `TRACE()` Debugging

## Status

Future work. Captured after debugging a discontinuity in `examples/basic`
using time scrubbing to isolate the failure at exactly `time = 10.0`, then
using an agent to locate the bug.

## Problem

LightPlayer owns the shader frontend, compiler, runtime, and execution model.
That gives us an opportunity to make shader debugging much better than
traditional GLSL tooling, especially on CPU-executed shaders where we control
the entire stack.

Today the workflow is still too indirect:

- a user notices an artifact such as "colors flash red sometimes" or "there is
  a weird bright patch";
- the operator narrows the failure by scrubbing time, changing inputs, or
  inspecting outputs;
- an agent or human reads source and guesses at the bad expression.

That can work, but it does not yet provide a first-class way to inspect a
shader invocation, capture values at a specific source location, or ask the
runtime to explain what happened at a suspicious pixel or tick.

## Direction

Treat shader probes as a standalone debug facility owned by the requesting
client, not by project state.

This should be the primary model:

- probes are sent as part of a read/debug request;
- probes do not mutate node/source/config state permanently;
- multiple clients can attach different probes to the same project at the same
  time;
- probes disappear automatically when the client stops asking for them;
- shader nodes compile probe variants keyed by `(shader revision, probe spec)`.

Also support a manual `TRACE(...)` debugging affordance inside shader source
for humans or agents that want to debug by editing code directly.

## Goals

- Make it easy to inspect shader execution at a precise time and position.
- Support both visual shaders and serial compute shaders.
- Keep authored project state clean; avoid "cleanup old debug code" workflows.
- Make probe requests agent-friendly and composable.
- Bound runtime overhead and memory use on embedded targets.
- Preserve the normal shader result: probes observe execution, they do not
  change shader semantics.

## Primary Model: Sidecar Probes

A probe is a standalone request object.

Conceptually a probe should describe:

- target node / shader;
- shader kind: visual or compute;
- selection data:
  - visual: point, set of points, rect, or other spatial selector;
  - compute: tick invocation or produced-state selector;
- optional condition expression;
- one or more trace expressions or trace sites;
- bounded output limits such as max events or max bytes.

This model keeps debugging separate from authored source and gives agents a
clean API:

- ask for a probe;
- inspect results;
- refine the probe;
- stop sending it when done.

## Manual `TRACE(...)`

Inline tracing is still valuable and should exist alongside sidecar probes.

The intended shape is a special debug construct such as:

```glsl
TRACE("palettePhase", palettePhase);
TRACE("tv", tv);
TRACE_IF(cyclePhase > 0.8, "near_boundary", cyclePhase, palette);
```

Notes:

- this is a LightPlayer language extension, not standard GLSL;
- the runtime does not need general-purpose string support;
- labels should be treated as compile-time debug labels, not as runtime string
  values;
- when tracing is disabled, these calls should lower to no-ops.

Internally, `TRACE(...)` can lower to one or more typed `lpfn_trace_*`
operations or another internal ABI surface. The important part is the source
language contract, not the exact builtin naming.

## Trace Sites And Scope

The main abstraction should be semantic trace sites, not "every variable on a
line".

Useful trace targets include:

- a specific expression value;
- a branch condition and chosen arm;
- a return value;
- a uniform/global read;
- a user-written `TRACE(...)` call;
- a source span selected by a sidecar probe.

Not every line/column is a valid insertion point. Sidecar probes should attach
to expressions or other semantically valid sites.

The upcoming `lps-glsl` frontend is a good fit for this because it carries
source spans through HIR and lowering. Source-level probe attachment should
eventually be compiler-native rather than based only on text rewriting.

## Visual And Compute Coverage

Probes should work for both shader classes.

For visual shaders, a probe request needs execution coordinates and environment
such as:

- time;
- sample position or rect;
- output size or fixture/render context.

For compute shaders, a probe request needs tick-time execution context such as:

- time;
- consumed uniform values;
- produced globals or persistent shader state.

The same trace event model should serve both, with different invocation
metadata.

## Runtime Shape

The likely runtime contract is VM-context-owned trace state with a bounded
buffer.

Expected ingredients:

- trace enabled flag;
- probe or compile key metadata;
- fixed-capacity event buffer;
- event count and dropped-event count;
- optional per-run selection/condition metadata.

Trace output should be bounded. Overflow should be explicit and observable.
Debugging must not silently consume unbounded memory on ESP32.

## Compilation Strategy

Shader nodes should detect probes relevant to them and compile a probe variant
separately from the normal shader variant.

High-level flow:

1. Collect active probes affecting this shader node.
2. Derive a stable probe compile key from shader revision plus probe spec.
3. Recompile when that key changes.
4. Execute the instrumented variant when a matching debug request is active.
5. Extract trace data from VM context and return it through probe/debug APIs.

The normal non-probed shader variant should continue to exist. Probing should
not permanently replace ordinary execution.

## Source Rewriting vs Compiler-Native Instrumentation

Two implementation paths are plausible:

### Source Rewriting

Mutate shader source to inject `TRACE(...)` calls or a generated helper region,
then compile that synthetic source.

Pros:

- simple to reason about at first;
- visible to humans and agents;
- a "compiled GLSL" view can show the inserted traces directly.

Cons:

- fragile around source edits and insertion context;
- weaker long-term semantic identity than compiler-native instrumentation.

### Compiler-Native Instrumentation

Attach trace requests to semantic sites in the frontend/lowering pipeline and
inject tracing during lowering.

Pros:

- better semantic stability;
- cleaner source-span attachment;
- better long-term fit for sidecar probes.

Cons:

- more upfront compiler work.

Current bias:

- support authored `TRACE(...)` in source syntax;
- prefer compiler-native instrumentation for sidecar probes;
- if early experimentation uses source rewriting, treat it as a stepping stone,
  not the final architecture.

## Conditions

Conditions are likely very useful and may simplify the model.

A probe should be able to say things like:

- only emit when a boolean expression is true;
- only emit for pixels inside a selected rect;
- only emit when a value crosses a threshold;
- only emit for the suspicious invocation rather than every invocation.

Conditions should gate trace emission and selection, not change shader control
flow or outputs.

## Representation Notes

Do not commit too early to "everything becomes `LpsValueQ32`".

It may be convenient to normalize debug values later, but the runtime trace
payload likely wants to preserve richer type information for:

- bool / int / uint semantics;
- vectors and matrices;
- structs or flattened aggregate fields;
- future texture/resource-adjacent diagnostics.

The runtime can store a compact typed payload and decode into a higher-level
debug value representation when extracting events.

## Agent-Oriented Use Cases

The target user experience is that an agent can respond to reports like:

- "the colors flash red sometimes";
- "there is a weird bright patch";
- "the output jumps when I scrub time".

An ideal workflow is:

1. Reproduce and narrow the failure with time/space selection.
2. Install a sidecar probe on the suspicious node.
3. Compare nearby invocations such as `time=9.99` and `time=10.0`.
4. Explain which expression or branch changed unexpectedly.

This should make shader probing a first-class agent skill rather than an
ad-hoc text-search exercise.

## Open Questions

- What is the wire/API shape for probe requests and probe results?
- Should sidecar probes identify source sites by span, frontend node id, or
  probe-local expression text?
- How much of the condition language should ship in v1?
- What is the right bounded event format for embedded targets?
- How should visual selection map onto fixture/sample space for non-texture
  outputs?
- Should there be a rendered "instrumented GLSL" view even when the actual
  implementation is compiler-native?
- How much probe support belongs in `project_read` versus a dedicated debug
  message family?
- What is the cheapest useful MVP: point snapshots, manual `TRACE(...)`, or
  source-span sidecar traces?
