---
status: fixed
found: 2026-07-27      # how: discovery for the node-authoring plan
fixed: pending         # lands with the node-authoring change set
area: lpa-studio-core/library
class: backend-contract-divergence
related: []
---
# LibraryStore::create wrote a manifest the loader rejects

**Symptom** — A package created through `CatalogOp::Create` could never
be opened: `ProjectRegistry::load_root` rejects its `project.json` at
the root format gate (`found: None`). Never user-visible only because
no UI reached `Create` — the gallery had no create action (D17).

**Root cause** — `LibraryStore::create`'s minimal manifest was
`{"kind":"Project","name":…}`, with no `format` key, while
`check_root_format` requires `format: 1` on every project root. The
writer and the loader disagreed on the minimum viable manifest, and no
test crossed the two: creation was unit-tested against the store, load
was unit-tested against fixtures, and nothing created-then-loaded. The
same format-less shape was baked into a preview-host test fixture,
propagating the wrong contract.

**Fix** — The minimal manifest now carries
`"format": lpc_model::PROJECT_FORMAT_VERSION` (canonical key order
preserved by `package_manifest`); the preview-host fixture was
corrected to match. The blank-project flow (`HomeOp::CreateProject`,
same change set) is the first UI consumer.

**Regression coverage** — `created_package_loads_through_project_registry`
(library store → real `ProjectRegistry::load_root` round-trip) and the
studio e2e `home_create_project_creates_and_opens_a_blank_package_end_to_end`.

**Lesson** — A writer whose output no reader ever consumes in tests is
an unverified contract. When a factory produces artifacts for a gate
elsewhere in the system, at least one test must push the factory's
output through the real gate — especially when the factory is
"unreachable from UI today", because that is exactly the code a later
feature wires up without re-auditing.
