#!/usr/bin/env python3
"""Turn a transcribed walk into a deterministic `--uart0-script`.

    scripts/emu/walk-script.py <walk.uart.bin> [-o <walk.script>]

`scripts/emu/upload-walk.sh` runs a real `lp-cli upload` against the
emulator through `uart-tcp-proxy.py`, which writes every host→device chunk
into the transcript wrapped in `<<HOST … >>`. That walk is honest and *not*
deterministic: the host's wall clock decides where each request lands
against the guest's emulated clock, which is the drift the spike report's
§7 and §11.3 spend most of their diff on.

This reads those host records back out and writes the same conversation as
a script the machine replays in guest time alone:

    after "[RECOVERY] boot complete (first frame served)" "M!{…hello…}\\n"
    after "\\"id\\":18446744073709551615" "M!{\\"id\\":1,…}\\n"
    after "\\"id\\":1," "M!{\\"id\\":2,…}\\n"

The needle for request *n* is the device's **answer to request n−1**: every
wire frame carries its request's `id`, and the answer is the only place that
id appears in the device's own output. So the script says what a client
says — "send the next one when the last one is answered" — and says it in a
form whose timing is a function of the guest and nothing else.

The bytes are emitted verbatim (as `\\xNN` where they are not printable
ASCII), so the script is the frames lp-cli sent, not a re-serialisation of
them: nothing here parses or rebuilds a wire message, and the framing rule
(`lpc_wire::json::to_serial_line`, PR #538) stays lp-cli's business.
"""
from __future__ import annotations

import argparse
import os
import re
import sys

OPEN = b"<<HOST "
CLOSE = b" >>"

# The line the first request waits for: the guest has booted, mounted its
# filesystem and served a frame, which is when a real client's readiness
# engine has seen the hello it wants.
FIRST_NEEDLE = "[RECOVERY] boot complete (first frame served)"


def host_chunks(blob: bytes, times: str | None) -> list[bytes]:
    """Every `<<HOST … >>` record's payload, in order.

    The proxy writes a `.times` sidecar beside the transcript — one line per
    record, `<seconds> <dev|host> <offset> <length>` — and that is the
    authoritative framing: a request's bytes can contain the close marker
    themselves (`projects/test/shader-oracle`'s README says `(v + 0x80) >> 8`,
    and the first cut of M5 P4's walk lost the tail of that request to it).
    The marker scan is the fallback for a transcript with no sidecar, and it
    says so when a record it found looks cut short.
    """
    if times is not None:
        out: list[bytes] = []
        for n, line in enumerate(open(times), 1):
            parts = line.split()
            if len(parts) != 4:
                raise SystemExit(f"{times}:{n}: expected `<t> <dir> <offset> <len>`")
            _, direction, offset, length = parts
            if direction != "host":
                continue
            rec = blob[int(offset) : int(offset) + int(length)]
            if not (rec.startswith(OPEN) and rec.endswith(CLOSE)):
                raise SystemExit(
                    f"{times}:{n}: the record at {offset}+{length} is not a "
                    f"<<HOST … >> record — is the sidecar from this transcript?"
                )
            out.append(rec[len(OPEN) : -len(CLOSE)])
        return out
    out = []
    at = 0
    while True:
        start = blob.find(OPEN, at)
        if start < 0:
            return out
        end = blob.find(CLOSE, start)
        if end < 0:
            raise SystemExit(f"unterminated <<HOST record at byte {start}")
        chunk = blob[start + len(OPEN) : end]
        if not chunk.endswith(b"\n"):
            print(
                f"walk-script: the host record at byte {start} does not end in a "
                f"newline — a request containing ` >>` was probably cut at the marker; "
                f"pass the proxy's `.times` sidecar (or keep it beside the transcript)",
                file=sys.stderr,
            )
        out.append(chunk)
        at = end + len(CLOSE)


def frames(chunk: bytes) -> list[bytes]:
    """A host chunk split into whole `M!…\\n` lines.

    A TCP read boundary is not a frame boundary — lp-cli's writes arrive
    coalesced or split — so the script is written per frame, which is the
    unit the guest's reader actually consumes.
    """
    return [line + b"\n" for line in chunk.split(b"\n") if line]


def frame_id(frame: bytes) -> str | None:
    """The `id` field of a wire frame, as text."""
    m = re.match(rb'M!\{"id":(\d+)', frame)
    return m.group(1).decode() if m else None


