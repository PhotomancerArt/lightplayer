# Phase 5: CLI `Symbolizer` extension

> Read `00-notes.md` and `00-design.md` for shared context.
> Depends on phase 3 (`meta.json` now contains a `dynamic_symbols` field).

## Scope of phase

Extend `lp-cli`'s profile `Symbolizer` so it reads `dynamic_symbols`
from `meta.json` and resolves PCs that fall inside JIT'd code to real
function names instead of `<jit:0x...>`.

### In scope

- `lp-cli/src/commands/profile/symbolize.rs`:
  - Extend the `meta.json` deserialization shape to include
    `dynamic_symbols: Vec<DynamicSymbol>` with default-empty serde
    behavior.
  - `Symbolizer` carries both static and dynamic symbol lists, each
    stored as a sorted `(start, end, name)` interval list.
  - `Symbolizer::lookup` (or its current equivalent) consults static
    first, then dynamic, before falling back to `<jit:0x...>` /
    `<unknown:0x...>`.
- Whatever the `Symbolizer` constructor is (e.g. `Symbolizer::new`,
  `from_meta`), it now accepts (or reads) both arrays from the same
  `meta.json` blob.
- Unit tests:
  - PC resolved via static symbols (regression).
  - PC resolved via dynamic symbols.
  - PC outside any range falls back to `<jit:...>` or `<unknown:...>`
    per existing convention.
  - Static symbol wins on PC overlap with a dynamic one.

### Out of scope

- alloc-trace `SymbolResolver` changes (phase 6).
- Anything in `lp-riscv-emu` (phase 3 already wrote the field).
- Re-emitting old profiles or back-compat handling for traces produced
  before m5 — the `default` serde attribute is sufficient.

## Code Organization Reminders

- Tests at the **top** in a `mod tests` block.
- Public types and the `Symbolizer` impl first; helpers (sorted
  interval search etc.) at the **bottom**.
- Don't duplicate the static-vs-dynamic interval-search logic — write
  one helper that takes a slice of intervals and a PC, and call it
  twice.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** weaken or skip existing `Symbolizer` tests.
- Do **not** suppress warnings.
- Do **not** change the `<jit:0x...>` / `<unknown:0x...>` fallback
  strings — they're the format consumed by report.txt and the flame
  chart renderer.
- If anything is ambiguous (especially around the existing `meta.json`
  schema), stop and report.
- Report back: files changed, validation output, any deviations.

## Implementation Details

### 1. Find the current shape

Open `lp-cli/src/commands/profile/symbolize.rs`. Locate:

- The serde struct used to deserialize `meta.json` (likely something
  like `MetaJson` / `TraceMeta` — exact name varies).
- The `Symbolizer` struct, its constructor, and its `lookup` method.

The current symbol list almost certainly comes from a static
`symbols: Vec<TraceSymbol>` (or similar) field on `meta.json`. The
existing interval-lookup logic is the model for the new dynamic
lookup.

### 2. Extend the meta-json shape

Add to the same struct:

```rust
#[serde(default)]
pub dynamic_symbols: Vec<DynamicSymbol>,
```

with:

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DynamicSymbol {
    pub addr: u64,
    pub size: u64,
    pub name: String,
    #[serde(default)]
    pub loaded_at_cycle: Option<u64>,
    #[serde(default)]
    pub unloaded_at_cycle: Option<u64>,
}
```

(Match field names exactly to the JSON written in phase 2's
`JitSymbols::to_json`. Cycle fields are present in m5 but not used at
lookup; deserialize with defaults so older traces still load.)

### 3. Extend `Symbolizer`

Give `Symbolizer` two interval lists:

```rust
pub struct Symbolizer {
    static_intervals: Vec<(u64, u64, String)>,   // (lo, hi, name)
    dynamic_intervals: Vec<(u64, u64, String)>,
}
```

Construct both in the existing constructor (sort by `lo`):

```rust
fn build_intervals<I, F>(items: I, f: F) -> Vec<(u64, u64, String)>
where
    I: IntoIterator,
    F: Fn(I::Item) -> (u64, u64, String),
{
    let mut v: Vec<(u64, u64, String)> = items.into_iter().map(f).collect();
    v.sort_by_key(|(lo, _, _)| *lo);
    v
}
```

`lookup` becomes a static-first then dynamic search:

```rust
pub fn lookup(&self, pc: u64) -> SymbolName<'_> {
    if let Some(name) = lookup_in_intervals(&self.static_intervals, pc) {
        return SymbolName::Resolved(name);
    }
    if let Some(name) = lookup_in_intervals(&self.dynamic_intervals, pc) {
        return SymbolName::Resolved(name);
    }
    SymbolName::Unresolved(pc) // existing fallback shape
}
```

(Match the actual return type — could be `String`, `Cow<str>`, or a
custom enum. Don't change the public signature unless absolutely
necessary; if you do, update all callers and document why.)

`lookup_in_intervals` is a binary search by `lo` then a `pc < hi`
check — match whatever the existing `lookup` does for static symbols.
Don't reinvent; refactor the existing one into the helper.

### 4. Tests

Add four tests to the existing `mod tests` (top of file):

```rust
#[test]
fn lookup_static_symbol_wins() {
    let s = Symbolizer::for_test(
        vec![sym(0x1000, 0x10, "static_fn")],
        vec![dyn_sym(0x1000, 0x10, "dyn_fn")],
    );
    assert_eq!(s.lookup(0x1000).resolved(), Some("static_fn"));
}

#[test]
fn lookup_dynamic_symbol_resolves() {
    let s = Symbolizer::for_test(
        vec![],
        vec![dyn_sym(0x8000_0000, 0x40, "palette_warm")],
    );
    assert_eq!(s.lookup(0x8000_0010).resolved(), Some("palette_warm"));
}

#[test]
fn lookup_falls_back_when_unresolved() {
    let s = Symbolizer::for_test(vec![], vec![]);
    assert!(matches!(s.lookup(0x1234), SymbolName::Unresolved(0x1234)));
}

#[test]
fn lookup_static_does_not_shadow_disjoint_dynamic() {
    let s = Symbolizer::for_test(
        vec![sym(0x1000, 0x10, "static_fn")],
        vec![dyn_sym(0x8000_0000, 0x10, "dyn_fn")],
    );
    assert_eq!(s.lookup(0x8000_0008).resolved(), Some("dyn_fn"));
}
```

(Adjust constructor / accessor names to actual API.)

Add a small integration-style test that loads a synthetic `meta.json`
blob (string literal, then `serde_json::from_str`) containing both
`symbols` and `dynamic_symbols`, builds a `Symbolizer`, and asserts
both lookups work.

## Validate

```bash
cargo test -p lp-cli
cargo build -p lp-cli
```

Both must succeed cleanly with no warnings. All pre-existing
`lp-cli` profile tests must still pass.
