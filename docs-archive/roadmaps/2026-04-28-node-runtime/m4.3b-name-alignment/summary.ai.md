### What was built

- Renamed shared portable representations in `lpc-model` from `WireValue` / `WireType` / `WireStructMember` to `ModelValue` / `ModelType` / `ModelStructMember` and aligned `Kind::storage()` plus module filenames.
- Removed `NodeProps`, `NodeSpecifier`, and `nodes` shim exports from `lpc-model`; call sites use `NodeSpec` and normal module paths.
- Dropped `lpc-source` crate-root shorthand aliases; exported authored types use `Src*` names including `SrcArtifact` / `SrcArtifactSpec`.
- Renamed wire disambiguators in `lpc-wire` to `WireNodeSpecifier` and `WireSlotIndex` where parallel concepts exist elsewhere.
- Renamed `lpc-view` cache types to suffix `*View` style (`ProjectView`, `NodeEntryView`, `NodeTreeView`, `TreeEntryView`, `PropAccessView`, `PropsMapView`, `StatusChangeView`).
- Renamed engine conversion helpers and bridge modules to `lps_value_f32_to_model_value` and `model_type_to_lps_type` (and mirrored filenames).
- Updated crate READMEs and node-runtime roadmap/design references for the selective-prefix policy.
- Phase 7: aligned active `m4.2-schema-types/plan.md` forward-guidance bullets with `ModelValue` / M4.3b (left M4.3a archive text historically accurate where it documents that milestone).

### Decisions for future reference

#### `Model*` value vocabulary in `lpc-model`

- **Decision:** Portable structural literals and serde shapes are named `ModelValue`, `ModelType`, and `ModelStructMember` in `lpc-model`, not `Wire*`.
- **Why:** The types are shared across source, wire payloads, engine conversion, and view caches; `Wire*` implied `lpc-wire` ownership and read as wire-only plumbing.
- **Rejected alternatives:** Keep `WireValue`/`WireType`; rename to `Core*` or `ValueShape`/`TypeShape`.
- **Revisit when:** A future split separates “serde disk shape” from “normalized model shape” badly enough that two public enums are justified.

#### Prefix policy (`Src*` / selective `Wire*` / `*View`)

- **Decision:** Use `Src*` for authored-source exports; reserve `Wire*` on the protocol only where the same noun exists in model/source/view/engine form; use natural `*View` suffixes for client cache structs; keep directional envelope names like `ClientMessage` / `ServerMessage` without forcing a `Wire*` prefix.
- **Why:** Readable at call sites beats blanket crate-prefixing every type; ambiguity drives the prefix, not the directory name alone.
- **Rejected alternatives:** Prefix every public wire type with `Wire*`; rename all `Client*`/`Server*` envelopes; migrate view types to `View*` prefixes instead of suffixes.
- **Revisit when:** New parallel types make direction-only names ambiguous in practice.

#### No shared-root compatibility aliases

- **Decision:** After M4.3b, do not reintroduce type aliases (`NodeSpecifier = …`, `ValueSpec = …`, etc.) at `lpc-model`/`lpc-source` crate roots for old names—update imports and docs instead.
- **Why:** Aliases obscure the authoritative name and regress editor/search hygiene.
- **Rejected alternatives:** Keep aliases “just for ergonomics” or gate them behind deprecation attributes without removing callers.
- **Revisit when:** A stable external FFI or published crate boundary requires explicit re-export stability guarantees (then document narrowly, still avoid duplicate spellings in first-party code).