def escape(data: bytes) -> str:
    out = []
    for b in data:
        c = chr(b)
        if c == "\\":
            out.append("\\\\")
        elif c == '"':
            out.append('\\"')
        elif c == "\n":
            out.append("\\n")
        elif c == "\r":
            out.append("\\r")
        elif c == "\t":
            out.append("\\t")
        elif 0x20 <= b < 0x7F:
            out.append(c)
        else:
            out.append(f"\\x{b:02x}")
    return "".join(out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("transcript")
    ap.add_argument("-o", "--out")
    ap.add_argument(
        "--times",
        help="the proxy's `.times` sidecar (default: <transcript>.times), the "
        "authoritative record framing",
    )
    ap.add_argument(
        "--first-needle",
        default=FIRST_NEEDLE,
        help="what the first request waits for",
    )
    ap.add_argument(
        "--chunk",
        type=int,
        default=64,
        help="bytes per host write (0 = one write per frame)",
    )
    ap.add_argument(
        "--chunk-gap",
        type=int,
        default=2,
        help="milliseconds of emulated time between chunks",
    )
    ap.add_argument(
        "--wait-for",
        action="append",
        default=[],
        metavar="ID=LINE",
        help="wait for LINE before sending request ID, instead of for the "
        "answer to the request before it. Needed when a request's answer "
        "arrives BEFORE the work it started: `loadProject` is acknowledged "
        "and then the device loads and compiles, head-down for tens of "
        "milliseconds, and a host that starts sending on the acknowledgement "
        "fills the 128-byte RX FIFO and loses the rest of its request. The "
        "line to wait for is the last one the load produces. Repeatable.",
    )
    ap.add_argument(
        "--note",
        action="append",
        default=[],
        help="a provenance line for the header (repeatable). The capture path "
        "alone is a scratch directory nobody else has; what a reader needs is "
        "the firmware, the project and the link it went over.",
    )
    args = ap.parse_args()

    blob = open(args.transcript, "rb").read()
    times = args.times or f"{args.transcript}.times"
    if not os.path.exists(times):
        print(f"walk-script: no {times}; framing host records by their markers", file=sys.stderr)
        times = None
    chunks = host_chunks(blob, times)
    if not chunks:
        raise SystemExit(f"{args.transcript} has no <<HOST records")

    lines = [
        "# Generated by scripts/emu/walk-script.py from",
        f"#   {args.transcript}",
        "# Every request waits for the answer to the one before it, and every",
        f"# request is written {args.chunk} bytes at a time {args.chunk_gap} ms apart, so the",
        "# whole walk is a function of guest time and no write outruns the",
        "# 128-byte RX FIFO. Do not hand-edit: re-run the capture and",
        "# regenerate.",
    ]
    lines.extend(f"# {n}" for n in args.note)
    lines.append("")
    overrides: dict[str, str] = {}
    for spec in args.wait_for:
        fid, _, line = spec.partition("=")
        if not line:
            raise SystemExit(f"--wait-for wants ID=LINE, got {spec!r}")
        overrides[fid] = line

    needle = args.first_needle
    count = 0
    for chunk in chunks:
        for frame in frames(chunk):
            fid = frame_id(frame)
            size = args.chunk if args.chunk > 0 else len(frame)
            pieces = [frame[i : i + size] for i in range(0, len(frame), size)]
            wait = overrides.pop(fid, needle) if fid is not None else needle
            lines.append(f'after "{escape(wait.encode())}" "{escape(pieces[0])}"')
            for piece in pieces[1:]:
                lines.append(f'then +{args.chunk_gap}ms "{escape(piece)}"')
            count += 1
            if fid is None:
                print(
                    f"walk-script: frame {count} has no id; the next request will "
                    f"wait on the same line again",
                    file=sys.stderr,
                )
            else:
                # The answer to this request is the only device output that
                # carries its id.
                needle = f'"id":{fid},'
    if overrides:
        raise SystemExit(
            f"--wait-for names request id(s) this capture has no frame for: "
            f"{', '.join(sorted(overrides))}"
        )
    lines.append("")

    text = "\n".join(lines)
    if args.out:
        open(args.out, "w").write(text)
        print(f"walk-script: {count} frames → {args.out}")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
