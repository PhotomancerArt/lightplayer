# Use cases — the pieces LightPlayer is built for

Captured 2026-08-21, from Yona's framing during the walk-up patching G1
gate. These are not personas; they are three real shapes of piece, and
they pull the product in different directions. When a decision is close,
ask which of these it serves and which it costs.

| Case | Shape | Ports | Wiring policy |
| --- | --- | --- | --- |
| [scarf](scarf.md) | one strip, one piece | 1 | **auto** — it should Just Work |
| [sign](sign.md) | laid-out lamps (letters, shapes) | 1–5 | **manual** — per-lamp placement matters |
| [dome-scale](dome-scale.md) | tens of thousands of lamps, many boxes | dozens | **manual**, re-wired every build |

## Why the list exists: the flow flag

The three cases split on ONE question — **what happens to a fixture's
lamps that no patch entry names?**

- **auto** flows them onto the wire after the last anchor. Nothing is
  ever unmapped for long, which is what makes the scarf effortless and
  what makes WLED's world work.
- **manual** leaves them on no wire at all: dark on the piece, unmapped
  in the editor. "Not mapped = not lit" becomes the progress bar.

Auto-mapping is not a default that fits everyone — it is right for one of
these three cases and actively counter-productive for the other two. So
it is a **flag at the fixture level**, stored in that fixture's
`{stem}.patch.json` as `"flow": "auto" | "manual"` (patch format 3, P5b);
absent field and absent file both mean `auto`, so every document written
before the flag existed reads exactly as it always did.

`lp-core/lpc-mapping/src/patch.rs` is the definition; the Patching
panel shows the flag on the object section and toggles it (undoable),
with **unmap all** beside it once a fixture is manual.

## What is still owed

- **A real-hardware walk of all three cases.** Everything here is
  reasoned from experience with the pieces and verified in the
  simulator; none of the three has been walked end-to-end on the
  hardware since the flow flag landed.
- **Creation-time flow defaults** — should a Strip preset create an
  `auto` fixture and a drawn shape a `manual` one? Decided at that walk,
  deliberately **not** now. A resolve-time heuristic is banned outright:
  it would flip existing, documented fixtures dark.

## See also

- [walk-up patching](../walk-up-patching.md) — the assignment flow these
  cases are patched with.
- [the selection-model ADR](../../adr/2026-08-20-walk-up-assignment-selection-model.md)
  — one selection, armed verbs, and the flag's place in it.
