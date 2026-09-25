#!/usr/bin/env bash
# Nested Cargo workspaces must repeat the root's `[patch]` entries.
#
# A `[patch]` table applies only to the workspace whose root manifest holds it.
# Any OTHER workspace in this repo (today `lp-xt/fixtures`, whose `mach` reaches
# lpc-wire through lpc-shared) that path-depends into product crates resolves
# the root's forks (ser-write-json, naga, pp-rs, esp-hal, …) from crates.io
# instead — silently, until the fork's behaviour is needed and CI breaks with an
# error pointing at the product crate. See
# docs/debt/nested-workspaces-miss-root-patches.md.
#
# For every nested workspace root (a Cargo.toml git knows about with a
# `[workspace]` table, outside target/ and dot-directories) not in EXCLUDED
# below, and for every crate the root patches that the nested graph reaches,
# this checks:
#
#   1. the nested manifest's `[patch.<same key>]` entry for it points at the
#      same source as the root's (same path, or same git url + ref);
#   2. its committed Cargo.lock does not resolve the crate from the registry
#      (or another git source) — i.e. the patch has actually taken effect;
#   3. a Cargo.lock that resolves it to a path has something in the manifests
#      that puts it there (a `[patch]` or a direct path dependency) — otherwise
#      the lockfile is stale and cargo will re-resolve it to crates.io on the
#      next build.
#
# "Reaches" is the union of (a) every package named in the nested Cargo.lock
# and (b) a walk of the manifests from the nested members through their path
# dependencies, collecting NON-optional registry/git dependencies. (b) catches
# a new reach before anyone has re-locked; (a) catches reaches through optional
# dependencies turned on by features, which (b) does not model.
#
# Why this reads files instead of `cargo metadata --offline`: `cargo metadata`
# needs every resolved package's sources on disk, and the nested lockfiles pin
# crates the root does not (16 of lp-xt/fixtures' 29 registry packages on
# 2026-09-23). CI's Lint job restores only the ROOT workspace's registry cache,
# so `--offline` fails there, and running it online would make a lint depend on
# the network. `lp-xt/fixtures` also pins the `esp` toolchain, which Lint does
# not install. Reading Cargo.toml + Cargo.lock needs neither, and rule 3 is what
# keeps a lockfile-based check honest when a manifest changes without a re-lock.
# Runtime: ~0.3 s (one git ls-files, a few dozen TOML files, no cargo).
set -euo pipefail
cd "$(dirname "$0")/.."

python3 - "$PWD" <<'PY'
import glob
import os
import subprocess
import sys
import tomllib

root = sys.argv[1]

# Nested workspace roots this lint deliberately skips, relative to the repo
# root, each with its reason. Prefer fixing the workspace to adding a line.
EXCLUDED = {
    # Vendored upstream forks: these ARE the patch targets. Their own
    # `[workspace]` tables come from upstream and are never built as a
    # workspace here — the root consumes them as path crates.
    "third_party/naga": "upstream fork's own workspace; it is the patch target",
    "third_party/pp-rs": "upstream fork's own workspace; it is the patch target",
    "third_party/ser-write": "upstream fork's own workspace; it is the patch target",
    "third_party/ser-write-json": "upstream fork's own workspace; it is the patch target",
    # One-off experiments. Not built by CI, and a spike's lockfile records
    # what it measured on its day; a root fork added later should not force
    # an edit to a finished experiment.
    "spikes/classic-oom-allocator": "one-off spike, not built by CI",
    "spikes/glsl-compile-working-set": "one-off spike, not built by CI",
}

PRUNE = {"target", "node_modules"}


def load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def rel(path):
    return os.path.relpath(path, root)


# --- the root's patches -----------------------------------------------------
def norm_spec(spec, base):
    """A patch/dependency spec reduced to a comparable source identity."""
    if "path" in spec:
        return ("path", os.path.realpath(os.path.join(base, spec["path"])))
    if "git" in spec:
        url = spec["git"].rstrip("/")
        if url.endswith(".git"):
            url = url[:-4]
        ref = tuple((k, spec[k]) for k in ("branch", "tag", "rev") if k in spec)
        return ("git", url, ref)
    return ("registry",)


