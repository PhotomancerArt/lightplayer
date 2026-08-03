---
status: carried
since: 2026-08-02
logged: 2026-08-02
area: lpa-server/panel_state + serde surface
related:
  - docs/adr/2026-08-02-panel-writers-and-state-persistence.md
  - docs/design/panel.md
  - docs/debt/serde-surface-is-the-flash-lever (see memory note of the same name)
---
# The firmware now carries TWO JSON codecs for `LpValue`, and the second one cost 50 KB

**Shape** — `/.lp/panel.json` is a tiny document: a version, a bool, and
a list of `{ scope, channel, value }`. Persisting it cost **50,512 B of
ESP32-C6 flash** (measured 2026-08-02: headroom 202,640 B at the P9 head
→ 152,128 B with P10). Symbol-level attribution (ELF diff of the two
builds) says almost none of that is the file format and almost all of it
is a *duplicate*:

| B | symbol |
|---|---|
| 10,332 | `LpValue as serde_core::de::Deserialize>::deserialize` @ `serde_json::Deserializer<SliceRead>` — **new** |
| 3,720 | `LpValue as serde_core::ser::Serialize>::serialize` @ `serde_json::Serializer` — **new** |
| ~5,500 | `serde_json` `Deserializer::deserialize_*` plumbing for the new types |
| 3,308 | `Project::new` (restore inlined) |
| 1,868 | `Project::write_panel_state` |
| 1,888 | merged globals (log strings, JSON field names) |
| ~3,000 | knock-on: `NodeDef::clone`, `SlotShape`/`LpType` clones re-instantiated |

The image already contained a hand-rolled streaming LpValue JSON reader
— `lpc_model::slot_codec::read_lp_value<JsonSyntaxSource>`, 13,594 B —
which is how project artifacts are parsed on device. Deriving
`Serialize`/`Deserialize` for a struct *containing* an `LpValue` and
handing it to `lpc_wire::json` (a `serde_json` facade) instantiated a
**second, complete** encode/decode path for the same type against a
different Deserializer/Serializer pair. The monomorphizations do not
share.

This is the general serde-surface tax, but with a specific and cheaper
fix than "write less serde": *use the codec that is already linked*.

**Carrying cost** — 50 KB off the C6's flash headroom on every image, on
our tightest target, permanently. Invisible at review time: a
`#[derive(Deserialize)]` looks free in a diff. Every future `/.lp/`
document that reaches for serde makes the same trade again.

**Workarounds** — to measure any similar change, take a reading before
and after (stash the change between):

```bash
just fw-esp32c6-size-check
```

To attribute a regression to symbols, diff the two ELFs
(`target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6`) — there
is no RISC-V `nm` on this machine, but the ELF32 symbol table is a
20-line Python parse and that is what produced the table above.

**Incident log**

- 2026-08-02 — filed. P10 (panel persistence) measured at −50,512 B on
  the C6. Accepted for now: the size gate passes with 2.3× its required
  margin, and the fix was out of P10's scope.
- 2026-08-03 — **pressure released, duplication unchanged.** Main
  flipped the ESP32 profile to `panic = "abort"` with `opt-level = "z"`
  overrides; the C6 image went 3,003,104 B → 2,169,520 B and headroom
  142,624 B → **976,208 B**. Re-measured on the current image by symbol
  class rather than by diff: the `serde_json` LpValue codec is
  **24,912 B** across 14 symbols, the hand-rolled `slot_codec` beside it
  is 15,970 B, and panel state's own code is only 5,694 B. So the
  *shape* of the waste is confirmed — the expensive thing is the
  duplicate codec, not the feature — while the urgency is much lower.
  Note the earlier −50,512 B figure was a build-to-build diff under the
  old profile and is not comparable to these absolute sums; both are
  recorded rather than reconciled, because the actionable number is the
  24,912 B duplicate.

**Exit criteria** — panel state encodes/decodes `LpValue` through
`slot_codec` (`read_lp_value` / `write_lp_value`), which is already in
the image, and the C6 recovers the bulk of the ~14 KB LpValue portion
plus most of the serde_json plumbing. One wrinkle to design around:
`read_lp_value` is *typed* — it takes the `LpType` it expects — whereas
the persisted value arrives untyped. The channel's declared `Kind` is
available at restore time from the scope's channel listing, so the read
can be typed by the channel rather than by the file; that is the piece
of design work, and it is small. Estimated paydown: half a day.
