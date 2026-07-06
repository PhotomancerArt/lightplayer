# lpc-history

Lite versioning with events: project identity, canonical content hashing,
content-addressed snapshots, a per-project history event log, and lineage
queries. Pure `no_std` + `alloc` domain code — no IO beyond a caller-supplied
`LpFs`, no clock (timestamps are caller-supplied f64 epoch seconds), no
randomness (uid bytes are caller-supplied).

This crate is the versioning spine of studio project management: connecting a
device is a pull that lands in history, pushes are recorded with device and
optional location, and "behind vs diverged" is answered mechanically.

## The model

- **Identity** — `PrefixedUid`: prefixed base-62 identifiers
  (`prj_h7Kq9xY2mQ4tB8Wz`, `mod_…`, `dev_…`), 16-char body minted from 128
  caller-supplied random bits.
- **Canonical content hash** — SHA-256. A package's hash is the hash of its
  `TreeManifest`: the sorted `(path, file-hash)` listing, under a preimage
  tagged `lph1` (see `hash/tree_manifest.rs` for the byte-exact format).
  Everything under the reserved **`/.lp/`** namespace is excluded — that is
  where machine metadata (the provenance sidecar) lives, so metadata churn
  never destabilizes a version hash. The exclusion is part of the spec; there
  are no knobs.
- **Snapshots** — `SnapshotStore` over any `LpFs` history root: per-file
  content-addressed blobs (`blobs/<hex>`) plus tree manifests
  (`trees/<hex>.json`). Saves dedup at file level; `materialize` writes a
  stored version back out byte-identically.
- **Events** — `HistoryEvent` (JSONL in `events.jsonl`, torn-tail tolerant):
  one origin (`Created` / `ImportedZip` / `RemixedFrom` / `ForkedFrom`),
  then `Saved`, `Pushed { device, location? }`, and `Connected { device,
  observed }` observations. Events are the persistence format; the committed
  JSON samples in the tests pin it.
- **Lineage** — `ProjectHistory` replays events into a line and answers
  `head()`, `contains()`, and `classify(observed) -> AtHead | Behind |
  Diverged`. UI version numbers (v1…vN) are derived from the save sequence,
  never stored.

## Invariants

**History is linear per project.** A project's history is one line of
versions. Forks mint a *new project uid* whose history begins with a
`ForkedFrom` origin pointing at the parent project and version; there is no
DAG and no in-project branching. "Diverged" therefore means exactly "a hash
not in my line". Versions observed via `Connected` are *known* (valid fork
parents — a diverged device copy is banked at connect and can be adopted) but
never join the line.

**The head rule.** Editing the head of a line advances the line; editing
anything else forks — lazily, on first save. This crate does not enforce the
rule at edit surfaces (that wiring lives in the studio layers); it provides
the primitives — saves at the head, fork constructors for everything else —
that make the rule the only expressible behavior.

**No merge, ever.** Fast-forward detection plus fork-as-new-project is the
entire model, by design.

## Deliberately not here

Prune/GC policy for snapshots, location capture, the device registry
(only the `DeviceAssociation` data shape lives here), enforcement of the
head rule at edit surfaces, and any UI verbs. Where a project's history root
physically lives is the storage layer's concern; this crate takes a chrooted
`LpFs`.

The durable design record is the studio project-management ADR (pending —
the roadmap's final milestone consolidates it); until then this README and
the planning notes are the reference.
