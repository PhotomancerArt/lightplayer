#!/usr/bin/env python3

import argparse
import os
import signal
import subprocess
import sys
import time
from pathlib import Path


def terminate_group(proc: subprocess.Popen[bytes]) -> None:
    if proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.time() + 3.0
    while time.time() < deadline:
        if proc.poll() is not None:
            return
        time.sleep(0.05)
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Run a command, tee stdout/stderr to a file, and stop once a marker appears."
    )
    parser.add_argument("--marker", required=True, help="Stop after this text appears in output.")
    parser.add_argument("--output", required=True, help="Path to the trace file to write.")
    parser.add_argument("cmd", nargs=argparse.REMAINDER, help="Command to run after `--`.")
    args = parser.parse_args()

    cmd = args.cmd
    if cmd and cmd[0] == "--":
        cmd = cmd[1:]
    if not cmd:
        parser.error("missing command after `--`")

    out_path = Path(args.output).expanduser().resolve(strict=False)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    proc = subprocess.Popen(
        cmd,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        stdin=None,
        text=False,
        bufsize=0,
        start_new_session=True,
    )

    marker = args.marker.encode("utf-8")
    saw_marker = False
    tail = b""
    try:
        with out_path.open("wb") as f:
            assert proc.stdout is not None
            while True:
                chunk = proc.stdout.read(4096)
                if not chunk:
                    break
                sys.stdout.buffer.write(chunk)
                sys.stdout.buffer.flush()
                f.write(chunk)
                f.flush()
                haystack = tail + chunk
                if marker in haystack:
                    saw_marker = True
                    terminate_group(proc)
                    break
                tail = haystack[-max(len(marker) - 1, 0) :]
    finally:
        terminate_group(proc)

    rc = proc.wait()
    if saw_marker:
        print(out_path)
        return 0
    return rc


if __name__ == "__main__":
    raise SystemExit(main())
