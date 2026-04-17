# Phase 3 — Call graph + topological order

## Scope of phase

Add **`lpir/src/inline/callgraph.rs`**: the data the orchestrator needs
to walk functions bottom-up and to skip recursive cycles cleanly.

This phase is purely additive analysis — it does not mutate the
module — so it can be tested in isolation against parsed LPIR
fixtures.

## Code Organization Reminders

- One file: `lpir/src/inline/callgraph.rs`. Internal to **`inline`**;
  not re-exported.
- Use **`alloc::vec::Vec`** + **`alloc::collections::BTreeMap`** /
  **`BTreeSet`** for determinism in **`#![no_std]`**. Avoid hash maps
  in core data.
- Edges only follow **`CalleeRef::FuncId`** — **`CalleeRef::ImportId`**
  is treated as an external leaf (no edge added).

## Implementation Details

### Public surface (crate-private)

```rust
pub(crate) struct CallGraph {
    /// callees_of[caller] = sorted, deduplicated list of local FuncIds called.
    pub callees_of: BTreeMap<FuncId, Vec<FuncId>>,
    /// callers_of[callee] = sorted, deduplicated list of local FuncIds calling it.
    pub callers_of: BTreeMap<FuncId, Vec<FuncId>>,
    /// Per-call-site list parallel to body order, for splicer iteration.
    pub call_sites_of: BTreeMap<FuncId, Vec<(usize, FuncId)>>,
}

pub(crate) fn build(module: &LpirModule) -> CallGraph;

/// Returns (topo_order, cyclic_set).
/// topo_order: leaves-first ordering of FuncIds reachable in a DAG.
/// cyclic_set: FuncIds participating in any cycle (skipped by inliner).
pub(crate) fn topo_order(g: &CallGraph) -> (Vec<FuncId>, BTreeSet<FuncId>);
```

`LpirModule::functions` is `BTreeMap<FuncId, IrFunction>` keyed by sparse
`FuncId(u16)` ids, so `BTreeMap<FuncId, _>` is the correct adjacency
shape. `CalleeRef::Local(FuncId)` is the local-call variant
(`CalleeRef::Import(ImportId)` is the external one — skipped here).

### `build`

- Iterate **`module.functions`**; for each function index **`f`**, walk
  **`func.body`** and collect every **`LpirOp::Call { callee:
  CalleeRef::FuncId(g), .. }`** along with its op index.
- Populate **`call_sites_of[f]`** in body order (no dedup — every call
  site is a distinct splice target).
- Populate **`callees_of[f]`** as the deduplicated, sorted set of
  `FuncId`s called from `f`. Same for **`callers_of`** in reverse.

### `topo_order`

- Kahn's algorithm; **leaves-first** = functions with **no outgoing
  local edges** come first.
- **`in_degree[g] = callees_of[g].len()`** (count of distinct local
  callees). Initial queue: all `g` with `in_degree == 0`.
- Pop the smallest `FuncId` from the queue into `topo_order`. For each
  `caller ∈ callers_of[g]`, decrement `in_degree[caller]`; push
  to the queue when it hits zero.
- Anything left with **`in_degree > 0`** after the queue drains is in a
  cycle (self-loops, mutual recursion, larger SCCs); collect those into
  **`cyclic_set`**.
- Determinism: process the queue in ascending **`FuncId`** order (use
  **`BTreeSet`** as the queue).

### Self-recursion is a cycle

A function that calls itself directly is a 1-cycle and lands in
**`cyclic_set`**. No special-casing needed — Kahn's handles it.

## Tests (`lpir` crate)

`tests/inline_callgraph.rs` (new):

- **`leaf`**: function calling no one → in `topo_order`, not in
  `cyclic_set`.
- **`linear_chain_a_b_c`**: A→B→C → topo order is `[C, B, A]`.
- **`diamond_a_bc_d`**: A→{B,C}, B→D, C→D → topo order is `[D, B, C, A]`
  or `[D, C, B, A]` (deterministic by `FuncId` order).
- **`self_recursive`**: A→A → A in `cyclic_set`, not in `topo_order`.
- **`mutual_recursion`**: A→B, B→A → both in `cyclic_set`.
- **`recursion_with_acyclic_tail`**: A→B, B→A, A→C → A and B in
  `cyclic_set`; C in `topo_order`.
- **`import_only_callee`**: A calls only an `ImportId` → A is a leaf
  (no edges out), in `topo_order`.
- **`multiple_call_sites_same_callee`**: A calls B twice →
  `callees_of[A] = [B]` (deduped); `call_sites_of[A]` has two entries
  with distinct op indices.

## Validate

```bash
cargo test -p lpir
```
