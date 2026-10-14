# ADR: ESP32-C6 flash budget — diagnostics trades, the WiFi blob, and what the lpfs partition is reserved for

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The reference target is an ESP32-C6 with **4 MB of flash**, partitioned
(`lp-fw/fw-esp32c6/partitions.csv`) as a 3 MB factory app partition
(`0x300000` = 3,145,728 B) plus a 960 KB `lpfs` data partition (`0xF0000`)
that holds user content. The app image is what this ADR budgets.

This is the second time the image has hit the ceiling. In June 2026 a
change-management feature push overshot the partition by ~302 KB; that effort
(archived plan `2026-06-12-bin-size-reduction`) recovered 431 KB via
externally-tagged serde enums and collection hygiene, landing at ~129 KB of
margin. It also recorded measured dead ends: profile tuning is exhausted
(`lto = true`, `opt-level = "z"`, `codegen-units = 1`), and the flash-MMU
`.text_gap` is not reclaimable. Those two still hold.

> ⚠️ **Corrected 2026-08-02.** This paragraph also carried
> "`panic = "abort"` saves only ~2 KB at `opt-level=z` + LTO", and that number
> was wrong — it **measured nothing**. The June-2026 A/B changed the Cargo
> profile's `panic` key, which the target spec silently overrides; only
> `-C panic=abort` in rustflags takes effect
> (`docs/reports/2026-03-13-esp32-unwinding-implementation.md`, Problem 6, which
> predates the mismeasurement and explains it). Measured properly, dropping the
> unwind tier is **796,032 B — 25.3% of this partition**. See
> [2026-08-02-rv32-firmwares-are-abort-tier.md](2026-08-02-rv32-firmwares-are-abort-tier.md).
>
> The lesson generalises and is why this correction is spelled out rather than
> silently edited: a size measurement that toggles a setting something
> downstream overrides produces a null result indistinguishable from a real one,
> and then sits in the record deterring anyone from re-checking. Before trusting
> a "measured, not worth it" entry here, confirm the toggle reached the compiler
> — `cargo rustc -- --print cfg` for panic strategy, section sizes for anything
> that should have changed shape.

That 129 KB margin was consumed over roughly six weeks of ordinary feature
work — `lpc_registry` (~51 KB), the streaming project-read serializer family
(~49 KB), `lpc_mapping` (~22 KB), `lpc_history` (~9 KB), and general growth.
The image crossed the partition line when PR #174 merged, and **the failure
surfaced as a red Deploy Studio Pages run on `main`** (`espflash::image_too_big`,
3,176,928 B into 3,145,728 B) rather than as a pre-merge signal, because
nothing in `pre-merge.yml` builds the device firmware. Two `main` deploys ran
red before PR #179's mapping retirement incidentally freed ~42 KB and brought
the image back under the line — at 3,136,320 B, or **9,408 B of headroom**.
Recovering by luck, one PR from red, with no pre-merge signal, is the state
this ADR responds to.

Measured composition of the over-budget image (2026-07-28, `esp32c6,server` +
default features): `.text` 2,524,484 B + `.rodata` 573,328 B. By attribution:
the WiFi/ESP-NOW C blob ~500 KB, `lpc_model` ~231 KB, `lps_glsl` ~235 KB,
`lpc_engine` ~254 KB, `core` ~140 KB, `lpa_server` ~108 KB, serde/JSON
machinery spread across `lpc_wire`/`serde_core`/`ser_write_json`/`serde_json`
~180 KB.

The constraint that shapes every option below: **the on-device GLSL JIT is the
product** (`AGENTS.md`). Moving the shader frontend to the host, shipping
precompiled bytecode, or feature-gating the compiler are not size levers
available to us at any price.

## Decision

### 1. Device builds trade diagnostics for flash, by named flags

`lp-fw/fw-esp32c6/.cargo/config.toml` carries a documented flag stack. Measured
marginally, each flag added to the one above it, against the 2026-07-28
baseline of 3,136,320 B:

