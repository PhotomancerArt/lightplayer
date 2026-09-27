#!/usr/bin/env python3
"""Torn-frame forensics over a board->host byte stream.

Reads either a Studio session recording (`?record=` JSONL, the `wire` rx
lines) or a raw capture (`?wire-capture=1` / `lpWireCapture()` .bin), finds
every packed frame (`0x00 kind COBS 0x00`) whose COBS body does not walk to
its end, and says as much as the bytes allow about each loss:

- how many bytes arrived, and how far the COBS walk overruns or falls short;
- which positions a loss of k bytes (k = 1..8) could sit at and still leave a
  valid COBS walk ("feasible offsets"), as offsets from the frame's leading
  `\\n` — the start of the board's write — so `offset % 64` is the position in
  the USB packet (the io_task's gate writes a frame 64 B at a time from its
  first byte) and `offset % 256` the position in the ChunkedWriter chunk;
- which recording chunks the frame spans (recordings only; a chunk is one
  `take_reads` drain, not one Web Serial read) and their times;
- the board text lines just before and after it.

Plain Python, no dependencies. Usage:

    scripts/link/torn-frames.py <recording.jsonl | capture.bin> [--port N] [--json]

`--json` prints one JSON object per torn frame instead of the table.
Investigation tooling (plan lp2025/2026-09-26-1720-reliable-device-link, M1a).
"""

import argparse
import base64
import json
import sys

PACKET = 64
CHUNK = 256
MAX_K = 8


def load(path, port):
    """Return (bytes, chunks) where chunks = [(start_offset, len, t)]."""
    if path.endswith(".jsonl"):
        rows = []
        with open(path) as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                d = json.loads(line)
                if d.get("kind") == "wire" and d.get("dir") == "rx":
                    if port is None or d.get("port") == port:
                        rows.append(d)
        rows.sort(key=lambda d: d["seq"])
        t0 = None
        with open(path) as f:
            first = json.loads(f.readline())
            t0 = first.get("t")
        buf = bytearray()
        chunks = []
        for d in rows:
            b = base64.b64decode(d["b64"])
            chunks.append((len(buf), len(b), d["t"] - (t0 or 0)))
            buf += b
        return bytes(buf), chunks
    with open(path, "rb") as f:
        data = f.read()
    return data, []


def cobs_walk(body):
    """Walk COBS codes. Returns ('ok'|'over'|'short', end_position)."""
    i = 0
    n = len(body)
    while i < n:
        c = body[i]
        if c == 0:
            return "zero", i
        i += c
    return ("ok" if i == n else "over"), i


def feasible_offsets(body, k):
    """Positions p (0..len) where inserting k unknown nonzero bytes makes the
    COBS walk land exactly on the end. Set-based walk: known bytes jump
    deterministically, wildcard bytes may jump 1..255."""
    n = len(body)
    out = []
    for p in range(n + 1):
        m = n + k
        # reachable code positions, scanning forward
        reach = bytearray(m + 1)
        reach[0] = 1
        for i in range(m):
            if not reach[i]:
                continue
            if p <= i < p + k:
                lo, hi = i + 1, min(i + 255, m)
                for j in range(lo, hi + 1):
                    reach[j] = 1
            else:
                src = i if i < p else i - k
                j = i + body[src]
                if j <= m:
                    reach[j] = 1
        if reach[m]:
            out.append(p)
    return out


def ranges(xs):
    if not xs:
        return "none"
    rs = []
    s = prev = xs[0]
    for x in xs[1:]:
        if x == prev + 1:
            prev = x
            continue
        rs.append((s, prev))
        s = prev = x
    rs.append((s, prev))
    return ",".join(f"{a}" if a == b else f"{a}-{b}" for a, b in rs)


def chunk_of(chunks, off):
    lo, hi = 0, len(chunks) - 1
    while lo <= hi:
        mid = (lo + hi) // 2
        s, n, _ = chunks[mid]
        if off < s:
            hi = mid - 1
        elif off >= s + n:
            lo = mid + 1
        else:
            return mid
    return None


def text_lines_near(data, start, end, span=400):
    """Printable text lines within `span` bytes before start / after end."""
    def lines(seg):
        out = []
        for raw in seg.split(b"\n"):
            s = raw.strip(b"\r")
            if len(s) >= 4 and all(32 <= c < 127 for c in s):
                out.append(s.decode())
        return out
    before = lines(data[max(0, start - span):start])[-2:]
    after = lines(data[end:end + span])[:2]
    return before, after


