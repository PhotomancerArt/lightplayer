# ADR: The Primary voice is the spectrum outline; destructive confirms arm as "Confirm ⟨verb⟩" on a marked card

- **Status:** Accepted
- **Date:** 2026-08-31
- **Deciders:** Photomancer
- **Supersedes:** None (amends
  [2026-08-30-studio-design-language-aurora.md](2026-08-30-studio-design-language-aurora.md)
  D1 in scope)
- **Superseded by:** None

## Context

Device-model round 1's G1 sitting ruled two treatments "functionally good,
needs design work": the add-a-device card's CTA and the inline two-click
armed confirmation. A three-round spike
(`spikes/devices-page-treatments/index.html`, PR #482 — the design record)
explored both against the accent reckoning's D1 ("no hue on resting
chrome; the spectrum answers interaction").

Two spike findings drove the outcome:

- The gradient Primary fill — spectrum background with near-black text —
  was ruled against on pixels: "rainbow-bg and black text just didn't
  work." The neutral Outline chip beat it at G1, but the page's one
  standing action then had no voice louder than its neighbors.
- The shipped armed treatment (instant red tint + "⟨verb⟩?") failed the
  "asks *are you sure?*" test. Prior art (GitLab Pajamas' two-step,
  Trello's named-object question, Linear's hold-to-delete, Gmail's
  act-then-undo) converges on the strong forms putting the **object at
  stake** into the question rather than recoloring the trigger.

## Decision

1. **The Primary tier wears a standing spectrum OUTLINE** (`ux-spectrum-cta`,
   style.css): a 1px masked-conic rainbow ring at 62% opacity, transparent
   fill, strong white text; hover brings the ring to 100%, spins it (3.2s),
   and adds a wider soft halo to the standard bloom. The gradient fill
   (`ux-primary-gradient`) is deleted. The class owns its own `::before`
   and is never composed with `ux-ir-ring`.
2. **Scoped D1 amendment**: the standing ring marks THE Primary action —
   exactly the slot the gradient fill held. Everything else stands: the
   outline family (`outline_action_class`), quiet chips, and menu rows
   stay neutral, so there is **one standing rainbow per surface**; status
   tones still never wear the spectrum.
3. **The armed confirmation reads as 2K+**: arming turns "Forget" into
   "**Confirm Forget**" (a "Confirm " prefix always in the DOM, its grid
   column animating 0fr→1fr — a content swap cannot drive a width
   transition), red ramps in over 160ms with a small knock, and the
   stand-down window shows as a **quiet drain**: border-tone, 1.5px, 55%
   opacity ("quieter animation" was an explicit gate ruling). The owning
   card marks itself via `.ux-armed-scope:has(.ux-armed)` — body dimmed
   and desaturated behind a red inset ring, footer at full contrast — so
   the consequence previews itself and **no armed state reaches Rust**.
   The two-click machine (blur/4s stand-down, generation counter, native
   dialog for non-inline confirmations) is unchanged.
4. The add-device card's invitation is **transport-open** ("Connect a
   LightPlayer board to control it.") with the transport on the verb
   ("It's plugged in", USB icon) — a future network path joins as a
   sibling verb, not a mode switch.

## Consequences

- Every Primary surface (action strip, share Copy, visitor Fork,
  AddDeviceCard) changed appearance at once; the geometry-parity and
  ring-placement tests in `action_button.rs` pin the composition.
- `--ux-armed-win` (CSS) and `ARMED_CONFIRM_WINDOW_MS` (Rust) must stay
  equal; cross-referencing comments pin them, no shared constant exists.
- The `:has()` marking assumes Chromium-class CSS, which the app already
  requires (Web Serial).
- Patch-panel verb arming is a different system (deliberately neutral
  language) and is intentionally untouched.

## Alternatives Considered

- **Keep the neutral Outline as the CTA** — lost at the gate: the page's
  standing action needs a voice; rejected variants (tile-as-button,
  standing rainbow-glow wash, raised fill, breathing luminance) are
  recorded in the spike's rejected strips.
- **Named-object armed label** ("Forget Porch sign?") — strongest per
  prior art but variable-width per device name; the verb-led instruction
  won ("can we try Forget / Confirm Forget").
- **Footer question bar with explicit Keep** — rejected at the gate
  ("janked out, got flickery": the row swap is a layout jump).
- **Hold-to-confirm** — fights the app's click-verb grammar; kept in the
  spike as calibration only.

## Follow-ups

- Story baselines re-capture across every Primary surface (CI
  merge-is-acceptance flow).
- If a second standing-rainbow candidate ever appears on a surface, this
  ADR's "one standing rainbow per surface" line is the tiebreaker.
