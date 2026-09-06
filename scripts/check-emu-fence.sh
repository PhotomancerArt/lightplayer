#!/usr/bin/env bash
# The lp-emu MIT fence.
#
# Everything under `lp-emu/` is MIT as a unit (vision D2, plan PD2,
# docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md). The rest of the repo is
# AGPL-3.0-or-later. This lint is what keeps the boundary real rather than
# aspirational, and it checks two things:
#
#   1. every workspace package whose manifest lives under `lp-emu/` declares
#      exactly `license = "MIT"`;
#   2. no crate under `lp-emu/` reaches a workspace-local crate OUTSIDE
#      `lp-emu/`, transitively, except the crates named in ALLOWED_OUTSIDE
#      below — each with the reason it is allowed.
#
# Rule 2 walks the DECLARED dependencies from `cargo metadata` (normal, dev and
# build, optional ones included), not the resolved graph: a dependency behind a
# cargo feature is still an import, and a fence that only sees the default
# feature set is a fence with a gate in it.
#
# The `lp-xt-emu-guest` crate is a member of the `lp-xt/fixtures` device-target
# workspace, not the root one, so `cargo metadata` here does not see it and
# running cargo in that directory would pull the esp toolchain. It gets a
# manifest-level check instead (below); it declares no dependencies at all.
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - "$PWD" <<'PY'
import json
import os
import subprocess
import sys

root = sys.argv[1]

# name -> why it is allowed to sit outside the fence.
#
# Adding an entry means accepting that the MIT unit is not self-contained on
# that edge. Say why, and prefer deleting the dependency.
ALLOWED_OUTSIDE = {
    # Compiler-backend infrastructure shared with `lpvm-native`; they stay
    # outside `lp-emu/` by vision Q3 and are AGPL today. Whether they flip to
    # MIT too is escalation E1 in the director log — the open question at G1.
    "lp-riscv-inst": "rv32 ISA encode/decode (vision Q3, AGPL; see E1)",
    "lp-riscv-elf": "rv32 ELF loader (vision Q3, AGPL; see E1)",
    "lp-xt-inst": "Xtensa ISA encode/decode (vision Q3, AGPL; see E1)",
    "lp-xt-elf": "Xtensa ELF loader (vision Q3, AGPL; see E1)",
    # The FP conformance vector corpus. A dev-dependency of lp-xt-emu only
    # (tests/fp_conformance.rs), and the same crate fw-esp32s3's device harness
    # uses — which is what makes the vectors identical on both sides. Same AGPL
    # question as the four above.
    "lp-xt-fp-vectors": "FP vector corpus, dev-dep only (AGPL; see E1)",
    # The rv32 guest runtime's crash-staging path (panic.rs) and its optional
    # profiling hook (allocator.rs, behind `profile`). AGPL, under lp-base/.
    # Not covered by E1 as written; raised at G1 alongside it.
    "lp-recovery": "guest panic -> staged crash record (AGPL; raised at G1)",
    "lp-perf": "guest free-list-shape hook, optional `profile` (AGPL; G1)",
    # Already MIT, and the WS281x strip decoder in M5 will need it.
    "lp-ws281x": "MIT already; the M5 strip decoder's encoder counterpart",
}

md = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=root))

# Workspace-local packages, keyed by their manifest directory and by name.
by_dir = {}
for p in md["packages"]:
    by_dir[os.path.dirname(p["manifest_path"])] = p


def relpath(p):
    return os.path.relpath(os.path.dirname(p["manifest_path"]), root)


def in_fence(p):
    return relpath(p).split(os.sep)[0] == "lp-emu"


fence = sorted((p for p in md["packages"] if in_fence(p)),
               key=lambda p: p["name"])
if not fence:
    sys.exit("emu fence: found no packages under lp-emu/ — is this the repo root?")

fail = 0

# --- rule 1: MIT ------------------------------------------------------------
for p in fence:
    if p["license"] != "MIT":
        print("NOT MIT: %s (%s) declares license = %r, expected \"MIT\""
              % (p["name"], relpath(p), p["license"]))
        fail = 1

# --- rule 2: no reach outside the fence -------------------------------------
def local_deps(p):
    """Declared path dependencies of `p` that are workspace-local packages."""
    out = []
    for d in p["dependencies"]:
        path = d.get("path")
        if not path:
            continue  # registry / git dependency: not our concern
        dep = by_dir.get(os.path.normpath(path))
        if dep is not None:
            out.append((d.get("kind") or "normal", dep))
    return out


for rootpkg in fence:
    seen = set()
    # (chain-of-names, package)
    stack = [([rootpkg["name"]], rootpkg)]
    while stack:
        chain, cur = stack.pop()
        for kind, dep in local_deps(cur):
            key = (cur["name"], dep["name"])
            if key in seen:
                continue
            seen.add(key)
            nchain = chain + [dep["name"]]
            if in_fence(dep):
                stack.append((nchain, dep))
                continue
            if dep["name"] in ALLOWED_OUTSIDE:
                continue
            print("FENCE BREACH: %s" % " -> ".join(nchain))
            print("    %s (%s dep) has its manifest at %s, outside lp-emu/"
                  % (dep["name"], kind, relpath(dep)))
            print("    license: %s" % dep["license"])
            fail = 1

# --- the out-of-tree guest --------------------------------------------------
guest = os.path.join(root, "lp-emu/lp-xt-emu-guest/Cargo.toml")
if not os.path.exists(guest):
    print("MISSING: lp-emu/lp-xt-emu-guest/Cargo.toml (a member of the")
    print("    lp-xt/fixtures workspace; this lint checks it by hand)")
    fail = 1
else:
    text = open(guest).read()
    if 'license = "MIT"' not in text:
        print("NOT MIT: lp-xt-emu-guest does not declare license = \"MIT\"")
        fail = 1
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("#") or "path" not in stripped:
            continue
        if "path = " in stripped and "workspace = " not in stripped:
            print("FENCE BREACH: lp-xt-emu-guest gained a path dependency:")
            print("    %s" % stripped)
            print("    (it had none; add it to this lint's walk before "
                  "adding one)")
            fail = 1

if fail:
    print()
    print("Everything under lp-emu/ is MIT as a unit and must not import the")
    print("AGPL product crates. If a dependency is genuinely needed, delete it")
    print("first; if it cannot be deleted, add it to ALLOWED_OUTSIDE in")
    print("scripts/check-emu-fence.sh with the reason, and say so in the PR.")
    print("See docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md.")
    sys.exit(1)

print("lp-emu MIT fence: OK (%d crates in the fence, %d crates allowlisted "
      "outside it)" % (len(fence) + 1, len(ALLOWED_OUTSIDE)))
PY