def scan(data):
    """Yield frames: dict(start, end, kind, body_start). start is the 0x00
    offset, end the closing 0x00 offset (or len(data) if unterminated)."""
    i = 0
    n = len(data)
    while i < n:
        if data[i] != 0:
            i += 1
            continue
        j = data.find(b"\x00", i + 1)
        if j == i + 1:
            # 00 00: an empty frame (resync marker lead); skip one byte
            i += 1
            continue
        if j < 0:
            yield dict(start=i, end=n, kind=data[i + 1] if i + 1 < n else None, closed=False)
            return
        body = data[i + 2:j]
        if (
            cobs_walk(body)[0] != "ok"
            and data[j - 1] == 0x0A
            and j + 1 < n
            and data[j + 1] in (ord("L"), ord("P"))
        ):
            # A frame that lost its closing 0x00: this 0x00 opens the next
            # frame (preceded by that frame's own `\n` lead). Report the torn
            # one without that `\n`, and rescan from the next frame's start.
            yield dict(start=i, end=j - 1, kind=data[i + 1], closed=False)
            i = j
            continue
        yield dict(start=i, end=j, kind=data[i + 1], closed=True)
        i = j + 1


def frame_bytes(data, f):
    lead = f["start"] - 1 if f["start"] > 0 and data[f["start"] - 1] == 0x0A else f["start"]
    return lead, data[lead:f["end"] + (1 if f["closed"] else 0)]


def is_whole(data, f):
    return f["closed"] and cobs_walk(data[f["start"] + 2:f["end"]])[0] == "ok"


def locate_by_twin(data, frames, f, lead, reach=15, min_block=48):
    """Estimate where a torn frame lost its bytes by aligning it with the
    most similar whole frame nearby that opens with the same bytes (same
    message kind, same learned-table state). Consecutive replies of one kind
    differ only in values, so long runs of identical bytes line up; the last
    run that lines up with no net shift before the loss marks where the loss
    begins, and a later run with a larger shift marks where the stream
    resumed. Offsets are from the torn frame's leading newline, which is the
    board's write start. Returns None when no twin is found."""
    import difflib
    i = frames.index(f)
    _, raw = frame_bytes(data, f)
    best = None
    for j in range(max(0, i - reach), min(len(frames), i + reach + 1)):
        g = frames[j]
        if j == i or not is_whole(data, g):
            continue
        _, tw = frame_bytes(data, g)
        if tw[:3] != raw[:3] or not (0.9 * len(raw) <= len(tw) <= 3 * len(raw)):
            continue
        sm = difflib.SequenceMatcher(None, raw, tw, autojunk=False)
        r = sm.ratio()
        if best is None or r > best[0]:
            best = (r, j - i, tw, sm)
    if best is None:
        return None
    _, delta, tw, sm = best
    blocks = [b for b in sm.get_matching_blocks() if b.size >= min_block]
    if not blocks:
        return None
    # the loss is where the shift (twin offset - torn offset) jumps by more
    # than a value-length wobble
    shifts = [(b.a, b.a + b.size, b.b - b.a) for b in blocks]
    loss_at = shifts[-1][1]
    resume_at = None
    lost = None
    for (a0, a1, s0), (b0, b1, s1) in zip(shifts, shifts[1:]):
        if s1 - s0 >= 12:
            loss_at = a1
            resume_at = b0 + (s1 - s0)  # in sent coordinates (twin-like)
            lost = s1 - s0
            break
    if resume_at is None:
        lost = len(tw) - (loss_at + shifts[-1][2])
    return dict(twin_delta=delta, twin_len=len(tw), loss_at=loss_at, resume_at=resume_at, lost=lost)


