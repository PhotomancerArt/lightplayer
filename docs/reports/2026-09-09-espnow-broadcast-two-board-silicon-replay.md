# The two-board `espnow-broadcast` silicon capture, and what it says about the air

**2026-09-09.** Desk step 3 of the C6-emulator roadmap's `d1-desk-batch.md`,
run on `5c1d37627b22` (`origin/main` at the merge of PR #636). This note is
the reading of it. The transcripts are the evidence and they are committed
beside the emulated pair, under
`lp-emu/transcripts/esp32c6/espnow-broadcast/`.

## What was captured

Two XIAO ESP32-C6 boards, both running **one** ELF (sha256
`bddf0018abd379afb55a21113b198b34bdbf25877488f890d2068e431e9f6280`), each
recorded alone through its own port while the other was powered and on the
air:

| transcript | board | MAC | device id | its peer's raw events |
|---|---|---|---|---|
| `silicon-esp32c6-2026-09-09-5c1d37627-a0f26287b48c.txt` | the desk board | `a0:f2:62:87:b4:8c` | `0x8cb48762` | 315–320 |
| `silicon-esp32c6-2026-09-09-5c1d37627-a0f26285a87c.txt` | the second board | `a0:f2:62:85:a8:7c` | `0x7ca88562` | 1494–1499 |

Each carries six `espnow-tx` records of its own frames (event 0–5, walking the
four-rung payload ladder 0/8/24/64) and six `espnow-rx` records of the other
board's, then `tx=6 rx=6 peers=1 peer_overflow=false dropped=0` and the
sentinel. The peer's raw event numbers start where that peer's own power-on
left them, which is why the payload's mask set masks them.

This is the silicon half acceptance criterion 4 asked for. M4 P3 shipped the
emulated pair without it, for physical reasons recorded in the desk file.

## The replay, four ways

The emulated pair as PR #636 committed it, replayed against the silicon pair,
machine for machine, at both time grades:

| left (silicon) | right (emulated) | compared | equal | differ |
|---|---|---|---|---|
| desk board | `t1` desk-board twin | 60 | 55 | 5 |
| second board | `t1` second-board twin | 60 | 55 | 5 |
| desk board | `t2` desk-board twin | 60 | 55 | 5 |
| second board | `t2` second-board twin | 60 | 55 | 5 |

All four are the same five differences, and they are all one field:

```
structural field espnow-rx[1].gap differs: 1 vs 2
structural field espnow-rx[2].gap differs: 1 vs 2
structural field espnow-rx[3].gap differs: 1 vs 2
structural field espnow-rx[4].gap differs: 1 vs 2
structural field espnow-rx[5].gap differs: 1 vs 2
```

Everything else is equal: both device ids, every sender's own event number,
every `msg_kind`, every `payload_len` on the tx side, `len_ok` on every rx
record, and the record counts and their kinds. `espnow-rx[0].gap` is 0 on both
sides by construction — there is no previous frame to compare the first one
against.

**No timing was gated, and none was reported as a ratio, because this payload
declares no timing field at all.** The replay's class table has exactly one
row. There is no pin row either, and that is the payload rather than the
configuration: ESP-NOW events are console fields, not pin fields, so no pad is
decoded on either side and silicon records none in any case (#624).

## What the one difference means

`gap` is the receiving board's own arithmetic: the distance between the peer
event number in this frame and the peer event number in the previous one from
the same peer. **1 means nothing was missed.**

- **Silicon: `gap` is 1 on every record, on both boards.** A real C6 heard
  every frame its peer sent, and the peer's `payload_len` walks the whole
  four-rung ladder — 64, 0, 8, 24, 64, 0 on one board and 24, 64, 0, 8, 24, 64
  on the other.
- **The emulator: `gap` is 2 on every record.** The peer's events arrive as
  0, 2, 4, 6, 8, 10 and the ladder is seen at half resolution — 0, 24, 0, 24,
  0, 24.

That settles the open question in
`docs/debt/emu-c6-air-delivers-every-other-frame.md`, which could not be
decided without silicon: **the every-other-frame behaviour is the emulator's,
not the payload's.** The debt file already establishes the air is not losing
the frames (`frames_sent`, `frames_offered` and `air_frames_delivered` agree
and `air_frames_undelivered` is 0) and that it is not a phase artefact of the
lockstep stagger; what was missing was any evidence about what the hardware
does. The hardware misses nothing.

**Nothing was fixed here, deliberately.** `lp-emu/esp/**`, including
`wifi_stub.rs`, was fenced for the sitting that recorded this, and a fix is
queued as its own PR. No number was tuned toward silicon, and no transcript
body was edited.

## What this does not say

- **It does not promote any grade.** The air's trust claim stays `modeled`
  (RD10). Byte-agreement on 55 of 60 fields is evidence to weigh in a
  `because`, not a promotion, and a grade moves only with a transcript.
- **It says nothing about the completion path.** The emulator originates the
  TX completion and the RX interrupt on a path nobody has observed on silicon
  (P0 §3). These two transcripts observe *frames the firmware reported*, which
  is a level above that mechanism; they neither confirm nor refute it.
- **It says nothing about the PHY, occupancy, collisions, RSSI or the C6's
  scan truncation.** Two boards a few centimetres apart on a quiet channel
  with nothing else transmitting is the friendliest medium there is. That the
  frames all arrived is what one should expect, and is not a measurement of
  the air's behaviour under load.
- **The peer's raw event numbers and byte counts are masked** in this payload,
  by the mask set and for the reason it states: a peer's counter starts at its
  own power-on and no capture can align two of those. So the comparison above
  is of `gap` and `len_ok`, which are the receiving board's own conclusions
  about what it heard, and not of the peer's numbering directly.
