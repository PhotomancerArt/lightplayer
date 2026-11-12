# Project Load Memory Roadmap

## Motivation

`button-sign` is too close to the ESP32-C6 heap ceiling before shader
compilation begins. The compiler remains the core product and must keep running
on device, so the right pressure point is the project representation that exists
before compilation: loader state, artifact definitions, node tree indexes, slot
shape metadata, and parse-time buffers.

The goal is to turn project load from a host-shaped graph of rich objects into
an embedded-shaped runtime image: compact, typed, mostly immutable after load,
and cheap to keep resident while compilation and rendering happen.

## Design Direction

Current shape:

```text
TOML artifacts
  -> toml::Value
  -> authored NodeDef cache
  -> NodeTree entries
  -> runtime node config/state
  -> binding/path/artifact indexes
  -> per-engine slot shape registry
```

Target shape:

```text
TOML artifacts / flash
  -> measured load-only pipeline
  -> compact runtime graph
  -> frozen indexes and interned ids
  -> static slot shape registry view
  -> cold authored-definition access when needed
```

## Success Criteria

- A profile mode isolates project load memory before first frame and before
  shader compilation.
- `button-sign` has enough free heap after project load to compile reliably on
  ESP32-C6 with meaningful margin.
- Base project load overhead drops materially for both small and large projects,
  not only for `button-sign`.
- Runtime behavior and client-visible project semantics remain intact.
- The on-device GLSL JIT path remains present in default firmware/server builds.

## Architecture Bets

- Measure load-only memory before making structural changes, then keep the
  profile command as the regression guard.
- Treat authored definitions as source/debug data, not the main runtime graph.
- Move static schema-like data out of per-project heap.
- Prefer frozen tables, ids, and compact arrays over tree maps for loaded
  projects.
- Avoid temporary parse forms that require holding two complete versions of the
  project at once.

## Alternatives Considered

- Host precompilation or host-generated runtime bytecode is not a solution for
  this roadmap because the product requires on-device GLSL compilation.
- Disabling shader compilation or hiding it behind `std` is not allowed.
- Pure allocator tuning may help later, but it should come after removing
  avoidable resident structures and temporary intermediates.

## Risks

- Client/editor APIs may rely on rich authored definitions being resident.
- Compact indexes can make live graph mutation harder if the project is edited
  in place.
- Direct parsing can reduce peak memory but may make error messages and schema
  evolution more delicate.
- Moving static registry data into flash or shared storage may require careful
  lifetime/API cleanup across `lpc-model`, `lpc-engine`, and server code.

## Milestones

1. Add load-only memory instrumentation.
2. Split static slot shape registry data from per-project heap state.
3. Replace always-resident authored graph storage with a compact runtime graph.
4. Convert graph/artifact/binding indexes to embedded-shaped tables and interned
   ids.
5. Remove high-peak TOML intermediates from artifact loading.
6. Validate memory wins on emulator, host profiles, and ESP32-C6.
