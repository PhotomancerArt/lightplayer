# ADR: JSON Pack — the wire's packed binary encoding

- **Status:** Accepted
- **Date:** 2026-09-24
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`lean-wire` (`docs/adr/2026-09-23-project-reads-send-only-what-changed.md`)
cut a steady choker lens reply from ~11 KB to ~2.4 KB by sending only what
changed. That plan's own R1 ruling ("this is not about binary, its about
leaning the data in general. binary is a later question.") deferred the
other axis: the same bytes, packed tighter. A follow-on spike
(`~/.photomancer/planning/lp2025/2026-09-23-1528-ion-wire-spike/findings.md`,
`findings-direct-tokens.md`) measured that axis on the pre-send-less
recording (2,874 B against 11,095 B) and modelled it on top of send-less
(1,024 B against a modelled 3,748 B). This plan
(`~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/`) builds it
for real, against the wire shapes send-less left in place, under the name
**JSON Pack** — Yona's naming call on 2026-09-23 ("lp-json-pack? or
something like that?").

The goal: the board's wire replies pack into a compact binary form that
decodes back to **byte-identical JSON text**, so nothing downstream of the
decoder — `Deserialize` impls, host tools, transcripts, `schemas/` — has to
change. JSON keeps working on every link that never asks for packed.

## Decision

### Why custom, not Ion

