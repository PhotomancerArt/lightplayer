### What was built

- Runtime produced/consumed endpoints now use `SlotPath` for slot identity instead of `ValuePath`.
- Legacy detail sync vocabulary is explicitly named with `Legacy*` types and `legacy_detail_*` accessors.
- Generic slot-watch wire/view vocabulary exists alongside legacy detail sync.
- `SlotMeta` now includes positive `writable` metadata for future mutation/editor flows.
- Focused model, wire, view, engine, client, and server checks/tests pass.

### Decisions for future reference

#### SlotPath for runtime endpoints

- **Decision:** Produced and consumed runtime endpoints use `SlotPath`; authored resolver defaults and overrides continue to use `ValuePath`.
- **Why:** Slots are the versioned production/consumption boundary, while `ValuePath` describes projection inside a leaf value.
- **Rejected alternatives:** Keeping runtime endpoints on `ValuePath`; converting the authored resolver cascade in this prep milestone.
- **Revisit when:** Real source defs and runtime state roots are exposed through slot access.

#### Legacy detail remains compatibility-only

- **Decision:** Existing node-detail sync keeps working, but public names make the legacy boundary obvious.
- **Why:** The cutover needs old debug/resource tests to keep passing while new generic slot sync grows beside it.
- **Rejected alternatives:** Removing legacy detail sync in M1; leaving old neutral names that look canonical.
- **Revisit when:** Generic slot sync can power the debug UI.

#### Watch vocabulary starts at roots

- **Decision:** `WireSlotWatchSpecifier` selects node/root pairs, plus `AllState` and `All` conveniences.
- **Why:** Root-level watches match the slot/version boundary without committing to deep mutation syntax yet.
- **Rejected alternatives:** Reusing legacy node-detail selectors; encoding watches as unstructured strings.
- **Revisit when:** M2.2/M3 deletes the old project sync path and rebuilds canonical slot-first messages.
