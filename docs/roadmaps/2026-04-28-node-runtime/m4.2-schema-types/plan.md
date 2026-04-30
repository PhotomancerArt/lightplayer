
---

> **Naming (planning onward):** `lpc-runtime` answers in this M4.2 doc denote
> **`lpc-engine`**. Authored literals / wire payloads align with **`ModelValue` /
> `SrcValueSpec`** per M4.3a–M4.3b (was `WireValue` before rename).

# Decisions for future reference

## `Binding::Literal(ValueSpec)` — wire-boundary-by-design

- **Decision:** `Binding::Literal` carries `ValueSpec` (the portable recipe), not `LpsValue` (the runtime handle-bearing type). The wire boundary intentionally uses `ValueSpec`; `LpsValue` is GLSL-runtime-only.
- **Why:** `LpsValue::Texture2D` carries a GPU handle that's meaningless across the wire. The design has `ValueSpec` as the portable form; teaching `LpsValue` serde would propagate the architectural smell deeper.
- **Rejected alternatives:** (a) Add serde to `LpsValueF32` — rejected because texture handles are fundamentally not portable; (b) Have `Binding::Literal` be a special wire form — rejected because `ValueSpec` already exists and works.
- **Revisit when:** The M4.3a/M4.3b work lands a focused portable value enum (`ModelValue`, formerly `WireValue`) that obsoletes both `ValueSpec` on the wire and the `LpsValueWire` private mirror in `value_spec.rs`.

## `PropAccess` uses `Box<dyn Iterator>`

- **Decision:** `PropAccess::iter_changed_since` and `snapshot` return `Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + '_>`.
- **Why:** Implementor simplicity wins. Sync is editor-only (not hot-path), `LpsValue` is small (no pixel bytes — `Texture2D` carries descriptor only), and the per-call `Box` is short-lived. Visitor pattern (zero-alloc) was considered but adds ergonomic cost for marginal gain on a non-hot path.
- **Rejected alternatives:** Visitor/callback pattern with `&mut dyn FnMut` — rejected for implementor-ergonomics cost on a non-hot path.
- **Revisit when:** If editor sync becomes a measurable frame-time cost on ESP32, benchmark and reconsider.

## `NodePropSpec` reuse vs new `NodePropRef`

- **Decision:** Reuse the existing `NodePropSpec` type (same shape as aspirational `NodePropRef`) rather than introduce a near-duplicate type.
- **Why:** The existing type already has the exact fields needed (`node: TreePath, prop: PropPath`), plus a `Display` impl and serde derives. Adding `target_namespace()` as an inherent method is cheaper than maintaining two types.
- **Rejected alternatives:** New `NodePropRef` type — rejected as unnecessary duplication.
- **Revisit when:** Never; the name difference is noted in design docs and the single type is the simpler path.

---

# Notes

Historical notes from plan development — kept for context.

## Triage table — confirmation-style questions (resolved)

| #   | Question                                                                                  | Context                                                              | Suggested answer                                              |
| --- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------- |
| Q1  | Directory name `m4.2-schema-types`?                                                       | Matches `m4.1-tree-spine-impl` style.                                | Yes.                                                          |
| Q2  | Put `Binding` expansion + `NodePropRef` + `NodeConfig` struct in `lpc-model`?             | They're wire-shared (project files, sync).                           | Yes — `lpc-model/src/prop/binding.rs` + `node/node_config.rs`. |
| Q3  | Put `ResolvedSlot` / `ResolveSource` / `ResolverCache` / `Bus` in `lpc-runtime`?          | Server-only data; no client mirror in M4.2.                          | Yes — under `lpc-runtime/src/resolver/` and `bus/`.            |
| Q4  | `BTreeMap` (not `hashbrown`) for `overrides`, `Bus.channels`, `ResolverCache`?            | M3 standardised on `BTreeMap`; consistency wins.                     | Yes.                                                          |
| Q5  | `Binding` serde uses per-variant rename so on-disk keys are `bus` / `literal` / `node`?   | Design 04 §Authoring shorthand shows `bind = { node = { ... } }`.    | Yes — `#[serde(rename = "node")]` on `NodeProp`.              |
| Q6  | `NodeConfig` skips `overrides` field on serialize when empty?                             | Most legacy nodes have empty overrides; keeps TOML clean.            | Yes — `#[serde(skip_serializing_if = "BTreeMap::is_empty")]`. |
| Q7  | Keep the existing `BindingResolver` trait stub in `binding.rs`?                           | Stub for compose-time channel-kind lookup. Real resolver is M4.3.    | Yes — leave as-is, M4.3 either fills it in or replaces it.    |
| Q8  | Leave `prop_cache: ResolverCache` and `prop_cache_ver: FrameId` on `NodeEntry` *commented*? | M4.1 left them as stubs; M4.3 wakes them up with the resolver.       | Yes — types now exist, but no field activation in M4.2.       |
| Q9  | `PropAccess` trait lives in `lpc-model/src/prop/prop_access.rs`?                          | Sibling to `prop_value.rs`; trait is used by sync layer (wire-side). | Yes.                                                          |
| Q10 | `Bus` lives in `lpc-runtime/src/bus/bus.rs` + `channel_entry.rs`?                         | Server runtime only; `ChannelName` is in `lpc-model::bus`.           | Yes — small files per type per repo style.                    |
| Q11 | Don't add `Slot.bind` Kind-validation in M4.2 (defer to resolver in M4.3)?                | Design 06 says lenient at first-use; resolver does the validation.   | Yes — defer.                                                  |
| Q12 | Skip `serde` for `Bus` / `ChannelEntry` (runtime-only, never on the wire)?                | M4.4 sync deltas don't ship bus state — they ship `PropsChanged`.    | Yes — pure runtime types, no serde derive.                    |
| Q13 | Skip `serde` for `ResolvedSlot` / `ResolverCache` (runtime cache, not wire)?              | Cache is rebuilt per-tick; clients see resolved values via sync.     | Yes — no serde.                                               |

