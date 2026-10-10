#!/usr/bin/env python3
"""E9: trace the workloads E8 did not capture, on the alloc-trace split image.

Boots fw-esp32c6's `alloc_trace_emu` split image under `lp-cli emu run
--alloc-trace`, with D0 held high (the choker's PowerButton opens), and drives
one workload over the in-process USB host link:

- `choker-edits`: upload Logo Sign (a copy on the XIAO's D10/D9, as E8's
  switch run made) and the PLAYFUL Choker (left running); ten shader edits,
  each a filesystem write of the choker's shader with one more line of work
  than the last (the `emu_edit_frag` test's growing edit, so every recompile's
  code is a little bigger); then a switch to Logo Sign and back. Each request
  is followed by `--request-gap` of emulated time.
- `meteor`: upload `catalog/patterns/meteor` and let it run (a
  `listLoadedProjects` and one `--request-gap` of steady running).

Both end with `stopAllProjects` and one more gap, so every block the project
held is seen to die at its unload: a lifetime census needs the project's end
in the trace, not only its start.

Writes `<out>.trace` and `<out>.console`. Usage:

    e09-trace-run.py <image dir> <workload> <out prefix> [gap ms] [extra lp-cli args…]

The image is `just fw-esp32c6-split esp32c6,server,radio,alloc_trace_emu`'s
output dir (`target/fw-split/esp32c6_server_radio_alloc_trace_emu`).
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

CHOKER = Path("catalog/projects/playful-choker")
EDIT_ANCHOR = "    vec3 color = texture(palette"


def logo_xiao() -> Path:
    """Logo Sign's outputs are a DevKit's (IO18, IO13 — the C6's USB D+); the
    emulated board is a XIAO. A copy on its D10/D9 (E8's retarget)."""
    logo = Path("target/e09/logo-sign-xiao")
    shutil.rmtree(logo, ignore_errors=True)
    shutil.copytree("catalog/projects/logo-sign", logo)
    out_json = logo / "output.json"
    out_json.write_text(
        out_json.read_text()
        .replace("ws281x:local:IO18", "ws281x:local:D10")
        .replace("ws281x:local:IO13", "ws281x:local:D9")
    )
    return logo


def edited_shader(original: str, edits: int) -> str:
    """`lp-cli/tests/emu_edit_frag.rs`'s `edited_shader`: `edits` lines of
    work inserted before the palette lookup."""
    at = original.index(EDIT_ANCHOR)
    lines = "".join(
        f"    lum *= 1.0 + 0.01 * sin(p.x * {e}.0 + time);\n" for e in range(1, edits + 1)
    )
    return original[:at] + lines + original[at:]


def main() -> int:
    image, workload, out = sys.argv[1], sys.argv[2], sys.argv[3]
    gap = sys.argv[4] if len(sys.argv) > 4 else "2500"
    extra = sys.argv[5:]
    Path("target/e09").mkdir(parents=True, exist_ok=True)
    pins = Path("target/e09/d0-high.pins")
    pins.write_text("# D0 (the PowerButton switch) high from power-on\n0 pin 0 1\n")

    uploads: list[str] = []
    reqs: list[str] = []
    if workload == "choker-edits":
        logo = logo_xiao()
        uploads = [str(logo), str(CHOKER)]
        name = json.load(open(CHOKER / "project.json"))["name"]
        original = (CHOKER / "shader.glsl").read_text()
        for edit in range(1, 11):
            body = edited_shader(original, edit)
            reqs.append(json.dumps({"filesystem": {"write": {
                "path": f"/projects/{name}/shader.glsl", "data": body}}}))
        reqs.append(json.dumps({"loadProject": {"path": "projects/Logo Sign"}}))
        reqs.append(json.dumps({"loadProject": {"path": f"projects/{name}"}}))
        reqs.append(json.dumps("stopAllProjects"))
        emulated_s = 20 + len(reqs) * (int(gap) / 1000 + 1.0) + 8
    elif workload == "choker-studio":
        # Studio's staged initial sync (`lp-cli profile --workload
        # studio-sync`'s three stages: the skeleton, the slot detail, the
        # binding-graph probe), here with ONE slot read of every node rather
        # than pages of 16 (the trace runs with no client to learn the ids;
        # the choker has fewer than 16 nodes, so it is one page either way),
        # then the card's feed: 30 output-frame reads, the first with its
        # geometry, the rest without (Studio's steady feed holds it).
        uploads = [str(CHOKER)]
        h = 1  # the first project loaded on a fresh board
        def read(request):
            return json.dumps({"projectRead": {"handle": h, "request": {"since": None, **request}}})
        reqs.append(read({"queries": [
            {"shapes": {"level": "detail"}},
            {"nodes": {"level": "detail", "nodes": "all", "include_slots": False}},
            {"resources": {"level": "summary", "payloads": "none"}},
            {"runtime": None}]}))
        reqs.append(read({"queries": [
            {"nodes": {"level": "detail", "nodes": "all", "include_slots": True}}]}))
        reqs.append(read({"probes": [
            {"binding_graph": {"structure": "always", "include_values": False}}]}))
        reqs.append(read({"probes": [{"output_frame": {"geometry": "always", "samples": "srgb8"}}]}))
        for _ in range(29):
            reqs.append(read({"probes": [{"output_frame": {"geometry": "none", "samples": "srgb8"}}]}))
        reqs.append(json.dumps("stopAllProjects"))
        emulated_s = 10 + len(reqs) * (int(gap) / 1000 + 0.3) + 5
    elif workload == "meteor":
        uploads = ["catalog/patterns/meteor"]
        reqs = [json.dumps("listLoadedProjects"), json.dumps("stopAllProjects")]
        emulated_s = 5 + len(reqs) * (int(gap) / 1000 + 1.0)
    else:
        print(f"unknown workload {workload}", file=sys.stderr)
        return 2

    cmd = [
        "target/release/lp-cli", "emu", "run",
        "--elf", f"{image}/loader.elf", "--over", f"{image}/merged.bin", "--mmu-page", "32k",
        "--host-link", "--link-nonce", "0e090001",
    ]
    for u in uploads:
        cmd += ["--upload", u]
    for r in reqs:
        cmd += ["--request", r]
    if reqs:
        cmd += ["--request-gap", gap]
    cmd += [
        "--pin-script", str(pins),
        "--alloc-trace", f"{out}.trace", "--alloc-trace-elf", f"{image}/p2.elf",
        "--console", f"{out}.console",
        "--timeout", f"{int(emulated_s)}s", "--wall-timeout", "580",
        *extra,
    ]
    shown = [c if len(c) < 120 else c[:100] + "…" for c in cmd]
    print(" ".join(shown), file=sys.stderr)
    return subprocess.call(cmd)


if __name__ == "__main__":
    sys.exit(main())
