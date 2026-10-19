# The first-light ratchet

Time to first light — power-on to the first lit LED frame — is the number the
tree-store adoption was for (under one second; CX1 went 613 → 378 ms at the
cutover). This ratchet keeps it from creeping back. Approved by Yona on
2026-10-18 (plan `lp2025/2026-10-16-0848-tree-store-adoption`); it is the one
deliberate exception to "never gate on emulated time"
([the emulator-first ADR's 2026-10-18 amendment](adr/2026-09-10-the-emulator-first-device-walk.md)).

## What is measured

CI's **`C6 first light (x64)`** job (`c6-first-light` in
`.github/workflows/pre-merge.yml`) runs `just first-light-measure`:
`lp-cli/tests/emu_first_light.rs` on **`lp-emu:esp32c6:t4`**, booting the
C6's **split image ROM-up** (mask ROM, our bootloader, the loader, the core,
the engine — `merged.bin`, the bytes a board is flashed with), cut to a
power-on boot twice per case, with a **saved Wi-Fi network** on the emulated
LAN (`LP_FIRST_LIGHT_NET=1`, the network seam), for:

| case | project | |
|---|---|---|
| `basic+net` | `projects/test/basic` | **gated** |
| `c40+net` | a c40-sized project (132 files, ~216 KB) | reported, never gated |

The reading is the firmware's own line, `[perf] first light at <ms> ms`
(`fw-esp32c6`'s `first_light`), the higher of the two boots. On the emulator
the line counts from reset (it models no LP timer); on silicon from power-on.
`t4` read basic 373 ms against CX1's 378 ms on the cutover image (#1177's
accuracy table: `docs/reports/2026-10-17-c6-flash-timing-emulator.md`), and two
boots of one image differ by hundredths of a millisecond, so whole
milliseconds do not flake.

**The record is CI's reading, not a desk's.** The image's bytes depend on the
host that built it (paths; the same reason [positional figures](chip-figures.md#positional-figures)
exist), and `t4` charges for where code lands in flash and cache: at
`6fadbca6c` a desk (M2 Max) build read 394 ms and the CI runner's build
385 ms. CI's runners build identical bytes, so CI agrees with itself; a desk's
`just first-light-check` shows a direction, never a value to record. Bless
with the number CI printed.

It runs where the C6's pinned figures run (the `emu_c6` filter or the C6
firmware's crate closure), on its own files, and on every main push.

## The rules

The record is [`scripts/first-light-record/esp32c6.json`](../scripts/first-light-record/esp32c6.json):
`first_light_ms`, and `recorded_by` — who wrote it.

| the measured value | CI | who clears it |
|---|---|---|
| > 500 ms | **red** | the change, or Yona |
| > 400 ms | a **warning** annotation (yellow) | nobody needs to; it says how close 500 is |
| more than **+5 ms** over the record, the image changed | **red, "needs Yona"**, no patch | **Yona only**: `just bless-first-light <ms>` + the PR label `first-light-blessed` |
| more than +5 ms over the record, the image did **not** change | red, with CI's re-record (`recorded_by: emulator-model`) and a note | anyone: `just apply-ci-figures` |
| up to +5 ms over the record | green | — |
| below the record | red, with CI's patch (`recorded_by: ci-ratchet`) | anyone: `just apply-ci-figures` |

"The image changed" is detect-changes' `c6_image`: the C6 firmware's crate
closure, the split image's other parts (loader, bootloader, boot records,
split pipeline, merged-image recipe), `third_party/`, `Cargo.lock`, the
toolchain, and the measured project. A PR touching none of those — an
emulator change under `lp-emu/`, or a change to the measurement itself —
cannot have moved the product's first light, so a move there is re-recorded
without Yona; the note in the PR comment says to check the emulator change
explains it. A main push counts as "changed".

A drop is red on purpose: the record only ratchets down if someone applies the
patch, and a green job would leave it stale.

## Blessing a rise (Yona's)

A rise of more than 5 ms on a change to the product is Yona's decision, never
an agent's:

```bash
just bless-first-light 392      # CI's reading, from the PR comment / job summary
git commit -am "chore(first-light): Yona blesses 381 → 392 ms (<why>)"
```

That writes the record with `recorded_by: yona`. Then put the label
**`first-light-blessed`** on the PR and re-run the `C6 first light` job (the
job reads the labels live; a re-run replays the old event). CI accepts a
**raised record** on a PR only with **both** — the field and the label — or,
on an image-unchanged PR, with `recorded_by: emulator-model`. Anything else
that raises the record is red: "the record rose … without Yona's bless".

The guards on the other paths:

- CI never hands back a patch for a rise on an image change; the PR's sticky
  figures comment leads with **"Needs Yona: first light rose more than 5 ms
  over its record"** and the bless command.
- `just apply-ci-figures` runs `scripts/ci/first-light-ratchet.py
  guard-apply` after applying: a staged record that rose is refused and every
  record is rolled back, unless CI re-recorded it as `emulator-model`.

These are courtesy locks between cooperating agents, like the desk's board
leases: they make the rule visible and the wrong path awkward, not
impossible. AGENTS.md says it in words.

## The periodic silicon cross-check

The emulator is the gate; silicon is the drift check. Every so often (after an
emulator timing change, a large boot-path change, or a month), at the desk:

```bash
board take c6-expendable --as "<you>" --for "first-light cross-check"
BOARD_HOLDER="<you>" LP_FIRST_LIGHT_WIFI_SSID=… LP_FIRST_LIGHT_WIFI_PASSWORD=… \
  just first-light-cx1
```

It builds this tree's split image, flashes `merged.bin` whole onto **CX1** (`c6-expendable`)
(the sacrificial board: it erases the filesystem), and takes 10 power-on boots
per case (`basic`, and `wifi` = basic with the bench network saved, the
ratchet's `basic+net`) with `lp-cli/tests/board_first_light.rs`, then prints
the record beside them. If silicon's median and the record disagree by more
than ~5 %, file a fidelity defect against the emulator under `docs/defects/`
and fix the model before trusting the gate again.

## Files

- `scripts/ci/first-light-ratchet.py` — `check` (the rules; writes the job's
  `figures-patch-c6-first-light` artifact), `bless`, `guard-apply`.
- `scripts/first-light-record/esp32c6.json` — the record.
- `scripts/ci/figures-patch.py comment` — renders the verdict into the PR's
  one figures comment.
- `just first-light-measure`, `just first-light-check` (measure + rules,
  locally), `just bless-first-light`, `just first-light-cx1`.

## The first record

Recorded on this PR's CI run (merge of `6fadbca6c`-based work, firmware
unchanged from main `6fadbca6c`), `lp-emu:esp32c6:t4`: **385 ms**
(`basic+net`, both boots; parts: boot 14, init 19, fs 6, radio 61, split 5,
server 8, load 86, lit 186 ms); `c40+net` read 399 ms. It was first
committed at the desk's 394 ms, and the ratchet's own drop patch, applied with
`just apply-ci-figures`, brought it to CI's 385 — the path a drop takes,
proven on its first run.