## Discussion-style questions (resolved)

- **Q-A:** `PropAccess` shape is `Box<dyn Iterator>` per the design doc — see resolved decisions.
- **Q-B:** Five-method operational API on `Bus`; see resolved decisions.
- **Q-C:** Defer the namespace policy check to M4.3 config-load; ship `PropNamespace` enum + `NodePropSpec::target_namespace()` helper in M4.2 so the M4.3 check is one line. See resolved decisions.

## Resolved decisions table (historical)

| #   | Decision                                                                                                                                  |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Q1  | Directory name `m4.2-schema-types`. ✓                                                                                                     |
| Q2  | `Binding` expansion, `NodePropRef`, `NodeConfig` struct land in `lpc-model` (`prop/binding.rs` + `node/node_config.rs`).                  |
| Q3  | `ResolvedSlot` / `ResolveSource` / `ResolverCache` / `Bus` land in `lpc-runtime` (`resolver/` and `bus/`).                                 |
| Q4  | `BTreeMap` (not `hashbrown`) for `overrides`, `Bus.channels`, `ResolverCache`. Consistent with M4.1.                                       |
| Q5  | `Binding` serde uses per-variant rename: `bus` / `literal` / `node` (TOML form: `bind = { node = { ... } }`).                              |
| Q6  | `NodeConfig.overrides` skipped on serialize when empty (`#[serde(skip_serializing_if = "BTreeMap::is_empty")]`).                          |
| Q7  | Existing `BindingResolver` trait stub in `binding.rs` stays as-is. M4.3 either fills it in or replaces it.                                 |
| Q8  | `prop_cache: ResolverCache` and `prop_cache_ver: FrameId` on `NodeEntry` stay commented in M4.2; M4.3 wakes them up. Types exist now.     |
| Q9  | `PropAccess` trait lives in `lpc-model/src/prop/prop_access.rs`.                                                                          |
| Q10 | `Bus` lives in `lpc-runtime/src/bus/{bus.rs,channel_entry.rs}` (one type per file).                                                        |
| Q11 | `Slot.bind` Kind-validation deferred to the resolver in M4.3 (lenient at first-use; warn + fall through to default).                      |
| Q12 | `Bus` / `ChannelEntry` are runtime-only types — no serde derive.                                                                          |
| Q13 | `ResolvedSlot` / `ResolverCache` are runtime cache — no serde derive.                                                                      |
| Q-A | `PropAccess` uses `Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + '_>` for `iter_changed_since` and `snapshot`. Implementor simplicity wins; sync is editor-only (not hot-path), `LpsValue` is small (no pixel bytes — `Texture2D` carries descriptor only), and the per-call `Box` is short-lived. |
| Q-B | `Bus` ships a five-method operational API in M4.2: `new`, `claim_writer(channel, writer, prop, kind) -> Result<(), BusError>`, `publish(channel, value, frame)`, `read(channel) -> Option<&LpsValue>`, `last_writer_frame(channel) -> FrameId`, `kind(channel) -> Option<Kind>`. `BusError::KindMismatch` on first-writer-kind conflict. Round-trip test (claim → publish → read) lands in M4.2; M4.3 wires it into `TickContext`. |
| Q-C | `NodePropSpec` deserialize stays permissive (any well-formed `(node, prop)` pair). The "target must address `outputs`" policy lives at config-load in M4.3. M4.2 ships a `PropNamespace { Params, Inputs, Outputs, State }` enum + `NodePropSpec::target_namespace() -> Option<PropNamespace>` helper so the M4.3 check is a one-liner. Design 06 gets a note about this M4.3 deliverable. |
| —   | **Implementation reconciliations** (caught while writing phases): (a) reuse existing `NodePropSpec` rather than create a new `NodePropRef`; (b) `Binding::Literal(ValueSpec)` not `Binding::Literal(LpsValue)`. The latter is **wire-boundary-by-design**, not an M2 workaround: `LpsValue::Texture2D` carries a runtime handle that's not meaningful across the wire, and `ValueSpec` is the existing portable form. Phase 3 documents this framing in design 04 / 06. The deeper crate split + a focused portable value enum (`ModelValue`) is M4.3a–M4.3b (see [`../m4.3a-crate-split-wire-value/plan.md`](../m4.3a-crate-split-wire-value/plan.md), [`../m4.3b-name-alignment/00-design.md`](../m4.3b-name-alignment/00-design.md)).