Ion 1.0/1.1 was the starting reference (per ruling R1a, "deviate if it pays,
and document why"). Strict Ion 1.0 measured close: 3,063 B on the pre-send-
less lens reply, against JSON Pack's 2,874 B. The gap is not the headline —
the deviations below are what keep the *device* side simple, at a small,
documented byte cost or win each. Reading the spike's ablation ladder
(`findings.md`, exact medians, pre-send-less → post-send-less, each rung
adding one deviation to the rung above):

| Rung | Deviation from Ion | Lens reply today | After send-less | Why |
|---|---|---:|---:|---|
| R0 | JSON line (baseline) | 11,095 | 3,748 | |
| R1 | strict Ion 1.0, shared table imported per frame, blobs raw, float32 | 3,063 | 1,103 | almost the whole win is here |
| R2 | drop the per-frame BVM + symbol-table header (31 B) | 3,032 | 1,072 | `WIRE_PROTO_VERSION` already names the table; saves 31 B on *every* frame |
| R3 | text-exact decimals instead of float32 | 3,038 | 1,076 | +4 B, but no float parser on the device and the text is exact |
| R4 | end-delimited containers instead of length prefixes | 3,185 | 1,114 | **costs 3.5–5%.** A streaming encoder with length prefixes has to back-patch every container (a memmove per level); this is the one deviation paid for in bytes, not saved |
| R5 | separate key/value tables; 240 one-byte keys | 3,147 | 1,085 | −1–3% |
| R6 | small ints and the 64 commonest values in the tag byte; LEB128 | 2,874 | 1,024 | −6–9%, the biggest deviation win |
| R7 | inline strings to 31 B with length in the tag (Ion: 13) | 2,874 | 1,024 | ±0 here |
| R8 | per-frame back-references | 2,874 | 1,024 | ±0 on this sample; it helps the device's doubled `layout2d` blob |

What we give up: no off-the-shelf Ion reader can read a capture, and there
is no external reference implementation. `lp-cli wire unpack` covers the
readability need this repo actually has. What we get: no per-frame header,
no back-patching, no float parser, and a decoder whose whole job is
"emit the byte-identical JSON text" — engineering simplicity on a `no_std`,
no-`alloc` device, which is worth more here than Ion-reader interop nobody
uses.

Ion 1.1's genuinely useful ideas — delimited containers, inline-text
symbols, LEB-style varints — are already in JSON Pack. Its remaining ideas
(templates/macros) were measured and rejected: after send-less, a 1,024 B
reply is 52% structure and 14% value tags; a template could remove at most
both, landing near 350 B, while a 1 KB-window LZSS reaches 388 B with no
template language, no macro table to version, and no working-draft spec
dependency (Ion 1.1 was still a working draft as of the spike, per its own
docs). Send-less also removes *unchanged* values, which no template can do
regardless.

### Board→host only

Packing applies to the board's replies. Requests, uploads, and every
host→board message stay JSON — the device needs no decoder, only an
encoder. The write direction measured ~26 KB/s in the spike and was not the
bottleneck; encoding host→board (`request 564 → 112 B`, upload chunks
−12%) is deferred, tracked as future work below.

### No LZ layer

A small-window LZSS on top of JSON Pack reaches 388 B (from a modelled
1,024 B post-send-less reply) for an estimated ~3.5 KB of encoder RAM (1 KB
history + 2 KB hash chain + 512 B heads) and a second stateful, loss-
sensitive layer — not built, not measured on the device. At JSON Pack's
current size, Studio's 150 ms pull loop is the rate cap, not the link
(see "Studio wasm delta" placeholder below and the cadence probe); LZ buys
at most ~1 more read/s. Deferred until a standing-subscription model
removes that pull pause and the link itself becomes the constraint again.

### The per-connection opt-in

Packing is negotiated **per link**, not built into the wire form
unconditionally:

- A host asks with `ClientRequest::SetEncoding { encoding, dictionary }`,
  itself always sent as JSON. The board answers `ServerMsgBody::SetEncoding
  { encoding }` with the encoding now in effect, and only switches its
  write path after that answer goes out.
- **Why the host asks, not a build-time switch.** A plain serial monitor
  (`espflash monitor`, `screen`, `scripts/emu/tty-capture.py`) and any old
  script that greps `M!` lines must keep working. If the board packed
  unconditionally whenever built with the feature, every one of those tools
  would see binary. The opt-in is a per-link *mode*, not a compatibility
  shim for an old wire form — JSON stays a first-class, live encoding on
  every link that never asks, which is why this is not the "capability
  fallback" AGENTS.md's wire-compatibility rule forbids.
- **How the board and host stay in agreement (Yona's G6 concern: "how do we
  know firmware and Studio agree? Studio version may very well not be the
  same as the firmware.").** Two layers:
  1. **Build time.** Studio already refuses any board whose hello `proto`
     differs from its own `WIRE_PROTO_VERSION`
     (`lpa-link/src/device_session/device_readiness.rs::gate_frame`). The
     hello itself always goes out as JSON, before any opt-in is possible, so
     that check runs before a packed frame ever could. `check-lint`'s
     `wire-dict-check` (`just wire-dict`) fails when the generated
     dictionary changed without a `WIRE_PROTO_VERSION` bump, so the same
     proto number always means the same dictionary.
  2. **Runtime, belt and braces.** `ServerHello.pack_dictionary` carries the
     board's dictionary fingerprint (`0` means "cannot pack"). The opt-in
     request carries the host's fingerprint. The board packs **only when the
     two match**; otherwise it answers `json` and stays JSON. A hand-built
     image, a forgotten bump, or a dev build can then never produce a frame
     the host would misread — the fingerprint is visible in the device card
     and in logs, not just enforced silently.
- **Reset to JSON on de-enumerate or a drain stall.** A board forgets the
  opt-in — reverts to JSON with no announcement — on a USB de-enumerate, on
  reboot, and when the host stops draining the link for a while (the same
  signal a board uses to detect a closed port on USB-Serial-JTAG, so a
  merely slow host trips it too). Every host reader accepts both forms
  unconditionally (`lpc_wire::WireStream`), so a silent fallback never
  breaks decoding; what needs to notice is whether to *re-ask*.
- **The host re-asks at most once per 3 s** (`PACK_REASK_INTERVAL_MS`,
  `lpc-wire/src/pack_opt_in.rs`). `PackOptIn` watches every message the
  board sends: it asks as soon as a hello reports a matching dictionary, and
  re-asks when a JSON message arrives on a link that was agreed packed — a
  fallback, not a refusal. A board that explicitly answers `json` is not
  asked again until the link resets. This bounds the opt-in to one extra
  request per interval even on a link that keeps dropping the mode, never a
  loop.

### Blobs from the type

Base64 fields (`serde_base64`) are wrapped in a marker newtype
(`newtype_struct("$lp::blob", …)`, the same trick as `serde_json::RawValue`)
rather than identified from a hand-maintained key list. Every JSON
serializer is transparent to newtypes, and the base64 text is streamed with
`collect_str`, so `serde_json` and `ser-write-json` both print exactly
today's text with no heap `String` on either path. The vendored
`ser-write-json` recognizes the marker name and offers the packed sink a
blob token first, so JSON Pack knows a field is a blob **from the type**,
not from a harvest of a traffic sample. The first spike's blob-key list
missed a real field this way (`display_layout…layout2d.c`, found only by
running a real device); the marker makes that class of miss structurally
impossible.

### The generated dictionary, versioned by `WIRE_PROTO_VERSION`

`schemas/` does not describe the wire — it covers persisted artifacts, and
only the ~86 `lpc-wire` types that derive `JsonSchema`. The dictionary
(keys and value strings, ranked by a committed traffic sample) is generated
by a host-only tracer that walks the wire types through their `Deserialize`
impls, never linked on the device (`cargo tree` on `fw-esp32c6` confirms
this — D7's "built tiny, host tools stay host-only" rule).

```bash
just wire-dict          # regenerate lp-core/lpc-wire/src/wire_dictionary.rs
just wire-dict-check    # fails on drift, and on a dictionary change with no WIRE_PROTO_VERSION bump
```

`wire-dict-check` runs in `check-lint`, so CI enforces both failure modes:
a stale generated file, and a dictionary that moved without the version
bump that makes the runtime fingerprint check in the previous section mean
anything.

### Framing beside console text

`\n 0x00 'P' COBS(payload) 0x00`, written into the same buffer a board
already writes `M!{json}\n` lines and log text into. Console log lines never
contain `0x00`, and COBS-encoded output never contains `0x00` either, so a
text-mode reader (a serial monitor, a capture tool) that meets `0x00` knows a
frame starts, and the next `0x00` ends it; a torn write resyncs at the next
`0x00`. `'P'` (Pack) is a frame-kind byte, leaving room for other kinds
later. Overhead is 3 B plus 1 per 254 B of payload. COBS runs in place in
the existing `FRAME_BUF`: the packed payload is written at a small headroom
offset and COBS is encoded forward from byte 0, so no second buffer is
needed.

### The token path, and G-F3's measured trade

The first spike's encoder re-lexed the serializer's own JSON text
byte-by-byte — correct, but it re-did work the serializer had already done
once. The follow-up (`findings-direct-tokens.md`) added a `SerWrite::token`
hook: the vendored `ser-write-json` offers a `Token` (key, string, number,
bool, null, container open/close, blob, …) at every output site, and a
packed sink consumes it directly; a JSON sink declines and writes its usual
text. **One serializer instantiation still serves both**, chosen per sink at
runtime, so JSON and packed transports share one monomorphization. Building
on the token path was the plan's call (G-F1); the text lexer is kept only
for `RawValue` slot-sync text (which has no tokens to offer) and host-side
capture tools.

The token path is what makes a packed reply cost about what today's JSON
reply costs, not roughly double it (the lexing approach's cost). Dropping
the packed sink's frame-size measure pass (G-F2: fail cleanly on `Full`,
fall back to JSON for that one frame) removes the other half of the
overhead the first spike carried.

Making the serializer's per-field write helpers a hook site costs the JSON
path something, independent of whether packing is ever used: those helpers
are inlined into every wire type's `Serialize` impl today, so anything
added at an output site is paid once per field of *every* wire type — the
monomorphization lever AGENTS.md already names, in a new place. P2 measured
the two independent levers (inlining vs out-of-line) apart, on the final
branch, `lp-emu:esp32c6:t2`, lp-emu commit `83b02b920`, over a steady
~11.8 KB choker reply:

| Build | Flash | Instructions |
|---|---:|---:|
| baseline (no hook) | 2,414,432 B | ~496,000 |
| hook, helpers `#[inline(always)]` | 2,463,904 B (+49,472) | ~531,000 (+7%) |
| hook, helpers `#[inline(never)]` | 2,409,744 B (**−4,688**) | ~628,000 (+27%) |

Applied per the accepted G-F3 ruling: **out of line** — smaller image,
more JSON-path instructions, taken because once Studio opts in, JSON
becomes the fallback and the non-Studio path, where the extra instructions
per field matter less than they did when JSON was the only path. The final
C6 image after this change was 2,410,432 B (−4,000 B against the pre-hook
baseline; the earlier `−4,688`/`−6,336`-shaped numbers reported across the
spike and P2 differ slightly by exact build and feature set — see
`docs/adr/2026-07-28-esp32c6-flash-budget.md` for how flash numbers are
tracked).

### Long-term version skew (known limit)

The runtime dictionary check and the build-time proto check both assume the
two ends can always be brought back to the same build — true for USB-
attached, reflashable boards today, and explicitly not a durable answer once
devices are fielded and can't be upgraded in lockstep (AGENTS.md, "Wire/
protocol compatibility": "this policy will be revisited once devices are
fielded"). Yona named this directly at G0: "we're going to have issues long
term with the forced-same wire version," and separately, for BLE: "that's OK
for Serial. it's less OK for bluetooth, since you can't just flash the
firmware. but it's not part of this work, really." This ADR's mechanism
(hello proto match + runtime dictionary fingerprint) is correct and
sufficient for the reflash-in-lockstep world this plan ships into; a
compatibility window, multiple co-resident dictionaries, or an OTA path is
future work, out of scope here, and connects to the BLE remote-control
vision's own version-skew questions.

**The intended next step is an in-band dictionary** (Yona, 2026-09-24:
follow-up, not before merge). When the host's fingerprint does not match,
the board would send its dictionary once per connection, and the host
would cache it by fingerprint. Build-to-build dictionary agreement then
disappears, and only message-shape agreement is left for the version-skew
work to solve. This protocol already leaves room for it: `SetEncoding`
carries the host's fingerprint, `ServerHello.pack_dictionary` names the
board's, and a mismatch is answered in JSON, so the follow-up adds a reply
rather than replacing the handshake. The decoder already accepts a
dictionary built at runtime (`OwnedDictionary`, behind `alloc`). A
per-connection learned table (HPACK-style) is the larger alternative. It is
not ruled out, but the torn frames seen on real hardware make its resync
cost a design question of its own. The options are recorded in the plan's
notes and were sent to the wire-version-skew vision.

### Measurements

All emulated numbers are on the tree at `3ba73b8f9` (lp-emu last changed at
`2cc482a1e`), on the emulator's clock, not silicon. Bytes are exact; instruction
counts are not time.

**Bytes** (`lp-emu:esp32c6:t1`, shipped image, PLAYFUL Choker, Studio headless
on `emu serve` with the tap on, 150 ms lens pause, packed vs `?wire=json`):

| class | JSON median B | packed median B | packed / JSON |
|---|---:|---:|---:|
| lens reply, steady | 2,401 | 712 | 29.7 % |
| lens first/full read (2 frames) | 34,076 | 13,298 | 39.0 % |
| device-card read | 594 | 306 | 51.5 % |
| heartbeat | 634 | 168 | 26.5 % |
| upload chunk ack | 146 | 71 | 48.6 % |
| upload chunk, host→board | 4,696 | 4,696 | JSON by design |

The whole board→host session: 472,996 → 161,430 B (34.1 %).

**CPU** (`lp-emu:esp32c6:t2`): a steady lens reply is 239,990 instructions as
JSON and 181,869 packed (−24.2 %). The first full read costs 11.5 % more
packed, all of it slot text through the lexer (the G-F4 lever).

**Flash** (against `8fe93d9db`): C6 +18,896 B (headroom 685,792 → 666,896),
S3 +13,904, v3 +18,256. The `json-pack` feature alone is +20,160 on the C6:
dictionary tables 9,423, encoder 3,594, token adapter 3,040, ryu f32 2,142,
lexer 1,816, transport 823, one JSON serializer copy dropped −1,294. It is
over the plan's +14 KB line because the generated dictionary carries every
wire name (4.0 KB more than the spike's harvested one) and the out-of-line
serializer saved 4.0 KB here against the spike's 6.3 KB. Measured lever:
trimming the dictionary to the names seen in traffic takes it to 3,667 B
(−5,756) with no change on the sample; it wants a broader sample first,
because unseen names would then travel inline.

**Studio wasm** (release): +40,448 B raw, +20,981 B gzip.

**Cadence** (lens reads per host second, emulator at ~0.48× real time):
150 ms 2.33 JSON / 2.34 packed; 75 ms 2.99 / 3.05; 33 ms 3.47 / 3.56. On the
emulator the read's service time on the board dominates, so packing barely
moves the rate; on silicon the link time is what packing removes (~27 ms →
~8 ms per steady reply at ~90 KB/s), which is why the pause is decided on a
real C6.

## Consequences

- Every board→host message class (lens reply, device-card read, knob turn,
  heartbeat, upload chunk) is smaller on a packed link once a host opts in;
  the per-class byte table, instruction counts, flash per chip, the
  Studio wasm delta and the cadence probe are under Measurements above.
- `WIRE_PROTO_VERSION` bumps once for this plan; the wire-bump fallout
  (manifests, emulator hello pins, the reference-client walk test,
  transcripts recaptured only where bytes moved, `schemas/` if a wire
  schema moved) is handled in the same change per the checklist learned in
  PR #785.
- A serial monitor, `fw-emu`, and any script that never sends the opt-in see
  exactly today's `M!{json}` text, including the boot hello — AC4 of the
  plan.
- `lp-json-pack` (`lp-base/lp-json-pack`) is a generic, dictionary-injected
  codec with no wire vocabulary of its own; `lpc-wire` owns the wire's
  dictionary, the `WireEncoding` choice, the token↔event adapter, and the
  frame scanner that knows `M!`. Nothing under `lp-emu/`'s MIT fence
  depends on it.
- Every host reader — Studio in the browser (Web Serial, the `?emu=` shim,
  the emulator tab), lp-cli's serial transport, the emulator door and its
  wire tap, and the bench text tools — reads packed frames through the same
  `WireStream`/`WireUnpacker` path, so `lp-cli wire unpack` is the one
  place a capture ever gets turned back into `M!{json}` lines by hand.

## Alternatives Considered

- **LZ compression layered on JSON Pack.** Measured (LZSS, a 1 KB window,
  reaches 388 B from 1,024 B modelled post-send-less), not built. Rejected
  for now: ~3.5 KB of estimated encoder RAM, a second stateful and loss-
  sensitive layer, for a saving the pull loop currently can't use (≤ 1 extra
  read/s at Studio's cadence). Revisit if a standing-subscription live view
  removes the 150 ms pull pause and the link becomes the constraint again.
- **Host→board packing.** Deferred (G4): the write direction is not the
  bottleneck today, and it would require a decoder on the device, whose
  flash cost is unmeasured. Tracked as future work.
- **Structural slot values instead of `RawValue` text** (G-F4), which would
  let the device drop its text lexer (~6 KB estimated). Deferred until
  flash pressure makes it worth a wire-type change outside this plan's
  scope.
- **Keeping the lexing (re-read-the-JSON) encoder as the only path.**
  Rejected once the token hook (F1) showed it cut encoder work by ~70% for
  one small, mechanical serializer change; the lexer is kept only where
  there are no tokens to offer (`RawValue`) or on the host, where re-lexing
  a capture is the whole point.
- **A build-time-only encoding switch (no per-link opt-in).** Rejected: it
  would make every plain serial monitor and old script see binary
  unconditionally, unlike the current policy where JSON remains available
  and default on every link until a host explicitly asks otherwise.

## Follow-ups

- Long-term wire version skew, once boards can no longer be reflashed on the
  spot (BLE-only, fielded devices) — needs its own vision/plan; connects to
  AGENTS.md's "revisit once devices are fielded" and to the BLE remote-
  control vision.
- Host→board packing (G4) and the LZ layer (G5), if a future measurement
  shows the link itself, not the pull loop, is the constraint again.
- Structural slot values (G-F4), if flash gets tight enough to be worth
  dropping the device's text lexer.
