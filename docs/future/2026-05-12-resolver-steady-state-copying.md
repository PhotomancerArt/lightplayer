# Resolver Steady-State Copying

## Status

Future work. Captured after profiling `examples/basic` with fixture direct
sampling on 2026-05-12.

## Context

Direct shader sampling removed the old texture-area sampling hotspot, but the
steady-render profile still showed more copying than expected for a simple
scene:

```text
profile: profiles/2026-05-12T07-50-10--examples-basic--steady-render--direct-sampling
total_attributed_cycles = 5.61M
memcpy = 490k cycles / 8.8%
```

That looked suspicious because the direct path should not need to move large
pixel buffers around. The useful finding was that the copying was not mostly a
single pixel-buffer transfer. It was many small copies in dataflow/resolver
bookkeeping.

## What We Learned

The hot `memcpy` callers were mostly:

- `EngineSession::resolve`
- `String::clone`
- `Production::new`
- `ResolveTrace::try_push_active`
- `ResolveTrace::pop_guarded`
- `EngineResolveHost::produce`
- small `RawVec` growth paths

The largest easy wins were:

- Do not construct trace events when `ResolveLogLevel::Off`.
- Avoid cloning `QueryKey` / `SlotPath` while dispatching through the resolver.
- Avoid deep-cloning resolved `LpValue` payloads on cache hits and cache insert.
- Use borrowed `FromLpValue` parsing for runtime reads.
- Use a small linear resolver cache instead of a per-frame `BTreeMap`.

After those changes:

```text
profile: profiles/2026-05-12T08-29-15--examples-basic--steady-render--resolver-borrow-lpvalue
total_attributed_cycles = 4.90M
memcpy = 226k cycles / 4.6%
```

This cut the attributed copy cost by a bit over half and reduced total
attributed frame cost by roughly 12-13%.

## Current Tradeoff

The quick fix uses shared ownership for cached production values:

```rust
Rc<WithRevision<LpValue>>
```

That avoids deep `LpValue` clones on cache hits, which is important because
products and structured values can be nontrivial. The tradeoff is one heap
allocation per newly produced resolver value. In the measured scene this was a
net win, but it is not the ideal embedded shape.

This is acceptable as an intermediate optimization, but it should not become the
final mental model for hot resolver storage if profiling later shows allocation
pressure.

## Better Long-Term Shapes

### Per-Frame Resolver Arena

Store resolver products in a frame-local arena and let cache entries point into
that arena.

Benefits:

- no per-production allocator metadata;
- cheap cache hit returns;
- all resolver scratch drops at frame end;
- memory pressure is easier to account for.

The hard part is lifetime ergonomics. The current resolver returns owned
`Production`s, which is simple. An arena-backed design likely wants handles or
borrowed products with stricter session lifetimes.

### Compact Query Keys

`QueryKey` is currently convenient but relatively heavy: it can own `SlotPath`,
`SlotAccessor`, and `ChannelName` strings. In steady-state resolution, most
queries refer to stable nodes, slots, and buses.

Potential direction:

- compile consumed slot access to small indexed accessors;
- give bindings stable ids;
- resolve bus/channel names to compact ids during node-tree/binding indexing;
- represent active resolver stack entries as small ids instead of cloned
  structural keys.

This would reduce active-stack copies and make cycle detection cheaper.

### Trace Log Separation

`ResolveTrace` currently owns both always-needed cycle-detection state and
optional diagnostic events. Even with event construction guarded, the type
invites accidental work in the hot path.

Potential direction:

- keep a small mandatory active stack for cycle detection;
- move event logging behind a separate optional sink;
- make the no-logging path visually and mechanically allocation-free.

### Resolver Cache Capacity

The vec cache avoids tree allocation, but it still grows dynamically. The engine
could reserve a small default capacity or reuse cache capacity across frames.

This should be measured carefully. The current cache is rebuilt every frame, but
capacity reuse may already be happening through the owning `Resolver` if the vec
is cleared rather than replaced.

### Borrowed Typed Reads

`FromLpValue` now borrows `&LpValue`, which keeps scalar/product reads cheap.
Future typed slot APIs should preserve that direction:

- borrowed reads for hot paths;
- explicit clone/owned conversion only when the caller truly needs ownership;
- view/accessor APIs that make accidental generic-value cloning difficult.

## What Not To Chase Yet

- Do not hand-optimize every small `String::clone` in isolation. Some are error
  paths, formatting paths, or low-value compared with query-key compaction.
- Do not replace JSON/wire shapes to solve this specific profile. This profile
  is steady render, not project sync.
- Do not add a complex custom allocator only for resolver products before
  trying a simple frame arena or compact handles.

## Suggested Next Investigation

The next focused profiling pass should answer:

1. How many resolver queries are created per steady frame?
2. How many distinct produced/consumed slots are touched?
3. How many `Production` allocations happen per frame?
4. How much resolver cache capacity is allocated, reused, or grown per frame?
5. How much of the remaining `String::clone` is slot/bus identity vs diagnostic
   formatting?

A small resolver debug counter block would make this much easier than reading
cycle profiles alone.