def show(src):
    if src[0] == "path":
        return "path %s" % rel(src[1])
    if src[0] == "git":
        return "git %s %s" % (src[1], " ".join("%s=%s" % kv for kv in src[2]))
    return "crates.io"


def patches_of(manifest):
    """{(source_key, crate_name): source identity} for a manifest's [patch]."""
    doc = load(manifest)
    base = os.path.dirname(manifest)
    out = {}
    for key, table in doc.get("patch", {}).items():
        for name, spec in table.items():
            if isinstance(spec, dict):
                out[(key, spec.get("package", name))] = norm_spec(spec, base)
    return out


root_patches = patches_of(os.path.join(root, "Cargo.toml"))
if not root_patches:
    sys.exit("nested patches: the root Cargo.toml has no [patch] entries — "
             "is this the repo root?")


def key_matches(patch_key, lock_source):
    """Does a Cargo.lock `source` belong to the patched source `patch_key`?"""
    if lock_source is None:
        return False
    if patch_key == "crates-io":
        return lock_source.startswith("registry+") or lock_source.startswith("sparse+")
    url = patch_key.rstrip("/")
    return lock_source.startswith("git+" + url)


# --- discovery --------------------------------------------------------------
# Tracked and untracked-but-not-ignored manifests: git already knows to skip
# target/ and friends, and asking it is ~30x faster than walking the tree.
listed = subprocess.check_output(
    ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard",
     "--", "Cargo.toml", "*/Cargo.toml"], cwd=root).decode().split("\0")
nested = []
for path in sorted(set(listed)):
    if not path or path == "Cargo.toml":
        continue
    if any(part in PRUNE or part.startswith(".") for part in path.split("/")):
        continue
    manifest = os.path.join(root, path)
    if os.path.isfile(manifest) and "workspace" in load(manifest):
        nested.append(os.path.dirname(manifest))


def excluded(ws_rel):
    return ws_rel in EXCLUDED


# Every exclusion must still name a real workspace, or it is dead weight.
fail = 0
for ws_rel in EXCLUDED:
    if not os.path.isfile(os.path.join(root, ws_rel, "Cargo.toml")):
        print("STALE EXCLUSION: %s is not a workspace root any more; delete "
              "its line from scripts/check-nested-patches.sh" % ws_rel)
        fail = 1


# --- the manifest walk ------------------------------------------------------
ws_root_cache = {}


def workspace_root_of(pkg_dir):
    """The workspace root whose [workspace.dependencies] a package inherits."""
    d = pkg_dir
    while True:
        if d in ws_root_cache:
            return ws_root_cache[d]
        m = os.path.join(d, "Cargo.toml")
        if os.path.isfile(m) and "workspace" in load(m):
            ws_root_cache[pkg_dir] = d
            return d
        parent = os.path.dirname(d)
        if parent == d:
            return None
        d = parent


def dep_tables(doc, is_member):
    kinds = ["dependencies", "build-dependencies"]
    if is_member:
        kinds.append("dev-dependencies")  # only members' dev-deps are built
    tables = [doc.get(k, {}) for k in kinds]
    for tgt in doc.get("target", {}).values():
        tables += [tgt.get(k, {}) for k in kinds]
    return tables


def walk(ws_dir):
    """Non-optional external deps reachable through path deps from the members.

    Returns ({(source_key, crate): chain}, {crate names of path packages}).
    """
    doc = load(os.path.join(ws_dir, "Cargo.toml"))
    members = set()
    for pat in doc["workspace"].get("members", []):
        for d in glob.glob(os.path.join(ws_dir, pat)):
            if os.path.isfile(os.path.join(d, "Cargo.toml")):
                members.add(os.path.realpath(d))
    if "package" in doc:
        members.add(os.path.realpath(ws_dir))

    reached, path_names, seen = {}, set(), set()
    stack = [(m, [rel(m)]) for m in sorted(members)]
    while stack:
        pkg_dir, chain = stack.pop()
        if pkg_dir in seen:
            continue
        seen.add(pkg_dir)
        pdoc = load(os.path.join(pkg_dir, "Cargo.toml"))
        name = pdoc.get("package", {}).get("name", rel(pkg_dir))
        path_names.add(name)
        chain = chain[:-1] + [name]
        inherit = {}
        wsr = workspace_root_of(pkg_dir)
        if wsr:
            inherit = load(os.path.join(wsr, "Cargo.toml")).get(
                "workspace", {}).get("dependencies", {})
        for table in dep_tables(pdoc, pkg_dir in members):
            for key, spec in table.items():
                spec = {"version": spec} if isinstance(spec, str) else dict(spec)
                base = pkg_dir
                if spec.get("workspace"):
                    ws_spec = inherit.get(key, {})
                    ws_spec = ({"version": ws_spec} if isinstance(ws_spec, str)
                               else dict(ws_spec))
                    ws_spec.update({k: v for k, v in spec.items()
                                    if k not in ("workspace",)})
                    spec, base = ws_spec, wsr
                if spec.get("optional"):
                    continue
                dep = spec.get("package", key)
                if "path" in spec:
                    d = os.path.realpath(os.path.join(base, spec["path"]))
                    stack.append((d, chain + [dep]))
                    continue
                skey = spec["git"].rstrip("/") if "git" in spec else "crates-io"
                reached.setdefault((skey, dep), chain + [dep])
    return reached, path_names