| Flag | Saving | What it costs |
|---|---|---|
| `-Zlocation-detail=none` | 59,488 B | Panic reports lose `file:line` (~292 `.rs` path strings, ~21 KB of `.rodata`, plus the per-site `Location` structs) |
| `-Zfmt-debug=none` | 95,584 B | `{:?}` formats to nothing — thinner panic payloads and debug logs on device |
| build-std `optimize_for_size` | 51,344 B | Size-tuned `core`/`alloc`; no measured render-loop cost (see below) |

Total: **206,416 B** (~202 KiB), with no code change and no feature removed.

`-Zfmt-debug=none` is the one with real day-to-day cost, and it is deliberately
included: 93 KiB is too large to leave on the table given the growth rate
above. **When debugging on device, delete that line first** — it is a one-line
revert that costs that flash back. `location-detail` accepts granular values
(e.g. `file` alone) if full removal proves too painful.

These are nightly `-Z` flags. The firmware already pins a nightly toolchain
(`lp-fw/fw-esp32c6/rust-toolchain.toml`) for `build-std`, so this adds no new
toolchain constraint.

Two further knobs were measured and **rejected because they save nothing**:
lld's `--icf=safe` (0 B — fat LTO at `codegen-units = 1` has already merged
what it would fold) and `ESP_LOG=warn` (0 B). `ESP_LOG` is worth spelling out
because it is a tempting non-fix: the firmware installs its own logger
(`fw_esp32_common::logger`) whose level is a *runtime* `log::max_level()` seeded to
Info and raisable from the client via the wire `SetLogLevel` command, so
`ESP_LOG` never gated our own `log::info!` calls. Compiling them out with the
`log` crate's `release_max_level_*` features would work, but it would break
`SetLogLevel` — a deliberate product capability — and is therefore not on the
table either.

Validated on hardware (ESP32-C6, `a0:f2:62:85:85:d8`): boots, loads and renders
`/projects/Basic` from lpfs, recovery ledger green, 24-25 fps and 163,988 B
free heap — matching a control build of the same commit without the flags
(25-26 fps, identical free heap). `optimize_for_size` on `core`/`alloc` in
particular costs no measurable render throughput.

### 2. Keep the WiFi blob; do not swap to 802.15.4 (for now)

The `radio` feature costs **499,744 B** — measured by building with and
without it. That is by far the largest single line item in the image, and
today we use it only for broadcast ESP-NOW frames.

It is nonetheless kept, for a reason that is not obvious: **the blob is
approximately all-or-nothing.** ESP-NOW requires `wifi` in `esp-radio`
(`esp-now = ["wifi", ...]`), and the linked blob already contains the full
station stack — WPA supplicant, SAE/WPA3, scanning, association state
machines (~196 such symbols are present in the current image). The linker
cannot garbage-collect inside a prebuilt static library. So we are already
paying for nearly everything real WiFi needs.

The alternative was real and was measured: the pinned `esp-radio` 0.18 exposes
a pure-Rust `ieee802154` feature (deps: `byte` + `ieee802154`, no C blob), and
the C6 has a native 802.15.4 radio. Swapping `RadioDriver`'s transport would
net roughly 460 KB, and the swap surface is contained — one 458-line file
(`lp-fw/fw-esp32c6/src/hardware/espnow_radio_driver.rs`), since the
`RadioMessage` framing, dedup ring, and channel model are ours.

We are not taking it, because **WiFi stays on the product table**. 802.15.4
would trade a capability we expect to want for flash we can find elsewhere,
and it carries two costs beyond the code: 802.15.4 and ESP-NOW are not
air-compatible (fleet-wide cutover, no mixed-version operation), and classic
ESP32 / ESP32-S3 boards — which the multi-board work is actively targeting —
have no 802.15.4 radio at all, so a mixed-chip fleet would need ESP-NOW
maintained alongside it anyway.

