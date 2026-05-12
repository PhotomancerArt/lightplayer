# Engine Memory Diagnostics And Recovery

## Status

Future work. Captured after the May 2026 render-texture leak that caused
ESP32-C6 OOMs during ordinary rendering.

## Problem

LightPlayer runs close to the memory limits of embedded targets. Slow steady
memory growth is almost always a bug, but today the engine cannot explain where
memory lives when the heap is low. By the time the allocator reports OOM, the
most useful domain context has already been lost.

The render-texture leak showed the gap clearly: resource managers would have
known that many transient render textures were live, but the OOM handler could
only report the failing allocation size.

## Goals

- Treat memory as a first-class engine diagnostic.
- Make leaks visible before OOM.
- Give nodes and resource managers a chance to report domain-specific memory
  state when OOM or low-memory pressure happens.
- Prefer degraded operation over crashing when possible.

## Diagnostic Ideas

### Engine-Wide Memory Report

Add an engine memory report that asks each subsystem for compact counters:

- graphics/render targets: live count, live bytes, peak bytes, alloc/free count
- runtime buffers: live count, live bytes, peak bytes
- output providers: channel count, buffer bytes, driver-owned bytes when known
- nodes: node-owned caches and scratch buffers
- resolver/dataflow: frame cache size, binding index size
- slot/resource registries: shape/resource counts and approximate heap use

This report should be available from normal debug reads and from emergency
logging paths.

### OOM Reporting Hook

Add an engine-level OOM or allocation-failure hook that can ask resource
managers and nodes to emit a compact memory report.

On embedded, the hook must avoid heap allocation itself. It should write to a
fixed-size stack buffer, serial stream, or small static emergency buffer.

### Frame Delta Heuristics

Track frame-to-frame heap deltas:

- sudden spikes are expected during shader compilation and project loading;
- slow monotonic increases during steady render are suspicious;
- repeated same-size growth is very suspicious.

This does not need to be clever at first. A useful MVP is:

```text
if project is loaded and no compile/reload is active:
    if free heap decreases for N consecutive frames:
        log warning with per-subsystem memory report
```

The warning threshold should be tuned for ESP32-C6 and kept cheap.

### Resource Manager Invariants

Resource managers should track live handles and expose invariants:

- transient render targets must be zero at frame end unless explicitly retained;
- output buffers should be stable after output open/configure;
- shader compile scratch should be released after compile;
- project unload should return engine-owned resources to zero or a known base.

Debug/profile builds can assert or warn on invariant violations.

## Recovery Ideas

OOM recovery is a bigger system feature, but the likely ladder is:

1. Stop the current frame's work and mark it failed.
2. Drop transient scratch and render targets.
3. Ask nodes to handle memory pressure and prune optional caches.
4. Drop debug/probe resources first.
5. Degrade project output for a frame rather than crashing.
6. If pressure remains, disable the project and keep the server alive.
7. If the server cannot allocate enough to communicate, enter a minimal
   watchdog/error loop.

The goal is not that every OOM preserves perfect output. The goal is that the
device keeps control of itself and can explain what happened.

## Open Questions

- Should `alloc` failures become recoverable `Result`s in more engine paths, or
  should we rely on panic recovery around large phases?
- How much domain memory accounting can be compiled into release firmware
  without unacceptable overhead?
- Should transient buffers be owned by explicit arenas with frame-reset
  semantics?
- Should project/runtime memory budgets be configurable per target?
- Can the profile harness reproduce ESP32 heap behavior closely enough, or do
  we need lightweight on-device memory tracing?
