# Architecture Decision Records

Architecture Decision Records, or ADRs, capture durable architecture and process
decisions for this repo.

Use ADRs for decisions that choose a direction among plausible alternatives and
have lasting architectural, operational, security, data-model, API, workflow,
product, embedded, or cross-repo/process consequences.

Do not create ADRs for ordinary feature work, bug fixes, UI copy/layout
changes, mechanical refactors, tests, scripts, helpers, or phase sequencing
unless they set a broader precedent.

## Filename

Use date-based filenames:

```text
YYYY-MM-DD-short-title.md
```

Date-based names keep files sortable and reduce conflicts between parallel
branches.

## Status

Use one of:

- `Proposed`
- `Accepted`
- `Superseded`
- `Rejected`
- `Deferred` — a design-heavy decision consciously postponed; pair it with a
  "Revisit when" trigger and list it in the Deferred Decisions index below.

Treat ADRs as durable history. If a decision changes, create a new ADR that
supersedes the old one instead of rewriting old context heavily.

## Deferred Decisions

Small deferrals live in the creating ADR's **Follow-ups** section; design-heavy
deferrals get their own ADR with `Status: Deferred` and a "Revisit when" line;
and this index is the one place that tracks every open cross-ADR follow-up so a
deferral is never silently lost — new ADRs add their open items here, and a
follow-up is struck from the table once it lands (or is checked off in its
source ADR).

The table below is the open-follow-ups/deferred index built in M7/P4 by scanning
every ADR's Follow-ups section. It lists the still-open items that carry a
recognizable revisit trigger; each row points back to its source ADR, which
holds the full context.

| Item | Source ADR | Revisit trigger |
|---|---|---|
| Per-item overlay gating (fetch-full-on-change assumes small overlays) | `2026-07-04-studio-editing-model` (a) | Measured overlay fetch cost matters |
| Save-panel diff DTOs (before/after values; M1 provides counts only) | `2026-07-04-studio-editing-model` (b) | Roadmap M3 (Save panel) |
| Composite edit semantics (map add/remove, option some/none, variant switch) — extend the editing-model ADR if precedent is set | `2026-07-04-studio-editing-model` (c) | Roadmap M3 |
| Singular `ProjectRegistry::mutate` bypasses policy/type validation (only `mutate_batch` enforces) | `2026-07-04-studio-editing-model` (d) | Any new caller of `mutate` |
| Event-driven (postMessage/waker) receive so the poll-sleep loop retires (~50–100 line bridge across `browser_worker_client_io.rs`, `worker_handle.rs`, JS worker) | `2026-07-04-client-pull-loop-and-actor` (a); `2026-07-03-simulator-clock-ownership` | Poll latency shows up in traces, or battery/CPU cost matters |
| Probe payload optimization (binary/compressed preview encoding, downscaled extents, delta frames) | `2026-07-04-client-pull-loop-and-actor` (b) | Steady-state tick cost is dominated by raw probe bytes; own design pass with measurements |
| Native/tokio actor parity: `tokio::spawn`/`LocalSet` spawn helper + native timer factory | `2026-07-04-client-pull-loop-and-actor` (c) | A native Studio shell exists |
| Layout-header semantic chunking (per-lamp-range events) | `2026-07-04-envelope-streaming` | Display-layout fixtures grow ~4×+ past the 16 KiB frame budget |
| Sub-root slot progressive patching | `2026-07-04-envelope-streaming`; `2026-06-27-project-read-event-frames` | `SlotMirrorView` can apply partial root snapshots safely |
| Real-hardware Studio smoke of the gated multi-frame serial read | `2026-07-04-envelope-streaming` | Post-merge hardware validation pass |
| Binary/compressed payload encoding for project-read frames | `2026-06-27-project-read-event-frames`; `2026-06-27-ser-write-json-raw-value` | JSON/base64 overhead becomes material after the bounded-transport contract settles |
| Membership-only `ids_revision` bump (strictly on id add/remove) | `2026-07-03-revision-gated-project-reads` | A correctness-neutral chattiness lean-out is worth doing |
| Flatten the now single-variant `AssetSlotValue` enum; directory-per-node layout | `2026-07-04-json-only-artifacts` | Studio editing work touches asset/node layout |
| ELF-symbol `Content` guardrail check in CI | `2026-07-04-json-only-artifacts` | CI ground-truth guardrail is prioritized |
| Concrete `UxRegistry`; operation-metadata derive macros | `2026-06-21-studio-ux-layer` | Dynamic UX nodes need registration/dispatch, or the manual op-metadata model has more usage pressure |
| `Ui*`→`*View` / `*Ux`→`*Controller` / `App*`→domain-noun renames | `2026-06-24-studio-core-and-layer-vocabulary` | The crate/layer refactor reaches the naming pass |
| Host-serial ESP32 management; self-hosted/vendored browser esptool; raw LittleFS backup/restore; long-management cancel/retry | `2026-06-22-studio-link-management-workflow` | Host-serial support, offline builds, backup, or flash/erase recovery is prioritized |
| Cancellation/retry affordances and section-aware Device activity | `2026-06-22-studio-device-ux-workflow`; `2026-06-22-studio-link-management-workflow` | Hardware workflows settle and need finer recovery control |
| CI/browser tooling for `wasm-bindgen-test`/Playwright worker smoke | `2026-06-17-browser-firmware-runtime`; `2026-06-17-studio-link-and-local-runtimes` | Browser-runtime CI execution is provisioned |

## Relationship To Shared Planning

Plans, roadmap-level plans, reviews, reports, scratch notes, and phase prompts
live in the personal planning workspace configured by `PHOTOMANCER_PLANNING_ROOT`
or `~/.photomancer/planning`.

Only durable decisions graduate into `docs/adr/`.
