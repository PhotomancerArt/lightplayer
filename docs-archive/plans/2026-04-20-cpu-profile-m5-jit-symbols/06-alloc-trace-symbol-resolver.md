# Phase 6: Alloc-trace `SymbolResolver` extension

> Read `00-notes.md` and `00-design.md` for shared context.
> Depends on phase 3 (`meta.json` now contains a `dynamic_symbols` field).

## Scope of phase

The alloc-trace report has its own symbol-resolution path
(`lp-riscv/lp-riscv-emu/src/profile/alloc.rs`) separate from the CLI
symbolizer. Bring it to parity: load `dynamic_symbols` from
`meta.json` and merge them into the resolver's sorted interval list.

### In scope

- `lp-riscv/lp-riscv-emu/src/profile/alloc.rs`:
  - Extend `TraceMetaSymbols` (or whatever struct deserializes
    `meta.json` here) to include `#[serde(default)] dynamic_symbols:
    Vec<DynamicSymbolJson>` matching the JSON written in phase 2.
  - `SymbolResolver::load` (or its current entry point) merges static
    + dynamic symbols into a single sorted interval list. Static wins
    on PC overlap (insert order: static first, then dynamic; lookup
    uses first-match in the sorted list — break ties by stable sort
    + insertion order).
  - Field naming should mirror phase 5 (`addr`, `size`, `name`,
    optional `loaded_at_cycle`, optional `unloaded_at_cycle`).
- Unit tests:
  - Static-only lookup (regression).
  - Dynamic-only lookup.
  - Static-wins on overlap.
  - Default empty `dynamic_symbols` when field absent (back-compat).

### Out of scope

- CLI `Symbolizer` (phase 5).
- Any change to `JitSymbols::to_json` schema (phase 2 owns it).
- Touching the alloc-trace report writer beyond what's needed for the
  resolver to compile.

## Code Organization Reminders

- Tests at the **top** in a `mod tests` block.
- Public types and `SymbolResolver` impl first; sort/merge helpers at
  the **bottom**.
- If the existing alloc-trace JSON shape is in a separate file from
  the resolver, keep the new `DynamicSymbolJson` next to the existing
  `TraceMetaSymbols`.
- Don't duplicate the binary-search logic the CLI symbolizer uses;
  but **don't share code across crates** for this either — it's a
  small enough function. Inline a local copy here.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** weaken or skip existing alloc-trace tests.
- Do **not** suppress warnings.
- The existing `<jit:0x...>` / `<unknown:0x...>` (or whatever the
  alloc-trace fallback prints) must continue to be the fallback. Don't
  change the rendered string format.
- If anything is ambiguous, stop and report.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. Locate the current shape

Open `lp-riscv/lp-riscv-emu/src/profile/alloc.rs`. Find:

- `TraceMetaSymbols` — the deserialize struct that pulls `symbols` out
  of `meta.json`.
- `SymbolResolver` — the type that holds the sorted interval list and
  exposes a `lookup(pc) -> ...` method.
- Wherever `SymbolResolver` is constructed (likely `SymbolResolver::load`
  or `from_meta` reading `meta.json` from disk).

### 2. Extend the serde struct

```rust
#[derive(serde::Deserialize, Debug)]
struct TraceMetaSymbols {
    // existing fields
    symbols: Vec<TraceSymbol>,

    #[serde(default)]
    dynamic_symbols: Vec<DynamicSymbolJson>,
}

#[derive(serde::Deserialize, Debug)]
struct DynamicSymbolJson {
    addr: u64,
    size: u64,
    name: String,
    #[serde(default)]
    loaded_at_cycle: Option<u64>,
    #[serde(default)]
    unloaded_at_cycle: Option<u64>,
}
```

### 3. Merge into the resolver

In `SymbolResolver::load` (or wherever the static intervals are built):

```rust
let mut intervals: Vec<Interval> = Vec::with_capacity(
    meta.symbols.len() + meta.dynamic_symbols.len(),
);

for s in &meta.symbols {
    intervals.push(Interval {
        lo: s.addr,
        hi: s.addr + s.size,
        name: s.name.clone(),
        kind: SymbolKind::Static,
    });
}

for d in &meta.dynamic_symbols {
    intervals.push(Interval {
        lo: d.addr,
        hi: d.addr + d.size,
        name: d.name.clone(),
        kind: SymbolKind::Dynamic,
    });
}

// Stable sort: keeps insertion order on equal `lo`, so a static
// interval with the same `lo` as a dynamic one is searched first.
intervals.sort_by_key(|i| i.lo);
```

If `Interval` doesn't already exist, define it as a small private
struct with `lo`, `hi`, `name`. The `kind` field is optional — only
add it if you need to disambiguate in the report output. m5 doesn't.

The lookup method stays the same shape (binary search by `lo`, then
linear walk back / forward to find the interval containing `pc`).

### 4. Tests

Add to the existing `mod tests`:

```rust
#[test]
fn dynamic_symbol_resolves() {
    let resolver = SymbolResolver::from_meta_json(r#"{
        "symbols": [],
        "dynamic_symbols": [
            { "addr": 2147483648, "size": 64, "name": "palette_warm",
              "loaded_at_cycle": 0, "unloaded_at_cycle": null }
        ]
    }"#).expect("parse");
    assert_eq!(resolver.lookup(0x8000_0010), Some("palette_warm"));
}

#[test]
fn static_wins_on_overlap() {
    let resolver = SymbolResolver::from_meta_json(r#"{
        "symbols": [{ "addr": 4096, "size": 16, "name": "static_fn" }],
        "dynamic_symbols": [
            { "addr": 4096, "size": 16, "name": "dyn_fn",
              "loaded_at_cycle": 0, "unloaded_at_cycle": null }
        ]
    }"#).expect("parse");
    assert_eq!(resolver.lookup(0x1000), Some("static_fn"));
}

#[test]
fn missing_dynamic_symbols_field_is_back_compat() {
    let resolver = SymbolResolver::from_meta_json(r#"{
        "symbols": [{ "addr": 4096, "size": 16, "name": "static_fn" }]
    }"#).expect("parse");
    assert_eq!(resolver.lookup(0x1000), Some("static_fn"));
    assert!(resolver.lookup(0x8000_0000).is_none());
}
```

(Field name `from_meta_json` is illustrative — match whatever
constructor the resolver actually exposes for tests. If there's no
test-friendly constructor, factor one out, but don't change the
production constructor's signature.)

## Validate

```bash
cargo test -p lp-riscv-emu --lib alloc
cargo build -p lp-riscv-emu
```

Both must succeed cleanly with no warnings. All pre-existing
alloc-trace tests must still pass.
