# Phase 3: Slot Watch Wire/View Vocabulary

## Scope Of Phase

In scope:

- Add wire types for slot-root watch interest.
- Add the watch specifier to `WireProjectRequest::GetChanges` alongside legacy detail.
- Add view-side fields/helpers to track slot watches separately from legacy detail.
- Add tests for serde/default behavior and basic view helpers.

Out of scope:

- Making the engine honor slot watch requests.
- Sending slot root snapshots/patches in project sync.
- UI rendering of watched slot roots.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add a new wire module, likely:

- `lp-core/lpc-wire/src/project/wire_slot_watch_specifier.rs`

Define:

```rust
pub enum WireSlotRootKind {
    Source,
    State,
    Params,
    Output,
}

pub struct WireNodeSlotRoot {
    pub node: NodeId,
    pub root: WireSlotRootKind,
}

pub enum WireSlotWatchSpecifier {
    None,
    AllState,
    All,
    ByRoots(Vec<WireNodeSlotRoot>),
}
```

Requirements:

- `WireSlotWatchSpecifier` defaults to `None`.
- Serde names should be stable and readable, likely `snake_case`.
- Re-export from `lpc-wire/src/project/mod.rs` and `lpc-wire/src/lib.rs`.
- Add `slot_watch_specifier` to `WireProjectRequest::GetChanges` with `#[serde(default)]`.
- Update request construction call sites and tests to pass/default it.
- Add `ProjectView` fields/helpers for slot watch state, separate from `legacy_detail_tracking`.

Suggested `ProjectView` helpers:

- `watch_slot_root(WireNodeSlotRoot)`
- `unwatch_slot_root(&WireNodeSlotRoot)`
- `slot_watch_specifier() -> WireSlotWatchSpecifier`

The helper names may live in view and use wire types directly for now.

## Validate

```bash
cargo fmt -p lpc-wire -p lpc-view -p lpa-client -p lp-cli
cargo test -p lpc-wire
cargo test -p lpc-view
cargo check -p lpa-client
cargo check -p lp-cli
```

