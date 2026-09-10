#!/usr/bin/env python3
"""Exact decoder-coverage census of an Xtensa artefact against `lp-xt-inst`.

Why this exists
---------------
`lp-xt/lp-xt-inst/src/bin/objdiff.rs` is the decoder-coverage oracle: it
disassembles an ELF with `lp-xt-inst`'s own `decode()` and diffs every
instruction against GNU `objdump`, so its numbers are `decode()`-exact rather
than a mnemonic-name-table proxy. As of M1 P1 (PR #660) it looks at **every**
`SHF_EXECINSTR` section of the ELF it is given, not just `.text` — so this
script hands it the artefact **whole** and lets it do that iteration itself.

2026-09-10 correction (was: per-section lifting, PR #657 → #660): the
original version of this script closed the "only `.text`" gap itself, before
`objdiff.rs` did, by using `objcopy` to lift each CODE section into its own
one-section ELF and running `objdiff` on that. That under-measured: lifting a
section drops the Xtensa configuration the original ELF carries (`e_flags`
and friends), and `xtensa-esp32-elf-objdump` then falls back to a
config-less opcode table where a loose `lsi` entry beats real MAC16 entries.
Same bytes, same objdump binary, same decoder — the classic mask ROM read
**96.52 % / 60 mismatches** section-by-section and **99.96 % / 0 mismatches**
read whole (PR #660's numbers; this script now reproduces the latter). See
`lp-xt/lp-xt-inst/README.md`'s "do not measure this with a per-section lifted
ELF" warning, which flagged the same failure mode in `objdiff.rs` before this
script's version of it was found.

This script now runs `objdiff` **once**, directly on the artefact (or, for
`--skip`, on an ELF-to-ELF `objcopy --remove-section` trim of it — see
`build_measured_elf`, which explains why *that* doesn't reintroduce the bug).
The per-section table below is metadata only, read back from `objdiff`'s own
"executable sections" listing; the instruction/coverage numbers are only ever
computed once, over the whole artefact, by `objdiff` itself.

The bootloader case (`--base`) is different in kind, not just degree: a raw
`espflash`-carved binary blob is not an ELF and never carried Xtensa
configuration to lose in the first place. Wrapping it with `objcopy -I binary
-O elf32-xtensa-le` (see `wrap_raw`) is the *only* ELF it ever has — there is
no "whole, unlifted" original to prefer it over, so that path is unchanged
from before and is not an instance of this bug.

Licence note (AGENTS.md): binutils is used here as a *tool whose output is
fact*. No binutils source, table, or logic is read or adapted.

Usage
-----
    scripts/emu/xtensa-inventory/census.py <artefact> [--out DIR]
        [--objdump PATH] [--objdiff PATH] [--base ADDR] [--json FILE]
        [--skip SECTION]...

`<artefact>` is an Xtensa ELF, or a raw binary when `--base` gives its load
address (that is the bootloader case). `--skip` names a CODE section to
exclude from the ELF case (ignored, with a warning, for `--base`, which has
only the one implicit section).

Output: a per-section metadata table, the aggregate totals, the ranked
unsupported mnemonics, and the special/user register census, on stdout;
optionally the same as JSON.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from collections import Counter
from pathlib import Path

DEFAULT_TOOLCHAIN = Path(
    os.path.expanduser(
        "~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin"
    )
)

# objdiff.rs's own "executable sections" listing, one line per section:
#   "  .text                vma=0x400d0020 size=1826353 bytes"
_REGION_RE = re.compile(r"^\s{2}(\S+)\s+vma=(0x[0-9a-f]+)\s+size=(\d+) bytes$")

# objdiff's own report lines.
_COUNT_RE = re.compile(r"^\s*(instructions \(objdump\)|supported \(decoded\)|matched|MISMATCHED|unsupported|data-directive bytes):\s+(\d+)")
_TALLY_RE = re.compile(r"^\s{2,}(\d+)\s{2}(\S.*)$")

SR_RE = re.compile(r"^(rsr|wsr|xsr|rur|wur)\.(\S+)$")


def run(cmd: list[str]) -> str:
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode not in (0, 1):  # objdiff exits 1 when mismatches exist
        sys.exit(f"census: command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stderr}")
    return proc.stdout


def build_measured_elf(objcopy: Path, elf: Path, skip: list[str], workdir: Path) -> Path:
    """The ELF actually handed to `objdiff`.

    With no `--skip`, this is the artefact unchanged — the whole point of
    this rewrite is to stop constructing a derived ELF at all. `--skip`
    still needs *some* derived ELF (to drop a named CODE section from the
    measurement), but it does it as an ELF-to-ELF `objcopy
    --remove-section`, not a binary round-trip: `objcopy` copies the input
    ELF's header (including `e_flags`, the Xtensa configuration the original
    per-section lifting dropped) and only strips the named section's
    contents, so the surviving sections keep the same config context they'd
    have had unmodified. That is a different `objcopy` mode from the one
    this script used to use (`-I binary -O elf32-xtensa-le`, which builds a
    *new* ELF header from scratch and carries no source config at all — the
    actual bug), so `--skip` does not reintroduce it.
    """
    if not skip:
        return elf
    out = workdir / f"skip_{'_'.join(s.strip('.').replace('.', '_') for s in skip)}.elf"
    cmd = [str(objcopy)]
    for name in skip:
        cmd += ["--remove-section", name]
    cmd += [str(elf), str(out)]
    run(cmd)
    return out


def wrap_raw(objcopy: Path, blob: Path, vma: int, workdir: Path) -> Path:
    """Wrap a raw binary blob (the bootloader case) as a one-section ELF.

    Not an instance of the section-lifting bug: `blob` is raw bytes with no
    ELF header of its own, so there is no original Xtensa configuration this
    wrapping could drop. It is the only ELF this artefact ever has.
    """
    out = workdir / (blob.stem + ".elf")
    run(
        [
            str(objcopy),
            "-I", "binary",
            "-O", "elf32-xtensa-le",
            "-B", "xtensa",
            "--rename-section", ".data=.text",
            "--set-section-flags", ".text=alloc,load,readonly,code",
            "--change-section-address", f".data={vma:#x}",
            str(blob),
            str(out),
        ]
    )
    return out


def parse_objdiff(text: str) -> dict:
    """Pull the section listing, counters, and the two mnemonic tallies out
    of a whole-artefact `objdiff` report."""
    regions: list[tuple[str, int, int]] = []
    counts: dict[str, int] = {}
    supported: Counter = Counter()
    unsupported: Counter = Counter()
    bucket = None
    for line in text.splitlines():
        m = _REGION_RE.match(line)
        if m:
            regions.append((m.group(1), int(m.group(3)), int(m.group(2), 16)))
            continue
        m = _COUNT_RE.match(line)
        if m:
            counts[m.group(1)] = int(m.group(2))
            continue
        if line.startswith("--- supported opcodes"):
            bucket = supported
            continue
        if line.startswith("--- UNSUPPORTED allowlist"):
            bucket = unsupported
            continue
        if line.startswith("--- MISMATCHES"):
            bucket = None
            continue
        if bucket is not None:
            t = _TALLY_RE.match(line)
            if t:
                bucket[t.group(2).strip()] += int(t.group(1))
    return {
        "regions": regions,
        "insns": counts.get("instructions (objdump)", 0),
        "decoded": counts.get("supported (decoded)", 0),
        "matched": counts.get("matched", 0),
        "mismatched": counts.get("MISMATCHED", 0),
        "unsupported_n": counts.get("unsupported", 0),
        "data_bytes": counts.get("data-directive bytes", 0),
        "supported": supported,
        "unsupported": unsupported,
        "raw": text,
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("artefact", type=Path)
    ap.add_argument("--base", help="load address; makes <artefact> a raw binary")
    ap.add_argument("--objdump", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-objdump")
    ap.add_argument("--objcopy", type=Path, default=DEFAULT_TOOLCHAIN / "xtensa-esp32-elf-objcopy")
    ap.add_argument("--objdiff", type=Path, default=Path("target/release/objdiff"))
    ap.add_argument("--out", type=Path, help="directory to keep the objdiff report")
    ap.add_argument("--json", type=Path)
    ap.add_argument("--skip", action="append", default=[], help="CODE section name to exclude (ELF artefacts only)")
    args = ap.parse_args()

    if not args.objdiff.exists():
        sys.exit(
            f"census: {args.objdiff} not found — build it first:\n"
            "  cargo build -p lp-xt-inst --features objdiff --bin objdiff --release"
        )

    tmp = tempfile.TemporaryDirectory(prefix="xt-census-")
    workdir = Path(tmp.name)
    outdir = args.out
    if outdir:
        outdir.mkdir(parents=True, exist_ok=True)

    if args.base:
        if args.skip:
            print("census: --skip has no effect with --base (one implicit section)", file=sys.stderr)
        base = int(args.base, 0)
        measured = wrap_raw(args.objcopy, args.artefact, base, workdir)
    else:
        measured = build_measured_elf(args.objcopy, args.artefact, args.skip, workdir)

    env = dict(os.environ, XT_OBJDUMP=str(args.objdump))
    proc = subprocess.run(
        [str(args.objdiff), str(measured)], capture_output=True, text=True, env=env
    )
    if proc.returncode not in (0, 1):  # objdiff exits 1 when mismatches exist
        sys.exit(f"census: objdiff failed ({proc.returncode}): {proc.stderr}")
    rep = parse_objdiff(proc.stdout)
    if outdir:
        (outdir / "objdiff.txt").write_text(proc.stdout)

    regions = rep["regions"]
    tot_sup = rep["supported"]
    tot_uns = rep["unsupported"]
    totals = {k: rep[k] for k in ("insns", "decoded", "matched", "mismatched", "unsupported_n", "data_bytes")}

    def pct(n: int, d: int) -> str:
        return f"{100.0 * n / d:.2f}%" if d else "n/a"

    print(f"=== xtensa-inventory census: {args.artefact} ===")
    print(f"objdump: {args.objdump}")
    print(f"objdiff run whole over: {measured}" + (" (skip: " + ", ".join(args.skip) + ")" if args.skip else ""))
    print()
    print(f"{'section':<28} {'bytes':>9} {'vma':>10}")
    for name, size, vma in regions:
        print(f"{name:<28} {size:>9} {vma:>#10x}")
    print(f"{'TOTAL (metadata only)':<28} {sum(s for _, s, _ in regions):>9} {'':>10}")
    print(
        "note: instruction/coverage numbers below are measured once, over the "
        "whole artefact above — objdiff iterates its own executable sections "
        "internally, so there is no per-section breakdown to print here "
        "without re-lifting sections (the bug this rewrite removes)."
    )
    print()
    print(
        f"instructions {totals['insns']}  decoded {totals['decoded']}  "
        f"matched {totals['matched']}  mismatched {totals['mismatched']}  "
        f"unsupported {totals['unsupported_n']}  data-bytes {totals['data_bytes']}"
    )
    print()

    print(f"exact coverage: {totals['decoded']}/{totals['insns']} = {pct(totals['decoded'], totals['insns'])} decoded; "
          f"{totals['matched']}/{totals['insns']} = {pct(totals['matched'], totals['insns'])} decoded AND matching objdump")
    print()

    print(f"--- unsupported mnemonics ({len(tot_uns)} kinds, {sum(tot_uns.values())} sites) ---")
    for mnem, n in tot_uns.most_common():
        print(f"  {n:>7}  {mnem}")
    print()

    srs: Counter = Counter()
    for src in (tot_sup, tot_uns):
        for mnem, n in src.items():
            m = SR_RE.match(mnem.split(" ")[0])
            if m:
                srs[(m.group(2), m.group(1))] += n
    print(f"--- special / user registers ({len({r for r, _ in srs})} registers, {sum(srs.values())} sites) ---")
    for (reg, op), n in sorted(srs.items(), key=lambda kv: (-kv[1], kv[0])):
        print(f"  {n:>7}  {op}.{reg}")
    print()

    print(f"--- supported mnemonics ({len(tot_sup)} kinds, {sum(tot_sup.values())} sites) ---")
    for mnem, n in tot_sup.most_common():
        print(f"  {n:>7}  {mnem}")

    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "artefact": str(args.artefact),
                    "measured_elf": str(measured),
                    "sections": [{"name": n, "bytes": s, "vma": v} for n, s, v in regions],
                    "totals": totals,
                    "unsupported": dict(tot_uns),
                    "supported": dict(tot_sup),
                    "special_registers": {f"{op}.{reg}": n for (reg, op), n in srs.items()},
                },
                indent=2,
            )
        )
    tmp.cleanup()


if __name__ == "__main__":
    main()
