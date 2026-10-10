#!/usr/bin/env python3
"""E8 (H8c): switch an emulated C6 between two projects N times, traced.

Boots the alloc-trace split image (fw-esp32c6 `alloc_trace_emu`), uploads
Logo Sign and then the PLAYFUL Choker (left running), and sends N pairs of
`loadProject` (Logo Sign, then the choker), each followed by `--request-gap`
of emulated time to settle (first frames, compile). D0 is held high so the
choker's PowerButton opens. Writes `<out>.trace` and `<out>.console`.

Usage: e08-switch-run.py <image dir> <out prefix> <pairs> [gap ms] [extra lp-cli args…]
"""

import json
import subprocess
import sys

import shutil
from pathlib import Path

# Logo Sign's outputs are a DevKit's (IO18, IO13 — the C6's USB D+); the
# emulated board is a XIAO, as the silicon board was. A copy on its D10/D9.
LOGO = Path("target/e08/logo-sign-xiao")
shutil.rmtree(LOGO, ignore_errors=True)
shutil.copytree("catalog/projects/logo-sign", LOGO)
out_json = LOGO / "output.json"
out_json.write_text(
    out_json.read_text()
    .replace("ws281x:local:IO18", "ws281x:local:D10")
    .replace("ws281x:local:IO13", "ws281x:local:D9")
)

image, out, pairs = sys.argv[1], sys.argv[2], int(sys.argv[3])
gap = sys.argv[4] if len(sys.argv) > 4 else "3000"
extra = sys.argv[5:]
# `E08_REBOOT_FIRST=1`: reboot once before switching (the board comes back
# with the choker as its startup project), so the switches start from a
# boot-loaded choker, as the silicon board's did.
import os
reboot_first = os.environ.get("E08_REBOOT_FIRST") == "1"
reqs = ["--request", json.dumps("reboot")] if reboot_first else []
# `E08_CHOKER=<dir>`: the choker copy to upload and switch to (default the
# catalog's); its project.json name is the path the switches load.
choker_dir = os.environ.get("E08_CHOKER", "catalog/projects/playful-choker")
choker_name = json.load(open(f"{choker_dir}/project.json"))["name"]
for _ in range(pairs):
    reqs += ["--request", json.dumps({"loadProject": {"path": "projects/Logo Sign"}})]
    reqs += ["--request", json.dumps({"loadProject": {"path": f"projects/{choker_name}"}})]
emulated_s = 6 + pairs * 2 * (int(gap) / 1000 + 0.6) + 5 + (8 if reboot_first else 0)
cmd = [
    "target/release/lp-cli", "emu", "run",
    "--elf", f"{image}/loader.elf", "--over", f"{image}/merged.bin", "--mmu-page", "32k",
    "--host-link", "--link-nonce", "0e080002",
    "--upload", str(LOGO),
    "--upload", choker_dir,
    *reqs,
    "--request-gap", gap,
    "--pin-script", "target/e08/d0-high.pins",
    "--alloc-trace", f"{out}.trace", "--alloc-trace-elf", f"{image}/p2.elf",
    "--console", f"{out}.console",
    "--timeout", f"{int(emulated_s)}s", "--wall-timeout", "580",
    *(["--reboot-on-reset"] if reboot_first else []),
    *extra,
]
print(" ".join(c if " " not in c and "{" not in c else repr(c) for c in cmd), file=sys.stderr)
sys.exit(subprocess.call(cmd))
