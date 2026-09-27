#!/usr/bin/env python3
"""Map every loss inside a soak capture's packed frames to exact byte offsets.

The soak's frames are reproducible: the text of frame `seq` is a pure function
of (seed, seq, min, max) (fw_esp32_common::soak_link::soak_text, ported
below and checked against every whole frame), and a packed soak frame is
`\\n 00 'L' COBS(prefix + text) 00` where the prefix (the learned header and
the Log envelope's codes) is what the frame's own first bytes say. So for a
torn frame the expected bytes can be rebuilt exactly and aligned with what
arrived: where the loss starts, how many bytes, where it resumes, and where
those fall in the board's writes (64 B USB packets from the write's first
byte, 256 B ChunkedWriter chunks).

    scripts/link/soak-loss-map.py <capture.bin> [--seed 1 --min 16 --max 16384] [--meta page.meta.json]

Investigation tooling (plan lp2025/2026-09-26-1720-reliable-device-link, M1b).
"""
import argparse, json, sys, collections

ALPHABET = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
M32 = 0xFFFFFFFF

def crc32(b):
    c = 0xFFFFFFFF
    for x in b:
        c ^= x
        for _ in range(8):
            c = (c >> 1) ^ (0xEDB88320 & (-(c & 1) & M32))
    return (~c) & M32

class XS:
    def __init__(s, seed): s.x = seed if seed else 0x12345678
    def next(s):
        x = s.x
        x ^= (x << 13) & M32; x ^= x >> 17; x ^= (x << 5) & M32
        s.x = x; return x

def log_uniform(r, lo, hi):
    if lo >= hi: return lo
    lb = 31 - (32 - lo.bit_length()); lb = lo.bit_length() - 1
    hb = hi.bit_length() - 1
    bit = lb + r.next() % (hb - lb + 1)
    a = max(1 << bit, lo); b = min((1 << (bit + 1)) - 1, hi)
    return a + r.next() % (b - a + 1)

def soak_text(seed, seq, mn, mx):
    mn = min(max(mn, 48), 16000); mx = min(max(mx, mn), 16000)
    r = XS(seed ^ ((seq * 0x9E3779B9) & M32))
    n = log_uniform(r, mn, mx)
    head = f"SOAK s={seq} n={n} c=".encode()
    pad = bytes(ALPHABET[r.next() >> 26] for _ in range(max(0, n - (len(head) + 11))))
    return head + f"{crc32(pad):08x} p=".encode() + pad

def cobs_encode(data):
    out = bytearray(); i = 0
    block = bytearray()
    for b in data:
        if b == 0:
            out.append(len(block) + 1); out += block; block = bytearray()
        else:
            block.append(b)
            if len(block) == 254:
                out.append(255); out += block; block = bytearray()
    out.append(len(block) + 1); out += block
    return bytes(out)

def cobs_decode_prefix(body):
    """Decode as far as the codes stay inside the body."""
    out = bytearray(); i = 0
    while i < len(body):
        c = body[i]
        if c == 0: break
        seg = body[i + 1:i + c]
        out += seg
        if i + c > len(body): break
        if c < 255 and i + c < len(body): out.append(0)
        i += c
    return bytes(out)

def frames(data):
    i = 0; n = len(data)
    while i < n:
        if data[i] != 0: i += 1; continue
        j = data.find(b"\x00", i + 1)
        if j < 0: yield (i, n, False); return
        if j == i + 1: i += 1; continue
        yield (i, j, True); i = j + 1

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("capture"); ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--min", type=int, default=16); ap.add_argument("--max", type=int, default=16384)
    ap.add_argument("--json", action="store_true")
    a = ap.parse_args()
    data = open(a.capture, "rb").read()
    checked = 0; losses = []
    # The envelope's closing codes after the text: read off the first frame
    # that decodes to the whole expected text.
    suffix = None
    for (s, e, closed) in frames(data):
        if not closed or data[s + 1] != ord("L"): continue
        pre = cobs_decode_prefix(data[s + 2:e])
        k = pre.find(b"SOAK s=")
        if k < 0: continue
        try: seq = int(pre[k + 7:pre.index(b" ", k + 7)])
        except ValueError: continue
        t = soak_text(a.seed, seq, a.min, a.max)
        if pre[k:k + len(t)] == t:
            suffix = pre[k + len(t):]; break
    if suffix is None:
        sys.exit("no whole soak frame to read the envelope from")
    for (s, e, closed) in frames(data):
        if s + 1 >= len(data) or data[s + 1] != ord("L"): continue
        body = data[s + 2:e]
        pre = cobs_decode_prefix(body)
        k = pre.find(b"SOAK s=")
        if k < 0: continue
        try:
            seq = int(pre[k + 7:pre.index(b" ", k + 7)])
        except ValueError:
            continue
        text = soak_text(a.seed, seq, a.min, a.max)
        expected_payload = pre[:k] + text + suffix
        expected = b"\n\x00L" + cobs_encode(expected_payload) + b"\x00"
        got = data[s - 1:e + 1] if closed else data[s - 1:e]
        if got == expected:
            checked += 1; continue
        # loss start: first mismatch; resume: longest common suffix
        p = 0
        while p < min(len(got), len(expected)) and got[p] == expected[p]: p += 1
        q = 0
        while q < min(len(got), len(expected)) - p and got[-1 - q] == expected[-1 - q]: q += 1
        lost = len(expected) - len(got)
        resume = len(expected) - q
        losses.append(dict(at=s - 1, seq=seq, frame_len=len(expected), got=len(got), lost=lost,
                           loss_start=p, resume=resume, start_mod64=p % 64, resume_mod64=resume % 64,
                           start_mod256=p % 256, gap=resume - p, closed=closed))
    if a.json:
        for l in losses: print(json.dumps(l))
        return
    print(f"{checked} soak frames byte-exact against their rebuilt bytes; {len(losses)} with a loss")
    for l in losses:
        print(f"  seq {l['seq']:5d} @{l['at']:9d}: frame {l['frame_len']:6d} B, got {l['got']:6d}, lost {l['lost']:5d}; "
              f"first missing byte at {l['loss_start']:6d} (pkt byte {l['start_mod64']:2d}), resumes at {l['resume']:6d} "
              f"(pkt byte {l['resume_mod64']:2d}){'' if l['closed'] else ', tail lost'}")
    if losses:
        c = collections.Counter(l['start_mod64'] for l in losses)
        print("loss start, byte within 64 B packet:", dict(sorted(c.items())))
        c = collections.Counter(l['resume_mod64'] for l in losses)
        print("resume, byte within 64 B packet:", dict(sorted(c.items())))

if __name__ == "__main__":
    main()