### 3. Reserve a WiFi budget now, so it is not a surprise later

Because the blob is already linked, actually *using* WiFi costs only what sits
above the driver. Measured (standalone probe: smoltcp 0.12 with Ethernet
medium, IPv4, TCP, UDP, DHCP client and DNS, static buffers, `opt-level="z"`,
fat LTO, `riscv32imac-unknown-none-elf`):

- IP stack: **~38 KB** (`.text` 34.2 KB + ~4 KB rodata/data). With
  `embassy-net` glue, budget **~50–65 KB**.
- Plain-HTTP client: **+10–20 KB**.
- **TLS 1.3 + crypto: +60–120 KB.** This is the decision that dominates —
  LAN-only HTTP skips it entirely.
- **RAM is the tighter axis, not flash.** A station-mode heap wants ~64 KB+
  on top of socket buffers, and `.bss` is already 339,680 B of the C6's
  512 KB SRAM.

Related consequence: **classic A/B OTA is impossible in this partition
scheme** — two 3 MB app slots do not fit in 4 MB of flash. WiFi-delivered
firmware updates would require a streaming/staged design, not an OTA slot
pair. That is a separate decision, not taken here.

### 4. The lpfs partition is a reserved lever, not a routine one

Shrinking `lpfs` to grow the app partition is the emergency lever from the
June plan (M0), and it remains parked. It is explicitly **reserved to be spent
alongside the radio/WiFi decision** — the moment we take on real WiFi (or
otherwise revisit the radio transport), the partition map should be redrawn
once, deliberately, with the then-current numbers. Spending it now to absorb
ordinary feature growth would leave nothing for the change that actually needs
it, and would silently reduce the space users have for content.

> **Amendment, 2026-10-02 — the reserve is spent.** The Wi-Fi roadmap's
> decision D1 redrew the map once, as this decision asked: `factory`
> `0x340000` (3.25 MB), `lpfs` `0xB0000` at `0x350000` (704 KB). Headroom
> 441,776 B (image 2,966,096 B; it was 183,216 B against 3 MB just before).
> Every fielded board's files are carried across by a layout migration; see
> `2026-10-02-c6-repartition-and-layout-migration.md`. There is no further
> lpfs lever: shrinking it again needs a migration of its own.

### 5. Overflow is a pre-merge failure, not a post-merge one

Pre-merge CI builds the firmware image, computes headroom against the
partition size, prints it on every run, and **fails when headroom drops below
64 KB** (`just fw-esp32c6-size-check`). Printing the number unconditionally
matters as much as the gate: it turns size into a visible, trended quantity
rather than a cliff discovered by a deploy job.

## Consequences

- On-device diagnostics are permanently thinner: no panic `file:line`, no
  `{:?}` content, no info-level logs in shipped builds. The revert path is
  documented in `lp-fw/fw-esp32c6/README.md` and is a per-developer local edit,
  not a shipped configuration.
- Together with the serializer sink erasure that accompanies this ADR, the
  image goes from **3,137,280 B to 2,862,048 B** — headroom **8,448 B →
  283,680 B**, 91.0% of the partition. (That pair is measured against this
  change's merge base, which carries the `fw-esp32c6` split; the per-flag
  marginals above were measured one merge earlier, hence the ~1 KB
  difference from their sum.) That is comfortable today
  and **substantially spoken for** by a future WiFi ship with TLS
  (~120–180 KB).
- The 500 KB blob is now an explicitly accepted cost with a stated
  justification, so future size work should not re-litigate it without new
  information (a materially smaller blob upstream, or WiFi leaving the
  roadmap).
- Growth discipline moves to the CI gate. Features that need more than 64 KB
  of headroom must find their own savings or make an explicit budget case.

## Spend ledger (running)

Deliberate spends since this ADR landed, so that future size work finds them
here — at the entry point the size gate's error message names — rather than
re-attributing them from a bloat diff. One line per spend: what it bought,
what (if anything) is clawback-able, and what clawing it back would cost.
Append; do not editorialize old entries.

