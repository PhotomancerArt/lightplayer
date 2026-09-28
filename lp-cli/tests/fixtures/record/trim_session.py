#!/usr/bin/env python3
"""Trim a `?record=` session into `emulated-c6-session.jsonl`, by `seq`.

Lines are kept VERBATIM (never edited) and only chosen; the rule is here so
the fixture can be re-cut from a new recording the same way:

  python3 trim_session.py <recording.jsonl> <wire-until-seq> <host-wire-until-seq> \\
      <fail-from-s> <fail-to-s> > emulated-c6-session.jsonl

- the session line, every route, error, toast, open and pool line, and the
  hotplug commands;
- journal lines that are link events (opened, closed, detached, wire notes)
  or an activity's end;
- board-to-host wire lines with `seq` <= <wire-until-seq>, and host-to-board
  ones with `seq` <= <host-wire-until-seq> — a PREFIX of each direction, from
  the link's first chunk, because a link's frames (and its learned packed
  table) decode only from the link's start. Set the host cut BEFORE the
  page's first `accessAdd`: that frame carries the browser's access key, and
  no fixture may hold one (the chunks are raw bytes, so there is no redacting
  them in place);
- request lines with `seq` <= <wire-until-seq>, and those whose time since
  the session start is inside [<fail-from-s>, <fail-to-s>] (the request a
  cable pull failed).
"""

import json
import sys

KEEP_KINDS = {"session", "route", "error", "toast", "open", "pool"}
JOURNAL_MARKS = ("WireNote", "LinkDetached", "Closed {", "ActivityEnded", "event: Opened {")


def main():
    path, wire_until, host_until = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    fail_from, fail_to = float(sys.argv[4]), float(sys.argv[5])
    lines = open(path, encoding="utf-8").read().splitlines()
    t0 = None
    for raw in lines:
        rec = json.loads(raw)
        kind = rec.get("kind")
        if kind == "session":
            t0 = rec.get("t", t0)
        t = rec.get("t")
        if t0 is None and t is not None:
            t0 = t
        since = (t - t0) if (t is not None and t0 is not None) else None
        keep = (
            kind in KEEP_KINDS
            or (kind == "command" and rec.get("name", "").startswith("DeviceHotplug"))
            or (kind == "journal" and any(m in rec.get("entry", "") for m in JOURNAL_MARKS))
            or (kind == "wire" and rec.get("dir") == "rx" and rec["seq"] <= wire_until)
            or (kind == "wire" and rec.get("dir") == "tx" and rec["seq"] <= host_until)
            or (
                kind == "request"
                and (rec["seq"] <= wire_until or (since is not None and fail_from <= since <= fail_to))
            )
        )
        if keep:
            print(raw)


if __name__ == "__main__":
    main()
