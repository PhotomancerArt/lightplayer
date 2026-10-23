---
status: open
found: 2026-10-22      # how: emulator walk (S3 radio M6, the relay cell's S3 arm, lp-emu:esp32s3:t1+net=lan)
area: lp-fw/fw-esp32s3 `wifi` image's main arena (136 KB, docs/adr/2026-10-21-s3-radio-memory.md) × a project load with the relay registered and a session open
class: budget-exhaustion
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (m6-s3-relay.md; M10, PSRAM)
  - docs/adr/2026-10-21-s3-radio-memory.md
  - lp-cli/tests/emu_relay_link.rs (an_emulated_s3_reaches_lightplayer_app_through_the_lans_uplink)
---
# The S3's Wi‑Fi image runs out of heap compiling `projects/test/basic` over the relay

**Symptom** — on an emulated S3 (`fw-esp32s3 --features wifi`, ROM-up,
`lp-emu:esp32s3:t1+net=lan`), joined, the relay registered and Alice's relay
session open, `lp-cli upload projects/test/basic relay:<mac>@<origin>` ends
in `transport error: Connection lost`. The board loads the project and starts
its shader's compile, then resets (`software system reset`, about 11.5 s
emulated). The next boot says why:

```text
[RECOVERY] previous run crashed (panic): at node:/Basic.show/…/shader-compile:glsl:
  memory allocation of 13092 bytes failed
Boot: the last run's load did not finish: Basic stopped the board while it loaded
```

The board's own heap rows on the way in (one run, the same image):

| state | free | largest block |
|---|---:|---:|
| joined, no account key (no relay buffers) | 117,168 B | 73,744 B |
| relay registered, no session | 110,444 B | 73,736 B |
| a relay session open, project load starting | 95,484 B | 73,736 B |
| `Basic` loaded, shader compile starting | 57,096 B | 46,672 B |

The recovery did what it is meant to do: the project that stopped the board
is not loaded again, and the board came back on the relay by itself.

**Cause** — the `wifi` image's main arena is 136 KB (the Wi‑Fi blob takes
the rest of `dram_seg`; S3 radio memory ADR, decision 5), against 168 KB on
the Bluetooth image and more on the C6. With the relay's buffers and a
session's held, what is left does not cover `Basic`'s load and its GLSL
compile. The relay itself is not at fault: registering, routing, the edit
tier, the LAN takeover, busy, the deploy, Cloud relay off and on and the
refused key all pass on the same run (the cell's S3 arm skips only the steps
that need a project running, and says so).

**Fix direction** — the S3's heap is M10's (PSRAM): the plan's ruling that
the Wi‑Fi image's project-size problem (Skirt 620) belongs there covers this
one too. `ble` stays the default and `wifi` stays off by default until then.
When it is fixed, the S3 arm of `emu_relay_link.rs` runs its project steps
too: delete its `project_runs` exception.