| Date | Spend | Bought | Clawback lever |
|---|---|---|---|
| 2026-10-10 | **+560 B in the core, +6,588 B in the engine** — a push is one transaction (plan `lp2025/2026-10-08-2339-wire-push-boundary-and-deflate`, M6 of the tree-store device round; `docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md`): the batch verbs and the deflated write on the fs wire (the `FsRequest` deserializer +1,780 B), the server's batch state and its hooks in `tick_and_send` (+1,876 B), the link-reset hook in the USB/UART/radio transports, `LpFs::write_deflated_chunk` and the engine's own copy of `lp_deflate::inflate` (+2,962 B as an outlined symbol; the core keeps the update protocol's). Measured locally, split image, branch point `e68b9bb41` → `798664a74`: core 1,429,296 → 1,429,856 B, engine 1,851,110 → 1,857,698 B, `app.bin` 3,325,952 → 3,334,144 B; **gated headroom 82,202 → 75,614 B**; update headroom 491,520 B both | One-slot pushes on a tree-store board (a cut at any point leaves the old project), deflated writes (39 % fewer wire bytes on the corpus, 0.41x at rest), and a littlefs board that inflates them | **The engine's inflate (~3 KB)** might share the core's copy instead of linking its own — not tried or measured. The rest is the feature |
| 2026-10-09 | **+3,280 B in the core, +3,430 B in the engine** — relay protocol 2, pictures through the cloud (plan `lp2025/2026-10-08-2050-pictures-through-the-cloud`, PR #1066, `docs/adr/2026-10-06-cloud-relay.md` amendment of the same date). Core: the relay client's protocol 2 half (the hello's firmware, the project report and its keyed tags, the picture schedule) and the picture hand-off; engine: the picture sampler and the frame hook's relay half. CI's builds: main `f5039fb93` (run 37905412888) core 1,426,368 B, engine 1,845,046 B, **gated headroom 88,266 B**, 15,424 B to the next core page; the PR's merge commit `2a42c672f` (run 37917190206) core 1,429,648 B, engine 1,848,476 B, **gated headroom 84,836 B**, **12,144 B to the next core page**. Local builds read the same growth (core +3,280 B, engine +3,412 B) | A board's colours and the name of its project on lightplayer.app, with no session open and nobody taking the board's one network slot | **None needed**: under the plan's 4 KB a side, and the page was not crossed. The one lever there was is taken: the board links only its direction's half of the relay codec (`decode_from_hub`/`encode_to_hub`), which saved 3,888 B of core (local). No new log line in the core |
| 2026-10-08 | **−80 B code in the core, +386 B in the engine** — updates through the relay (plan `lp2025/2026-10-06-2249-ota-wifi-updates` PR C, #1044, `docs/adr/2026-10-06-ota-update-protocol.md` amendment of the same date): `LinkTrust::Relayed`, core-only serving a relayed link, the anonymous key refused there. Recorded on the PR, CI's builds: main `0cabfb52d` core 1,424,560 B, engine 1,841,556 B, **gated headroom 91,756 B**, 17,232 B to the next core page; the PR's `8b21192` core 1,424,480 B, engine 1,841,942 B, **gated headroom 91,370 B**, **17,312 B to the next core page** | A board updated from anywhere, through lightplayer.app, with the same rules as the LAN | **None, and nothing to claw back**: the cost is below the noise of a build. The core did not cross its page |
| 2026-10-07 | **about +6,768 B code in the core** — Bluetooth updates (plan `lp2025/2026-10-05-0820-ota-studio-ble-updates` PR-3, #1005, `docs/adr/2026-10-06-ota-update-protocol.md` "Part C" amendment): core-only serves every Bluetooth link (receive window 32), the link mux passes a radio link's channel-3 messages to the core's session with the tier it was granted, core-only keeps the Wi-Fi controller. The figure is the one the relay row below recorded as arriving with #1005's merge; the PR itself recorded the gate: **158,448 B** at `4cad23fdb` (main merged), 4,848 B above the Wi-Fi ADR's then-floor of 153,600 B | A phone (Bluefy) that updates a board with no cable, and a core-only board that still answers on Bluetooth | **None short of the feature**: core-only must serve the radio links an update arrives on |
| 2026-10-07 | **+8,448 B code in the core, −3,408 B in the engine** — updates over Wi-Fi (plan `lp2025/2026-10-06-2249-ota-wifi-updates` PR A, `docs/adr/2026-10-06-ota-update-protocol.md` amendment of the same date): core-only's own key answer for a LAN link (`lpc-update`'s `key_lookup`), core-only's radio and LAN links in `fw-esp32-common` (`core_only_links`, which replaced the chip crate's Bluetooth-only pump), the LAN's update-mode window and socket buffer, the keyed-`L` refusal, the trial core's three-minute deadline. Measured locally on #1019's head (`b44f7ab4a`) and on the branch: core 1,415,728 → 1,424,176 B, engine 1,844,166 → 1,840,758 B, `app.bin` 3,321,856 → 3,317,760 B; **gated headroom 89,146 → 92,554 B**, image headroom 86,016 → 90,112 B, update headroom 491,520 B both. The core stays in its 44th 32 KiB page (engine at `0x178000` both sides), **17,616 B short of the next one** (26,064 B before). The plan's estimate was +2.5–5.5 KB of core | A house board that takes its updates over Wi-Fi with no host on USB or Bluetooth, and a new core whose Wi-Fi fails gives the board back instead of waiting for a cable | **None short of the feature**: every piece is code core-only runs, and core-only must hold everything an update needs. The engine's −3,408 B was not attributed symbol by symbol (most likely shared code the core now reaches, which the split places in the core) |
| 2026-10-07 | **+39,344 B code in the core, +4,340 B in the engine at P8** — the cloud relay's board side (plan `lp2025/2026-10-06-0815-wifi-relay` PR B, #1019, `docs/adr/2026-10-06-cloud-relay.md` "Device side"). By piece, in the core: the relay client **+29,840 B** (the task's future poll alone 10,434 B; `RelayClient::handle` 4,616 B), embassy-net's `dns` feature **+4,304 B**, the shared network slot, challenge, mux and the wire field **+5,200 B** (+4,484 B in the engine). At the PR's last firmware change the core sat **64 B under its 32 KiB page**; the merge of main's #1005 (core-only Bluetooth updates; +6,768 B arrived with it, including the core-only arms this branch needed to match) crossed it (DD209), so the page is main's growth, not this PR's. At `81816d2f4` the core is 6,704 B into its page, 58,832 B short of the next boundary. CI's build of `a303512e4` reads **88,308 B gated headroom** (main before the PR, `175bc506b`: 160,138 B; the plan's A3 asked 128 KB and a relay client ≤ 24 KB, both missed; Yona accepted it 2026-10-07 as "OK but tight"). Update headroom 622,592 → 557,056 B at P8 when the core first crossed a page. Heap, at boot with nothing saved, +2,580 B used (103,000 → 105,580 B) | A board on Wi-Fi that reaches lightplayer.app from anywhere by itself: registered by account key, no secret on the wire, one sealed session shared with the LAN | **None short of the feature.** The relay must live in the core (OTA M8's rule: a core-only board reaches the relay). DNS (+4,304 B) is embassy-net's resolver; a hand-written one for a single name was not tried or measured. A relay-state log line was written and taken out: +688 B of core crossed a page and cost 32 KiB of update headroom at once |
| 2026-10-06 | **+2,112 B code in the core** (1,214,800 → 1,216,912 B; engine −234 B; steady headroom 301,006 → 301,240 B): the boot hashes on the SHA accelerator, with the core read through a scratch cache window (`docs/adr/2026-10-06-ota-update-protocol.md`, amendment of the same date) | The core's hash every boot 1,243 → 157 ms on silicon, and the engine guard 1,387 → 235 ms | Going back to `sha2` alone gets the bytes back and costs about a second on every boot. Not worth it |
| 2026-10-06 | **+52,896 B code in the core, +57,344 B image** — over-the-air updates (plan `lp2025/2026-10-04-0757-ota-update-protocol` Part B, `docs/adr/2026-10-06-ota-update-protocol.md`): `lpc-update`'s board session, the core-side login (`lpc-access`), `sha2`, `lp_deflate`'s inflate, the channel-3 edge and outbox, the board manifest's JSON, the update light, the engine guard. Core 1,161,408 → 1,214,304 B, engine 1,837,462 → 1,829,376 B (−8,086 B: the hook's engine side is the transport's dispatch only); `app.bin` 3,051,520 → 3,108,864 B. **Steady headroom 300,544 B**, update headroom 884,736 B (`esp32c6,server`, measured against main at `1f0354758`; after merging #880: core 1,214,800 B, steady headroom 301,006 B) | A board that never needs USB again: core installs, engine heals, resume after any cut | **None short of the feature.** The core must hold everything an update needs without the engine. `sha2` at `opt-level = 3` would cut its ~1.2 s boot hash by ~30% for +12,208 B (measured, not taken) |
| 2026-09-25 | **+4,160 B** — the BLE knob-jump fix (PR #831, `docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md`): the vendored esp-radio 0.18 fork that copies a chained ACL mbuf whole (`third_party/esp-radio`), the host's HCI connection ledger that closes every link on a host-restart `Reset`, and the advertiser's restart. Headroom after this, the easy-access work (#821/#824) and #834's own +208 B, measured on #834's merged tree: **278,240 B**, image 2,867,488 B — within ~78 KB of the ~200 KB "radio day" line | A remote that survives any single write length, and a BLE host that comes back from a restart instead of going silent until a USB reboot | **None worth taking**: dropping the fork brings the dead-remote bug back. Drop it when upstream esp-radio copies chained mbufs (`third_party/esp-radio/README-LP.md`) |
| 2026-09-24 | **+359,616 B** — the BLE link (plan `ble-remote-control` M4, `docs/adr/2026-09-24-ble-transport.md`): esp-radio's BLE controller blob, bt-hci, trouble-host 0.6, the NUS service, the link mux, and `esp-radio/coex` (+8,544 B of it, measured alone). Image 2,461,792 B (`ble` off) → 2,821,408 B (`ble` on); **headroom 324,320 B** on M4's branch, **334,768 B** at the merged head `9bfac876d` (image 2,810,960 B, after main's own savings merged in; the `ble` delta is unchanged). lpfs untouched | Phone control over BLE with no cable: the wire, unchanged, on untrusted links behind M3's access gate. BLE is inert until the device store enables it, but the bytes are in every image | **None short of dropping the feature** (a non-`ble` build is the whole clawback, and gives back the image above). ⚠️ **The larger cost is RAM, not flash**: linking it moves 36,000 B of static RAM (~21.6 KB of the BLE blob's IRAM-placed link-layer code, the rest statics), which on this chip comes out of the main task's stack — 71,152 → 38,680 B after moving the host state to the heap, below the meteor example's 35.8 KB high-water plus margin. **Ruled 2026-09-24 (Yona): the heap pays, one image** — main heap region 260,000 → 236,000 B, heap total 325,536 → 301,536 B, stack 62,664 B ("a stack overflow crashes; a smaller heap only narrows the compile margin"; `2026-09-02-esp32c6-ram-split.md`, Amendment) |
| 2026-08-02 | **−796,032 B (a CREDIT)** — RV32 unwinding teardown (`docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md`) | Headroom 259,360 → 1,055,392 B. One panic posture across all four chips; the nightly pin decoupled from `unwinding`'s ABI; the esp-hal `text.x` patch retired | **n/a — this is a credit, not a spend.** Re-spending it means re-adopting unwinding, which needs ~41 KB of stack the chip does not have (it has ~34 KB) and which was non-functional on device for its last five weeks. Do not treat this as budget that appeared from nowhere: it is what the WiFi+TLS claim (~120–180 KB, Decision 3) and any C3 port will draw on |
| 2026-08-01 | **+10,208 B** — resolver persistent resolution (PR #243, `docs/adr/2026-07-31-resolver-persistent-resolution.md`) | −54% engine cycles on the 1-fixture oracle; S3 quad-strips 20→25 fps | **Mostly none** — the spend is the feature; reverting costs the perf win back. The only cheap slice is the intern table's reverse-lookup + error-formatting paths (cycle errors would report ids instead of names): unmeasured, likely single-digit KB flash — its real holding is a few KB of *heap*, not flash. Do not spend an afternoon here expecting 10 KB. |

## Amendment (2026-10-04): the split image's headroom

Since `2026-10-04-c6-split-link-firmware-loader-and-boot-records.md` the C6
ships a **split image** (loader, boot records, core and engine inside
`factory`), and `just fw-esp32c6-size-check` builds and gates that image.
One headroom number became four, and each report line says which it is:

- **image headroom** — `factory`'s length minus `app.bin`'s (the old single
  number's successor);
- **steady headroom, core low** — the region (`factory` + `0x8000` to its
  end) minus the page-rounded core minus the engine, with the core at
  `0x18000` as flashed;
- **steady headroom, core high** — the same with the core at the region's
  high end, where an update leaves it;
- **update headroom** — whether a second core of the same size fits beside
  the running one while it is replaced.

The **gate is the smallest of the steady and update headrooms**, against the
same 64 KB floor. The split costs little code but up to two MMU pages
(32 KiB each) of alignment, so the image grows more than the code does.
Measured when it landed (same tree, `fw-esp32c6-size-check unsplit=1`):
`app.bin` 3,036,670 B against `factory` 3,407,872 B — **image headroom
371,202 B**, steady (low and high) 371,202 B, update 1,015,808 B; legacy
overlap 109,058 B before `0x310000`; code delta +8,478 B and image delta
+59,790 B against the monolithic image of the same tree (2,976,880 B).

## Amendment (2026-10-08): the core's growth through the OTA roadmap

The OTA roadmap (`lp2025/2026-10-03-1330-ota-firmware-updates`) is closed.
What it spent in the **core**, the part that must hold everything an update
needs without the engine, is the ledger rows above, in order; this table
collects the recorded figures so the next size question starts in one place.
It does not re-measure anything, and the builds differ (the sum is a guide,
not a build's delta).

| What | Core code | Gated headroom after | Ledger row |
|---|---:|---:|---|
| The split image itself (loader, two records, page alignment) | +8,478 B code, +59,790 B image (against the monolithic image) | 371,202 B | the 2026-10-04 amendment above |
| The update protocol: board session, login (HMAC against the stored keys, `lpc-access`), SHA-256, the deflate decoder (`lp-deflate`, ~3.1 KiB of it), the update light, the engine guard | +52,896 B | 300,544 B | 2026-10-06, Part B |
| Boot hashes on the SHA accelerator | +2,112 B | 301,240 B | 2026-10-06 |
| Bluetooth updates | about +6,768 B | 158,448 B | 2026-10-07, #1005 |
| The relay's board side | +39,344 B | 88,308 B | 2026-10-07, #1019 |
| Updates over Wi-Fi | +8,448 B | 92,554 B | 2026-10-07, PR A |
| Updates through the relay | −80 B | 91,370 B | 2026-10-08, PR C |

The OTA-attributed rows add up to about +109 KB of core. The drops in
headroom between them include work that is not OTA's: the Wi-Fi link
(`2026-10-07-c6-wifi-link.md`) and the relay, which the core carries because
core-only must reach them. Where the core sits now: **17,312 B short of its
next 32 KiB page** (PR C's CI build), which is the number to watch, because
crossing a page costs a whole page of update headroom at once (the relay row's
688-byte log line is the example). The 64 KB floor holds at 91,370 B.

*2026-10-09:* `main` moved the core 1,872 B closer before relay protocol 2
(15,424 B short at `f5039fb93`, CI), and relay protocol 2 spent 3,280 B more
(PR #1066, the ledger's top row): the core is now **12,144 B short of its
next page**, gated headroom 84,836 B (CI's build of the PR's merge commit).

## Alternatives Considered

- **Swap ESP-NOW for raw IEEE 802.15.4** (~460 KB). Rejected for now — see
  Decision 2. This is the largest lever we know of and stays on the shelf,
  paired with the lpfs redraw.
- **Shrink `lpfs` now** to absorb the overshoot. Rejected — reserved (Decision
  4), and it would trade user content space for our lack of discipline.
- **`panic = "abort"`** — **ADOPTED 2026-08-02, worth 796,032 B.** This entry
  previously read "~2 KB, measured June 2026. Rejected — negligible". That
  measurement flipped the Cargo profile only, which the target spec overrides,
  so it measured nothing (see the correction in Context).

  The note appended here on 2026-07-28 — that the recovery path was **broken on
  device**, a caught panic overflowing the main stack and cascading into a
  non-reentrant-lock panic — turned out to be the whole story rather than an
  aside. It was never fixed: PR #187's one-line fix cost 50 KB of heap and was
  declined. So the image carried 778 KiB of unwind tables for a feature that
  converted a contained failure into a bricked boot. Both facts were in this
  file, one paragraph apart, for five weeks.

  See [2026-08-02-rv32-firmwares-are-abort-tier.md](2026-08-02-rv32-firmwares-are-abort-tier.md).
- **lld `--icf=safe`** and **`ESP_LOG=warn`**. Measured at 0 B each; see
  Decision 1.
- **Drop `-C force-frame-pointers`.** Unmeasured, likely tens of KB. Kept:
  on-device backtrace quality is worth more than the flash, especially now
  that panic location strings are gone.
- **`lps-glsl` at `opt-level = "z"`** (currently `"s"`). Parked — modest yield
  against a compile-time-sensitive hot path.
- **Move the GLSL frontend off device** (~235 KB). Not available at any price;
  the on-device compiler is the product (`AGENTS.md`).

## Follow-ups

- Radio transport decision (keep ESP-NOW+WiFi vs. 802.15.4) paired with the
  lpfs/partition redraw — "radio day". **2026-09-24: BLE was taken with lpfs
  untouched** (+359,616 B, headroom 324,320 B on M4's branch and 334,768 B
  at its merged head; see the ledger). Radio day is
  still owed for WiFi/TLS, and it now starts from ~335 KB, not ~686 KB: the
  WiFi+TLS claim (~120–180 KB, Decision 3) still fits beside BLE, but a second
  radio feature of BLE's size would not without the redraw. Re-check RAM
  first, as Decision 3 says — BLE's lesson was that the static-RAM cost, not
  the flash cost, was the one that bit.
- If WiFi ships: decide TLS vs. LAN-only HTTP early, since it is the
  difference between a ~60 KB and a ~180 KB claim on the budget, and
  re-check the RAM budget before the flash budget.
  **2026-10-01:** the ESP-NOW radio now asks for lean Wi-Fi driver buffers.
  That costs 0 B of flash and saves 10,320 B of radio heap
  (`2026-09-02-esp32c6-ram-split.md`, Amendment 2026-10-01). A station that
  joins a network should re-check those counts before it ships.
- Streaming/staged firmware update design, if WiFi delivery is wanted (A/B OTA
  is off the table in 4 MB).
- Revisit `-Zfmt-debug=none` if on-device debugging becomes painful; the
  granular `location-detail` values are the cheaper middle setting.
