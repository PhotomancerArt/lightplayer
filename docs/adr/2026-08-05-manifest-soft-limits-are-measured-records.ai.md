# Board-manifest soft limits are measured records, and warn-and-proceed

## Status

Accepted, 2026-08-05. Shipped with plan `2026-08-05-0901-pinmux-8wire-overlap`
(PR #356). Makes code of the posture `lpc_model::ManifestLimits` documented
("Soft limits — measured envelopes — are a separate, per-(build × board)
concern and never live here").

## Context

The firmware-manifest roadmap established hard facts (partition layout, chip
RAM) in `ManifestLimits` and reserved "soft limits" for measured envelopes,
but no code carried one. The 8-wire design target forced the question: on
the classic ESP32 the heap binds well before frame time (~89.5 B/LED of
duplication, two heap regions), so "8 channels" must never be read as 8×
dome strips — and that constraint needs a durable, surfaceable home.

## Decision

- **A soft limit is evidence, not policy.** `HwSoftLimits` on the board
  manifest (`lpc-hardware/src/manifest/hw_soft_limits.rs`) carries
  `HwMeasuredLimit { value, measured }` records, where `measured` is
  free-text provenance: date, firmware, workload, observed margins. A record
  without provenance is a guess and does not belong here.
- **Exceeding one warns and proceeds — never refuses.** The check lives
  where outputs open (`Esp32OutputProvider::open`), logs the value and the
  provenance, and continues; the Studio face renders over-envelope in the
  attention tone as advice. Refusal would invert the convention and punish
  exactly the person probing past the envelope with a scope in hand.
- **The manifest-file field is optional and additive** (serde default,
  schemars-generated schema) — older manifests parse unchanged, older
  firmware ignores the field.
- **The record reaches Studio through the hello**
  (`HardwareFacts::total_led_budget`, stamped by the embedder like the efuse
  identity), because the device's own manifest — not Studio's static board
  catalog — is the truth for a hand-edited `/hardware.json`.

The first record: DOM-Z-102 `totalLeds = 1500`, provenanced to the overlap
plan's G1 soak (29.99 fps, zero trips, 240 s, scope-checked).

## Consequences

- New envelopes are cheap to add (a field on `HwSoftLimits`, a record in a
  board JSON) and self-documenting via provenance.
- A record is falsifiable: a better measurement replaces value AND
  provenance together.
- The honest 8-wire tier reads from the record (8×~200 at today's
  envelope), and Studio's budget bar makes the envelope visible where LED
  counts are edited.
