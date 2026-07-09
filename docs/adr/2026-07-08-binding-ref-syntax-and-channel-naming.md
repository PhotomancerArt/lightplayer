# ADR: URI-Style Binding Refs and Bus Channel Naming Norms

- **Status:** Accepted
- **Date:** 2026-07-08
- **Deciders:** Photomancer
- **Supersedes:** the dot-based relative node-ref syntax
  (`..shader#output`), the `bus#` prefix, and the never-adopted
  `<kind>/<dir>/<index>` channel convention (archived quantity.md §11)
- **Superseded by:** None

## Context

Binding endpoints were authored as `<owner>#<slot-path>`: `bus#channel`
or a slashless relative node ref (`..shader#output`, `..#entry_time`).
This had three problems, surfaced while building the Studio bus pane and
binding indicators (2026-07-06 roadmap):

1. **The bus pretended to be a node.** `bus#…` used the node-ref
   grammar and `BusSlotRef` stored a `SlotPath`, but the bus is a
   virtual, project-global patch bay — a different kind of thing than a
   sibling node, and dressing it as one made both harder to explain.
2. **The slashless node path reads wrong.** `..shader` for "sibling
   named shader" defies filesystem intuition; in practice it reads as a
   typo. The dot syntax also collided visually with dotted slot paths.
3. **The documented channel convention was a dead letter.**
   `ChannelName`'s docs prescribed `<kind>/<dir>/<index>`
   (`audio/in/0`); nothing ever used it. Practice went dotted and
   name-based (`time.seconds`, `trigger`, `visual.out`) — but with
   inconsistencies (`time.delta_seconds` puts a non-unit where the unit
   convention says unit; unit-in-name duplicates what slot metadata
   already knows).

## Decision

**URI-style refs, one meaning per symbol:**

```text
bus:<channel>              channel = purpose[.in|.out][/instance]
node:<path>#<slot.path>    path = filesystem-style; / = tree, .. = parent
```

- `:` introduces the scheme; `/` is node-tree (and channel-instance)
  hierarchy; `#` is "the slot within the addressed thing"; `.` is field
  structure — the same dotted syntax slots use everywhere else
  (`entries[2].trigger`).
- Node paths are slashful and relative (`node:../shader#output`,
  `node:..#entry_time`, `node:child/grandchild#out`). A leading `./`
  normalizes away. Absolute (`/`-rooted) refs are rejected until the
  model grows root-anchored resolution. Multi-hop parents (`../../x`)
  now parse.
- `#` is **reserved in bus refs** for a future field-within-channel
  fragment (`bus:pose#head.x`) and rejected today. `BusSlotRef` stores a
  `ChannelName`, not a `SlotPath` — the channel's full string is its
  identity.
- Refs without a recognized scheme fail with "binding ref must start
  with `bus:` or `node:`". The retired syntaxes fail loudly, never
  reinterpret.

**Channel naming norms (convention, not enforcement):**

- **Unitless canonical names**: `time`, `time.delta` (renamed from
  `time.seconds`, `time.delta_seconds`). Unit truth lives in slot
  metadata and the well-known channel registry; pickers and popovers
  display it ("time — seconds (f32)"); binding-time validation warns on
  writer-unit vs channel-unit mismatch (M4). Unit segments stay legal to
  mark deviating channels (`time.millis`).
- **`.in`/`.out` only on project-boundary channels** (`visual.out`
  toward fixtures, `visual.in` from a camera). Interior channels
  (`time`, `trigger`) carry no direction — internally every channel has
  writers and readers, so direction only means something at the
  boundary. This is what killed the old convention: it forced direction
  onto everything.
- **`/instance`** for parallel channels, name or number
  (`visual.out/left`, `visual.out/2`); the unadorned name is the primary
  instance, so the primary-product convention (D6: primary visual =
  provider of `visual.out`) is unchanged. The editing UX proposes `/2`
  when a second writer appears.
- **UI transport events** use the `transport.*` family
  (`transport.next`, `transport.prev`, `transport.pause`) — not
  `control.*`, which is the control-product stream (`control.out`).
  `trigger` stays bare (grandfathered).
- A **well-known channel registry in code** (purpose, kind, unit, doc
  per channel) powers the picker, mismatch warnings, instance
  proposals, and `default_bind` targets. Arbitrary names remain legal —
  channels are created lazily by reference; the UX teaches the norm
  rather than a validator gatekeeping it.

## Migration

One atomic commit, hard cutover (standing policy: no wire/disk compat
during heavy development): all example/test projects, embedded test
JSON, story fixtures, and UI display strings rewritten; `time.seconds`
→ `time` everywhere including the loader default-binding helpers. The
old syntax parses to errors, not silent reinterpretation.

## Consequences

- Node-card chips and bus-pane rows read `bus:time`, `bus:trigger`,
  `node:..#entry_time`. Node refs got longer (`node:` prefix) — fine:
  they are rare and read-only in the editor (D1, bus-only authoring);
  explicitness is the point.
- The compound rename shortens the most-authored string
  (`bus#time.seconds` → `bus:time`).
- `RelativeNodeRef` keeps its shape (`parent_hops` + segments) but
  gains multi-hop parsing; the "slash is reserved for files" doctrine
  is repealed for node refs.
- M2's binding-graph wire surface is unchanged structurally; only
  channel-name strings differ.
- Future work slots in without new syntax: absolute node refs
  (`node:/fixture#input`), bus value fragments (`bus:pose#head.x`),
  instance channels (`bus:visual.out/left`).