# --- the check --------------------------------------------------------------
checked = []
for ws_dir in nested:
    ws_rel = rel(ws_dir)
    if excluded(ws_rel):
        continue
    checked.append(ws_rel)
    manifest = os.path.join(ws_dir, "Cargo.toml")
    lockfile = os.path.join(ws_dir, "Cargo.lock")
    if not os.path.isfile(lockfile):
        print("NO LOCKFILE: %s has no committed Cargo.lock; this lint reads "
              "it to see what the workspace resolves" % ws_rel)
        fail = 1
        continue
    lock = load(lockfile).get("package", [])
    ws_patches = patches_of(manifest)
    reached, path_names = walk(ws_dir)

    for (pkey, crate), want in sorted(root_patches.items()):
        entries = [p for p in lock if p["name"] == crate]
        by_walk = reached.get((pkey, crate))
        foreign = [p for p in entries if key_matches(pkey, p.get("source"))]
        pathy = [p for p in entries if p.get("source") is None]
        if not (by_walk or foreign or pathy):
            continue  # not reached: nothing to repeat
        have = ws_patches.get((pkey, crate))
        where = ("reached via %s" % " -> ".join(by_walk) if by_walk
                 else "in %s" % rel(lockfile))
        hint = ("    root: [patch.%s] %s -> %s"
                % (pkey if "/" not in pkey else '"%s"' % pkey, crate, show(want)))

        if have is not None and have != want:
            print("PATCH MISMATCH: %s patches %s to %s, root patches it to %s"
                  % (ws_rel, crate, show(have), show(want)))
            print("    (%s)" % where)
            fail = 1
            continue
        if have is None and (by_walk or foreign):
            print("MISSING PATCH: %s reaches %s but %s has no [patch.%s] entry "
                  "for it, so it resolves from %s instead of the root's %s"
                  % (ws_rel, crate, rel(manifest), pkey,
                     foreign[0]["source"] if foreign else pkey, show(want)))
            print("    (%s)" % where)
            print(hint)
            fail = 1
            continue
        if have is not None and foreign:
            print("STALE LOCK: %s patches %s to %s, but %s still resolves it "
                  "from %s — run cargo in %s to re-lock"
                  % (ws_rel, crate, show(want), rel(lockfile),
                     foreign[0]["source"], ws_rel))
            fail = 1
            continue
        if have is None and pathy and crate not in path_names:
            print("STALE LOCK: %s resolves %s to a path, but nothing in the %s "
                  "manifests puts it there (no [patch.%s] entry, no path "
                  "dependency) — the root patches it to %s; add the patch, "
                  "or the next build re-resolves it from %s"
                  % (rel(lockfile), crate, ws_rel, pkey, show(want), pkey))
            print(hint)
            fail = 1

if fail:
    print()
    print("A [patch] table applies only to its own workspace. Repeat the root's")
    print("entry in the nested workspace's Cargo.toml (see")
    print("docs/debt/nested-workspaces-miss-root-patches.md), or, if that")
    print("workspace should not be checked, add it to EXCLUDED in")
    print("scripts/check-nested-patches.sh with its reason.")
    sys.exit(1)

print("nested patches: ok (%d root patch(es); checked %s)"
      % (len(root_patches), ", ".join(checked) or "no nested workspaces"))
PY
