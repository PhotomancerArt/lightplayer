---
status: fixed
found: 2026-10-09      # ci — Validate Browser (x64) on #1076, run 37951739994, job 113893308952
fixed: this change
area: lp-app/lpa-link/tests/browser_serial_conformance.rs (`a_classic_request_is_one_link_message_and_its_answer_comes_back_packed`, and the C6 test sharing its body)
class: marker-search-over-random-bytes
related: [docs/adr/2026-09-27-lp-link-one-comms-layer.md, lp-base/lp-link/src/sniffer.rs]
---
# A conformance test took a checksum that spelled `M!` for an `M!` line

**Symptom** — `Validate Browser (x64)` failed on #1076, a PR that does not
touch `lpa-link`, in the Web Serial conformance suite:

```
panicked at lp-app/lpa-link/tests/browser_serial_conformance.rs:1277:5:
nothing the host wrote is an M! line: "\0\u{2}\u{3}\u{1}\u{1}\u{5}B���\u{1}\u{1}\u{1}\u{1}\u{1}\u{7}\u{1}\u{4}�_M!\0\0\u{2}\u{3}…"
```

The failing test was `a_classic_request_is_one_link_message_and_its_answer_comes_back_packed`.
`main` passed the same suite at 587bb7169.

**Root cause** — To check that the host no longer wrote the old `M!` line
framing, the test searched everything the host wrote for the two bytes `M!`
(`0x4D 0x21`). Since the USB cut-over those bytes are lp-link frames
(`0x00 COBS-FF 0x00`), and some of them are random on every run. Each SYN
carries the host's session nonce (four bytes from `crypto.getRandomValues`).
Every frame ends in a CRC-32C: a SYN's covers the nonce, and every later
frame's is keyed with both ends' nonces. Any two neighbouring random bytes
spell `M!` once in 65,536.

In the failing run the host's nonce was `0xe7d4bb42` and its first SYN's
checksum was `dd 5f 4d 21`. The `M!` is that checksum's last two bytes, just
before the frame's closing `0x00`.

How that is known: the panic's text is lossy, because the page decoded the
bytes as UTF-8 and every byte ≥ `0x80` became `�`. The frames still read by
hand. The first SYN is header `03 00 00 00`, nonce `42 ? ? ?`, peer nonce 0,
`max_payload` 256 and `rx_window` 4 (the classic's `uart()` preset, so the
classic variant), then checksum `? 5f 4d 21`. Exactly one value of the three
unknown nonce bytes fits both SYNs' checksums (`dd 5f 4d 21` and
`02 88 6c 0d`). That nonce sets every later frame's checksum, and the 212
bytes rebuilt from it reproduce the panic's text character for character.
They are in the test now as `HOST_WROTE_IN_CI`.

The rate: the same exchange run natively on lp-link with random host nonces
put `M!` in the host's ~201 bytes in 65 of 200,000 runs on `usb()` (1 in
3,077) and 72 of 200,000 on `uart()` (1 in 2,778). With both variants in the
suite, that is about one suite run in 1,450. Nothing else in those bytes
changes between runs: the board double's nonce is fixed (`0xB0A2_0001`), and
so are the headers and the JSON.

**Fix** — The test now checks what the assertion was meant to say: every
byte the host wrote belongs to a link frame. The bench keeps everything the
host wrote (`LinkBench::host_wrote`), and `host_proto_messages` checks it in
two steps:

- Split on the `0x00` delimiter. Frame bodies sit between pairs and nothing
  sits anywhere else, so there is no text and no `M!` line.
- Run lp-link's own `LinkSniffer` over it. It decodes every frame and
  verifies its checksum under the session that the host's SYNs name, then
  returns the proto messages.

The test then asserts that the request is one of those messages, byte for
byte. The C6 and classic tests share that body. No other test in the repo
searched binary bytes for `M!`. The other searches read decoded text: `lp-cli
wire unpack` output and emulator UART transcripts.

**Regression coverage** — in `browser_serial_conformance.rs`, none of which
needs the shim:

- `the_frame_check_reads_a_checksum_that_spells_m_bang_as_a_checksum`: the CI
  bytes pass.
- `the_frame_check_catches_an_m_bang_line`: an appended `M!{…}\n` fails.
- `the_frame_check_catches_a_frame_that_does_not_verify`: one changed
  checksum byte fails.

**Lesson** — A substring search proves something about text, not about a
binary stream. Once a stream carries random bytes (nonces, checksums,
compressed or packed payloads), a missing marker proves nothing, and a found
one may be chance. Parse the framing, and assert on what the parser reports.
