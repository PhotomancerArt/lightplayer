#!/usr/bin/env python3
"""Read `flash-reprogram` transcripts: does a re-programmed torn word settle?

    scripts/emu/flash-reprogram-analyze.py                      # every committed silicon transcript
    scripts/emu/flash-reprogram-analyze.py <transcript.txt>...  # just these
    scripts/emu/flash-reprogram-analyze.py --json               # one JSON object per torn word

The `flash-reprogram` payload (`lp-fw/fw-checks/src/checks/flash_reprogram/`)
prints, on every boot, the records this reads: `fr-boot` (the reset reason),
`fr-scan` (where the work loop's program stopped), `fr-window` and `fr-torn`
(the words around that point read 64 times; a bit that read both ways is
weak), and `fr-reads` for each torn word at three stages, 4,096 reads each:

- `before`: read left alone, before anything touches it (the control);
- `after`: read right after its write was programmed again with the
  intended bytes;
- `recheck`: read on the NEXT boot, after the next power cut.

Only boots that report reset `poweron` are cuts. A `recheck` counts as
"after a further power cycle" only when the boot that made it is one too
(the first boot of a batch is espflash's reset, `usb-uart`).

Reads the files; never edits them. Prints Markdown.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import statistics
import sys

TAG = "[fw-check-json] "
DEFAULT_GLOB = "lp-emu/transcripts/esp32c6/flash-reprogram/silicon-*.txt"
WRITE = 256
COMMAND = 32


def boots_of(path: str) -> list[dict]:
    """Every boot in a transcript, its records grouped by kind, in order."""
    boots: list[dict] = []
    cur: dict | None = None
    with open(path, "rb") as f:
        for raw in f:
            line = raw.decode("utf-8", errors="replace")
            if TAG not in line:
                continue
            try:
                rec = json.loads(line.split(TAG, 1)[1].strip())
            except json.JSONDecodeError:
                continue
            kind = rec.get("kind", "")
            if kind == "fr-boot":
                cur = {"path": path, "n": len(boots), "boot": rec, "records": []}
                boots.append(cur)
            elif cur is not None:
                cur["records"].append(rec)
    return boots


def of(boot: dict, kind: str, **match) -> list[dict]:
    return [
        r
        for r in boot["records"]
        if r.get("kind") == kind and all(r.get(k) == v for k, v in match.items())
    ]


def one(boot: dict, kind: str) -> dict | None:
    rs = of(boot, kind)
    return rs[0] if rs else None


def flicker(reads: dict) -> list[dict]:
    """Per bit that read both ways: total zeros and the per-chunk share."""
    per = reads["reads"] // reads["chunks"]
    out = []
    for b in reads.get("bits", []):
        z = b["zeros"]
        out.append(
            {
                "bit": b["bit"],
                "zeros": sum(z),
                "share": sum(z) / reads["reads"],
                "chunk_min": min(z) / per,
                "chunk_max": max(z) / per,
                "chunks_stuck": sum(1 for c in z if c in (0, per)),
            }
        )
    return out


def words_of(paths: list[str]) -> tuple[list[dict], list[dict]]:
    """(boots, torn words with their three stages)."""
    all_boots: list[dict] = []
    words: list[dict] = []
    for path in paths:
        boots = boots_of(path)
        all_boots.extend(boots)
        for i, b in enumerate(boots):
            nxt = boots[i + 1] if i + 1 < len(boots) else None
            for t in of(b, "fr-torn"):
                addr = t["addr"]
                w = {
                    "path": os.path.basename(path),
                    "boot": b["n"],
                    "reset": b["boot"]["reset"],
                    "torn": t,
                    "before": next(iter(of(b, "fr-reads", stage="before", addr=addr)), None),
                    "after": next(iter(of(b, "fr-reads", stage="after", addr=addr)), None),
                    "recheck": None,
                    "recheck_reset": None,
                }
                if nxt is not None:
                    w["recheck"] = next(iter(of(nxt, "fr-reads", stage="recheck", addr=addr)), None)
                    w["recheck_reset"] = nxt["boot"]["reset"]
                words.append(w)
    return all_boots, words


def outcome(reads: dict | None) -> str:
    if reads is None:
        return "—"
    if reads["correct"]:
        return "stable, correct"
    if reads["stable"]:
        return "stable, WRONG"
    return "still weak"


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("paths", nargs="*")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    paths = args.paths or sorted(glob.glob(DEFAULT_GLOB))
    if not paths:
        print(f"no transcripts ({DEFAULT_GLOB})", file=sys.stderr)
        return 1
    boots, words = words_of(paths)
    if args.json:
        for w in words:
            print(json.dumps(w))
        return 0

    # A `fresh` boot set the region up (no state yet): it scans nothing.
    cuts = [b for b in boots if b["boot"]["reset"] == "poweron" and b["boot"]["state"] != "fresh"]
    resets: dict[str, int] = {}
    for b in boots:
        resets[b["boot"]["reset"]] = resets.get(b["boot"]["reset"], 0) + 1
    p = print
    p(f"## `flash-reprogram`: {len(paths)} transcript(s), {len(boots)} boots, {len(cuts)} power cuts")
    p()
    p("Boots by reset: " + ", ".join(f"`{k}` {v}" for k, v in sorted(resets.items())))
    p()

    # Where the cuts landed.
    full = clean = torn_boots = 0
    shapes = {"write boundary": 0, "command boundary": 0, "inside a command, on a word": 0}
    write_us = []
    frontier_ms = []
    anomalies = []
    for b in cuts:
        scan = one(b, "fr-scan")
        win = one(b, "fr-window")
        timing = one(b, "fr-timing")
        if timing:
            write_us.append(timing["write_us_total"] / timing["writes"])
        if scan is None:
            anomalies.append(f"{b['path']} boot {b['n']}: no fr-scan")
            continue
        if scan["frontier"] is None:
            full += 1
            continue
        if win and win["torn_words"] > 0:
            torn_boots += 1
        else:
            clean += 1
            f = scan["frontier"]
            if f % WRITE == 0:
                shapes["write boundary"] += 1
            elif f % COMMAND == 0:
                shapes["command boundary"] += 1
            else:
                shapes["inside a command, on a word"] += 1
        if scan["span_other"] > 1 or (win and win["capped"]):
            anomalies.append(f"{b['path']} boot {b['n']}: span {scan['span_words']} words, other {scan['span_other']}")
        prep = one(b, "fr-prepare")
        if prep and not prep["blank"]:
            anomalies.append(f"{b['path']} boot {b['n']}: an erased sector did not read blank")
    per_write = statistics.median(write_us) if write_us else None
    if per_write:
        for b in cuts:
            scan = one(b, "fr-scan")
            if scan and scan["frontier"] is not None:
                frontier_ms.append(scan["frontier"] / WRITE * per_write / 1000)
    p("### Where the cut landed")
    p()
    p("| cut boots | region full (missed) | clean stop | torn word(s) |")
    p("|---:|---:|---:|---:|")
    p(f"| {len(cuts)} | {full} | {clean} | {torn_boots} |")
    p()
    if clean:
        p("Clean stops: " + ", ".join(f"{k} {v}" for k, v in shapes.items()) + ".")
        p()
    if per_write and frontier_ms:
        q = statistics.quantiles(frontier_ms, n=10) if len(frontier_ms) >= 10 else []
        p(
            f"A 256-byte write takes {per_write:.0f} µs (median of the boots' timed writes), so the "
            f"cut came {min(frontier_ms):.0f}–{max(frontier_ms):.0f} ms into the loop "
            f"(median {statistics.median(frontier_ms):.0f} ms"
            + (f", 10th–90th percentile {q[0]:.0f}–{q[-1]:.0f} ms" if q else "")
            + ")."
        )
        p()

    # The torn words.
    cut_words = [w for w in words if w["reset"] == "poweron"]
    weak_words = [w for w in cut_words if w["torn"]["weak"] > 0]
    p("### Torn words")
    p()
    p(
        f"{len(cut_words)} torn word(s) after power cuts, {len(weak_words)} with a weak bit "
        f"({sum(w['torn']['weak'] for w in weak_words)} weak bits)."
    )
    p()
    if cut_words:
        p(
            "| transcript | boot | addr | byte in write | clears landed | unlanded | weak (64 reads) "
            "| left alone, 4,096 reads | after re-program | after the next cut |"
        )
        p("|---|---:|---|---:|---:|---:|---:|---|---|---|")
        for w in cut_words:
            t = w["torn"]
            ctl = w["before"]
            if ctl is None:
                ctl_s = "—"
            else:
                fl = flicker(ctl)
                ctl_s = outcome(ctl)
                if fl:
                    ctl_s += "; " + ", ".join(
                        f"bit {x['bit']} 0 on {x['share']:.0%} ({x['chunk_min']:.0%}–{x['chunk_max']:.0%} a chunk)"
                        for x in fl
                    )
            re_s = outcome(w["recheck"])
            if w["recheck"] is not None and w["recheck_reset"] != "poweron":
                re_s += f" (reset `{w['recheck_reset']}`, not a cut)"
            p(
                f"| {w['path']} | {w['boot']} | {t['addr']} | {t['in_write']} | "
                f"{t['landed']} of {t['clears']} | {t['unlanded']} | {t['weak']} | {ctl_s} | "
                f"{outcome(w['after'])} | {re_s} |"
            )
        p()

    # The question.
    def tally(stage: str, only_cut: bool = False) -> dict[str, int]:
        out: dict[str, int] = {}
        for w in weak_words:
            if only_cut and w["recheck_reset"] != "poweron":
                continue
            k = outcome(w[stage])
            out[k] = out.get(k, 0) + 1
        return out

    p("### The answer, for words with a weak bit")
    p()
    p("| stage | outcome |")
    p("|---|---|")
    p("| left alone (4,096 reads) | " + ", ".join(f"{k} {v}" for k, v in tally("before").items()) + " |")
    p("| right after re-program (4,096 reads) | " + ", ".join(f"{k} {v}" for k, v in tally("after").items()) + " |")
    p(
        "| after the next power cut (4,096 reads) | "
        + ", ".join(f"{k} {v}" for k, v in tally("recheck", only_cut=True).items())
        + " |"
    )
    p()
    writes_after = [r for b in cuts for r in of(b, "fr-write", stage="after")]
    writes_re = [r for b in cuts for r in of(b, "fr-write", stage="recheck")]
    for name, ws in (("after re-program", writes_after), ("at the next boot's re-check", writes_re)):
        if ws:
            ok = sum(1 for r in ws if r["span_ok"] and r["tail_ok"])
            p(
                f"Re-programmed writes {name}: {ok} of {len(ws)} read as meant and stable over 64 reads, "
                f"every byte after the span still erased "
                f"({sum(1 for r in ws if not r['tail_ok'])} with a disturbed tail)."
            )
    p()
    if anomalies:
        p("### Anomalies")
        p()
        for a in anomalies:
            p(f"- {a}")
        p()
    return 0


if __name__ == "__main__":
    sys.exit(main())
