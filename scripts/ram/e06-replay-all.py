#!/usr/bin/env python3
"""E6: run `tlsf-replay.py replay` over every C6 first-fit trace E8 and E9
recorded, in each heap configuration, and write one summary table.

    e06-replay-all.py <dir of decompressed traces> <elf dir> <out dir> [workload…]

`<elf dir>` holds `e09-llff-p2.elf`, `e08-p2.elf`, `e08-p2-payload-cache-off.elf`
(the traces' own symbol tables, gunzipped from the planning folder).
"""
import json
import subprocess
import sys
from pathlib import Path

# workload -> (trace stem, its symbol table, the boot that holds the workload:
# the auto-load traces' workload is boot 1, after the reboot).
TRACES = {
    "choker-edits": ("c6-choker-edits", "e09-llff-p2.elf", 0),
    "meteor": ("c6-meteor", "e09-llff-p2.elf", 0),
    "choker-studio": ("c6-choker-studio", "e09-llff-p2.elf", 0),
    "lab-copy-autoload": ("c6-lab-rehearsal-autoload", "e08-p2.elf", 1),
    "catalog-choker-autoload": ("c6-choker-autoload", "e08-p2.elf", 1),
    "choker-upload-packed": ("c6-choker-upload-packed", "e08-p2.elf", 0),
    "reboot-5-switches": ("c6-switch-reboot-then-5-pairs", "e08-p2.elf", 1),
    "lab-copy-reboot-5-switches": ("c6-switch-lab-copy-reboot-then-5-pairs", "e08-p2.elf", 1),
    "20-switch-pairs": ("c6-switch-20-pairs", "e08-p2.elf", 0),
    "lab-copy-autoload-cache-off": ("c6-lab-rehearsal-autoload-payload-cache-off",
                                    "e08-p2-payload-cache-off.elf", 1),
}
# (name, regions, fllen): the allocator alone on today's regions; the FLLEN=14
# fork with the stack kept (main less its 9,184 B of extra .bss); the stock
# flip with the stack kept (main less 21,064 B).
CONFIGS = [
    ("tlsf-same-regions", "main=186848,dram2=65536,radio=49152", 32),
    ("tlsf14-keep-stack", "main=177664,dram2=65536,radio=49152", 14),
    ("tlsf-stock-keep-stack", "main=165784,dram2=65536,radio=49152", 32),
    # Diagnostic, not a buildable layout: main given 32 KiB more than today,
    # about TLSF's whole per-block overhead back, so the placement policy can
    # be read apart from what the headers cost. First fit gets the same.
    ("diag-tlsf-main+32k", "main=219616,dram2=65536,radio=49152", 32),
]


def main() -> int:
    tdir, edir, out = map(Path, sys.argv[1:4])
    only = sys.argv[4:]
    out.mkdir(parents=True, exist_ok=True)
    table = []
    for wl, (stem, elf, boot) in TRACES.items():
        if only and wl not in only:
            continue
        for cfg, regions, fllen in CONFIGS:
            prefix = out / f"{wl}.{cfg}"
            res = subprocess.run(
                [sys.executable, str(Path(__file__).with_name("tlsf-replay.py")), "replay",
                 str(tdir / f"{stem}.trace"), "--elf", str(edir / elf), "--regions", regions,
                 "--fllen", str(fllen), "--boot", str(boot), "--out", str(prefix)],
                capture_output=True, text=True, check=True)
            s = json.loads(res.stdout)
            table.append({"workload": wl, "config": cfg, "boot": boot, **s})
            print(f"{wl:28} {cfg:22} ff {s['worst_ff_largest']:>6} ff-here "
                  f"{s['worst_ff_same_regions']:>6}/{s['ff_same_regions_would_oom']} tlsf-req "
                  f"{s['worst_tlsf_request']:>6} hole {s['worst_tlsf_hole']:>6} oom "
                  f"{s['tlsf_would_oom']:>5} ovh {s['median_overhead']} max {s['max_overhead']}",
                  flush=True)
    name = "summary.json" if not only else f"summary-{'-'.join(only)}.json"
    (out / name).write_text(json.dumps(table, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