def check_json_lines(data):
    """`M!{json}` lines that do not parse: a torn JSON reply."""
    bad = []
    at = 0
    for raw in data.split(b"\n"):
        if raw.startswith(b"M!"):
            try:
                json.loads(raw[2:].decode())
            except Exception:
                bad.append((at, len(raw) + 1))
        at += len(raw) + 1
    return bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("path")
    ap.add_argument("--port")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--feasible", action="store_true",
                    help="also list COBS-feasible loss offsets for k=1..8 (slow on big frames)")
    a = ap.parse_args()
    data, chunks = load(a.path, a.port)
    frames = list(scan(data))
    good = 0
    torn = []
    kinds = {}
    for f in frames:
        body = data[f["start"] + 2:f["end"]]
        kinds[f["kind"]] = kinds.get(f["kind"], 0) + 1
        if f["kind"] not in (ord("L"), ord("P")):
            continue
        st, at = cobs_walk(body)
        if st == "ok" and f["closed"]:
            good += 1
            continue
        lead = f["start"] - 1 if f["start"] > 0 and data[f["start"] - 1] == 0x0A else f["start"]
        rec = dict(
            start=f["start"],
            write_start=lead,
            arrived=f["end"] + (1 if f["closed"] else 0) - lead,
            body_len=len(body),
            closed=f["closed"],
            walk=st,
            walk_end=at,
            overrun=at - len(body),
        )
        # For an unterminated-looking body, the loss can include the close.
        feas = {}
        if a.feasible:
            for k in range(1, MAX_K + 1):
                offs = feasible_offsets(body, k)
                feas[k] = [o + 2 + (f["start"] - lead) for o in offs]
        rec["feasible"] = feas
        rec["twin"] = locate_by_twin(data, frames, f, lead)
        if chunks:
            c0 = chunk_of(chunks, f["start"])
            c1 = chunk_of(chunks, max(f["start"], f["end"] - 1))
            rec["chunks"] = [
                dict(i=ci, start=chunks[ci][0] - lead, len=chunks[ci][1], t=round(chunks[ci][2], 3))
                for ci in range(c0, (c1 if c1 is not None else c0) + 1)
            ]
            rec["t"] = round(chunks[c0][2], 3)
        before, after = text_lines_near(data, lead, f["end"] + 1)
        rec["text_before"] = before
        rec["text_after"] = after
        torn.append(rec)
    json_lines = sum(1 for raw in data.split(b"\n") if raw.startswith(b"M!"))
    json_bad = check_json_lines(data)
    if a.json:
        for r in torn:
            print(json.dumps(r))
        return
    print(f"{len(data)} bytes, {len(frames)} frames (kinds {dict((chr(k) if k else 'None', v) for k, v in kinds.items())}), {good} whole L/P, {len(torn)} torn; "
          f"{json_lines} M! lines, {len(json_bad)} that do not parse")
    for at, n in json_bad[:20]:
        print(f"   bad M! line at byte {at}, {n} B")
    for idx, r in enumerate(torn):
        print()
        t = f" t=+{r['t']}s" if "t" in r else ""
        print(f"#{idx} at byte {r['write_start']}{t}: {r['arrived']} B arrived (body {r['body_len']}), "
              f"closed={r['closed']}, walk={r['walk']} overrun={r['overrun']}")
        tw = r["twin"]
        if tw:
            print(f"   twin (whole frame {tw['twin_delta']:+d}, {tw['twin_len']} B): arrived intact to offset "
                  f"{tw['loss_at']} (packet {tw['loss_at'] // PACKET} byte {tw['loss_at'] % PACKET}); "
                  + (f"resumes at sent offset ~{tw['resume_at']} (packet {tw['resume_at'] // PACKET} byte "
                     f"{tw['resume_at'] % PACKET}), ~{tw['lost']} B lost"
                     if tw.get("resume_at") is not None else f"nothing after it; ~{tw['lost']} B lost (tail)"))
        for k in range(1, MAX_K + 1):
            offs = r["feasible"].get(k)
            if offs is None:
                continue
            if not offs:
                continue
            mods = sorted(set(o % PACKET for o in offs))
            print(f"   k={k}: {len(offs)} feasible offsets [{ranges(offs)[:160]}]"
                  f"  mod64 {'{' + ranges(mods)[:80] + '}'}")
        for c in r.get("chunks", []):
            print(f"   chunk {c['i']}: frame offset {c['start']} len {c['len']} t=+{c['t']}")
        for s in r["text_before"]:
            print(f"   before: {s[:120]}")
        for s in r["text_after"]:
            print(f"   after:  {s[:120]}")


if __name__ == "__main__":
    main()
