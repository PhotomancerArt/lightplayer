#!/usr/bin/env python3
"""CI's own firmware images, for running the chip emulator suites locally
without building firmware. See docs/ci-images.md.

  scripts/ci/ci-images.py pack  <chip> <out-dir>          # CI: collect + manifest
  scripts/ci/ci-images.py fetch [main|<pr>|<sha>|<run-id>] [chip...] [--force]
  scripts/ci/ci-images.py env   <chip>                    # `export` lines, for `eval`
  scripts/ci/ci-images.py with  <chip|all|""> -- <cmd...> # exec cmd with the env
  scripts/ci/ci-images.py status [chip...]                # what LP_CI_IMAGES holds

Chips: esp32c6, esp32v3, esp32s3.

**The one rule this script exists to hold:** an image built from firmware
sources other than the ones in this checkout is a stale image, and a suite run
against it proves nothing about this tree. `pack` records the git tree id of
every path that feeds the image (`SOURCE_PATHS`); `env`/`with`/`fetch` compare
them with HEAD and with the working tree, and refuse on any difference unless
`LP_CI_IMAGES_FORCE=1` (or `fetch --force`). Pinned reference images
(`emu-ref/<commit>-<slug>`) are keyed by their commit and are exempt.

`with` and `env` do nothing at all when `LP_CI_IMAGES` is unset: the recipes
that call them build their own images exactly as before.
"""

from __future__ import annotations

import datetime
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ".github/workflows/pre-merge.yml"
CHIPS = ("esp32c6", "esp32v3", "esp32s3")
ALIASES = {"c6": "esp32c6", "v3": "esp32v3", "esp32": "esp32v3", "s3": "esp32s3"}

# Everything a chip firmware image is built from. The link closure of
# `lp-fw/fw-esp32{c6,v3,s3}` (their path dependencies, checked 2026-09-25: no
# chip firmware depends on `lp-emu/`, `lp-cli` or any `lp-app/` crate but
# `lpa-server`), plus the lockfile and the toolchain pin. Deliberately NOT
# `lp-emu/`: the emulator and its tests are what you change while testing
# against CI's images, and they do not feed the image. Nor the justfile or
# scripts/: the build recipes' flags are fixed profiles, and the merged image
# is espflash's (pinned) output — if one of those changes, re-run CI.
SOURCE_PATHS = (
    "lp-fw",
    "lp-core",
    "lp-base",
    "lp-shader",
    "lp-gfx",
    "lp-riscv",
    "lp-xt",
    "lp-app/lpa-server",
    "third_party",
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
)

# The heap-budget gate's own variable for each chip's shipped ELF
# (`scripts/heap-budget-check.sh`, `chip_facts`), pointed at the same file the
# boot suite reads, so `heap-budget-check-chips*` / `bless-chips` compose.
HEAP_ALIAS = {
    "esp32c6": ("LP_EMU_C6_ELF_ESP32C6_SERVER_RADIO", "tree/ESP32C6_SERVER_RADIO/fw-esp32c6"),
    "esp32v3": ("LP_EMU_V3_ELF_ESP32_SERVER_FLOAT_F32", "@LP_EMU_ESP32V3_ELF"),
    "esp32s3": ("LP_EMU_ESP32S3_ELF", "@LP_EMU_ESP32S3_ELF"),
}

# What each Xtensa image variable is, for the manifest (the boot recipes in
# the justfile are the authority; this only labels what they built).
XT_META = {
    "LP_EMU_ESP32V3_ELF": ("esp32,server,float-f32 (defaults)", "release-esp32v3"),
    "LP_EMU_ESP32V3_TEST_RMT_ELF": ("defaults + test_rmt", "release-esp32v3"),
    "LP_EMU_ESP32V3_FRAME_DUMP_ELF": ("defaults + frame-dump", "release-esp32v3"),
    "LP_EMU_ESP32V3_MERGED": ("espflash save-image --merge of LP_EMU_ESP32V3_ELF", ""),
    "LP_EMU_ESP32V3_REF_ELF": ("esp32,server,float-f32 at the pinned commit", "release-esp32v3"),
    "LP_EMU_ESP32V3_REF_MERGED": ("espflash save-image --merge of LP_EMU_ESP32V3_REF_ELF", ""),
    "LP_EMU_ESP32S3_ELF": ("esp32s3,server,float-f32 (defaults)", "release-esp32s3"),
    "LP_EMU_ESP32S3_MERGED": ("espflash save-image --merge of LP_EMU_ESP32S3_ELF", ""),
}


def die(msg: str, code: int = 1) -> "NoReturn":  # noqa: F821
    print(f"ci-images: {msg}", file=sys.stderr)
    sys.exit(code)


def note(msg: str) -> None:
    print(f"ci-images: {msg}", file=sys.stderr)


def git(*args: str, check: bool = True) -> str:
    out = subprocess.run(["git", "-C", str(ROOT), *args], capture_output=True, text=True)
    if check and out.returncode != 0:
        die(f"git {' '.join(args)} failed: {out.stderr.strip()}")
    return out.stdout


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


def chip_name(arg: str) -> str | None:
    arg = ALIASES.get(arg, arg)
    return arg if arg in CHIPS else None


# ── sources ─────────────────────────────────────────────────────────────────


def source_trees(rev: str = "HEAD") -> dict[str, str]:
    """The git object id of every SOURCE_PATH at `rev`."""
    trees = {}
    for path in SOURCE_PATHS:
        oid = git("rev-parse", "--verify", "--quiet", f"{rev}:{path}", check=False).strip()
        trees[path] = oid or "(absent)"
    return trees


def digest(trees: dict[str, str]) -> str:
    text = "".join(f"{k} {trees[k]}\n" for k in sorted(trees))
    return hashlib.sha256(text.encode()).hexdigest()


def staleness(manifest: dict) -> list[str]:
    """Why these images are not this checkout's, or [] when they are."""
    why = []
    theirs = manifest["sources"]["trees"]
    ours = source_trees()
    for path in SOURCE_PATHS:
        if theirs.get(path) != ours.get(path):
            why.append(f"{path}: images built from {theirs.get(path, '?')[:12]}, HEAD has {ours[path][:12]}")
    dirty = git("status", "--porcelain", "--untracked-files=normal", "--", *SOURCE_PATHS).strip()
    if dirty:
        lines = dirty.splitlines()
        why.append(
            f"uncommitted changes under the firmware sources ({len(lines)}): "
            + ", ".join(line[3:] for line in lines[:6])
            + (" …" if len(lines) > 6 else "")
        )
    return why


# ── pack (CI) ───────────────────────────────────────────────────────────────


def read_images_env(path: Path) -> dict[str, str]:
    """`export -p` / `declare -x NAME="value"` lines → {NAME: value}."""
    out = {}
    for line in path.read_text().splitlines():
        m = re.match(r'^(?:declare -x|export) (LP_[A-Z0-9_]+)=(.*)$', line)
        if m:
            value = m.group(2)
            try:
                value = shlex.split(value)[0] if value else ""
            except ValueError:
                value = value.strip('"')
            out[m.group(1)] = value
    return out


def pack(chip: str, out: Path) -> None:
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    files: list[dict] = []
    env: dict[str, str] = {}

    def add(src: Path, rel: str, kind: str, **meta) -> None:
        dst = out / rel
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dst)
        files.append({"path": rel, "sha256": sha256(dst), "bytes": dst.stat().st_size, "kind": kind, **meta})

    def add_ref_dir(ref_dir: Path, binary: str) -> None:
        name = ref_dir.name
        commit = name.split("-", 1)[0]
        for f in (binary, "merged.bin", "partitions.csv", "SHA256SUMS", "PROVENANCE"):
            if (ref_dir / f).is_file() and not any(x["path"] == f"emu-ref/{name}/{f}" for x in files):
                prov = (ref_dir / "PROVENANCE").read_text().strip() if (ref_dir / "PROVENANCE").is_file() else ""
                add(ref_dir / f, f"emu-ref/{name}/{f}", "pinned", commit=commit, provenance=prov)

    target = ROOT / "target"
    if chip == "esp32c6":
        # Tree images: `lp_emu_esp32c6::test_support`'s keyed copies,
        # `target/lp-emu-c6/<SLUG>-<16-hex source key>/fw-esp32c6`. CI builds
        # one tree, so one key; two would mean something else wrote here.
        by_slug: dict[str, Path] = {}
        for elf in sorted((target / "lp-emu-c6").glob("*/fw-esp32c6")):
            m = re.match(r"^(.+)-([0-9a-f]{16})$", elf.parent.name)
            if not m:
                continue
            if m.group(1) in by_slug:
                die(f"two source keys for {m.group(1)} under target/lp-emu-c6: refusing to guess")
            by_slug[m.group(1)] = elf
        for slug, elf in sorted(by_slug.items()):
            add(elf, f"tree/{slug}/fw-esp32c6", "tree", slug=slug, profile="release-esp32")
        for ref in sorted((target / "emu-ref").glob("*")):
            if ref.name.startswith("wt-") or not (ref / "fw-esp32c6").is_file():
                continue
            add_ref_dir(ref, "fw-esp32c6")
        env["LP_EMU_C6_IMAGE_DIR"] = "."
    else:
        dir_name = {"esp32v3": "lp-emu-esp32v3", "esp32s3": "lp-emu-esp32s3"}[chip]
        images_env = target / dir_name / "images.env"
        if not images_env.is_file():
            die(f"{images_env} is missing: the boot recipe writes it after building the images")
        for var, value in sorted(read_images_env(images_env).items()):
            src = Path(value)
            if not src.is_file():
                note(f"{var}={value} is not a file; not packed")
                continue
            parts = src.resolve().parts
            if "emu-ref" in parts:
                ref_dir = src.resolve().parent
                add_ref_dir(ref_dir, src.name)
                env[var] = f"emu-ref/{ref_dir.name}/{src.name}"
            else:
                features, profile = XT_META.get(var, ("", ""))
                add(src, src.name, "tree", features=features, profile=profile)
                env[var] = src.name
    if not files:
        die(f"nothing to pack for {chip}")

    alias, rel = HEAP_ALIAS[chip]
    env.setdefault(alias, env[rel[1:]] if rel.startswith("@") else rel)

    commit = git("rev-parse", "HEAD").strip()
    trees = source_trees()
    espflash = shutil.which("espflash")
    manifest = {
        "schema": 1,
        "chip": chip,
        "commit": commit,
        "head_sha": os.environ.get("LP_CI_HEAD_SHA") or commit,
        "run_id": os.environ.get("GITHUB_RUN_ID", ""),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""),
        "job": os.environ.get("GITHUB_JOB", ""),
        "runner": f"{os.environ.get('ImageOS', '?')} {os.environ.get('RUNNER_ARCH', '?')}",
        "created": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "espflash": subprocess.run([espflash, "--version"], capture_output=True, text=True).stdout.strip()
        if espflash
        else "",
        "sources": {"paths": list(SOURCE_PATHS), "trees": trees, "digest": digest(trees)},
        "env": env,
        "files": files,
    }
    (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    total = sum(f["bytes"] for f in files)
    print(f"ci-images: packed {len(files)} files, {total / 1e6:.1f} MB uncompressed, for {chip} at {commit[:12]}")
    for f in files:
        print(f"  {f['bytes']:>10}  {f['sha256'][:16]}  {f['kind']:<6}  {f['path']}")


# ── local: load, verify, export ─────────────────────────────────────────────


def images_root() -> Path | None:
    value = os.environ.get("LP_CI_IMAGES", "").strip()
    if not value:
        return None
    root = Path(value)
    if not root.is_absolute():
        root = (Path.cwd() / root).resolve()
    if not root.is_dir():
        die(f"LP_CI_IMAGES={value} is not a directory (fetch one with `just fetch-ci-images`)")
    return root


def load(chip_dir: Path, verify: bool = True) -> dict:
    manifest_path = chip_dir / "manifest.json"
    if not manifest_path.is_file():
        die(f"{chip_dir} holds no manifest.json — not a fetched CI image set")
    manifest = json.loads(manifest_path.read_text())
    if verify:
        for f in manifest["files"]:
            path = chip_dir / f["path"]
            if not path.is_file():
                die(f"{path} is missing (listed in {manifest_path})")
            got = sha256(path)
            if got != f["sha256"]:
                die(f"{path}: sha256 {got} does not match the manifest's {f['sha256']} — refetch it")
    return manifest


def check_fresh(chip: str, manifest: dict, force: bool) -> bool:
    """Say whether the images are this checkout's; die if not, unless forced."""
    if not any(f["kind"] == "tree" for f in manifest["files"]):
        return True
    why = staleness(manifest)
    if not why:
        return True
    head = git("rev-parse", "--short=12", "HEAD").strip()
    msg = (
        f"{chip}: CI's images were built from different firmware sources than this checkout "
        f"(images: {manifest['commit'][:12]}, run {manifest.get('run_id') or '?'}; HEAD: {head}).\n  "
        + "\n  ".join(why)
    )
    if force:
        note(msg + "\n  FORCED (LP_CI_IMAGES_FORCE=1 / --force): these results are about THOSE sources, not yours.")
        return False
    die(
        msg
        + "\n  A suite run against these images would test someone else's firmware. Fetch images for "
        "this HEAD (push, let CI build, `just fetch-ci-images <pr>`), rebase onto the images' commit, "
        "or set LP_CI_IMAGES_FORCE=1 if you know the difference cannot matter."
    )


def chip_env(chip: str) -> dict[str, str]:
    root = images_root()
    assert root is not None
    chip_dir = root / chip
    if not chip_dir.is_dir():
        die(f"LP_CI_IMAGES={root} has no {chip}/ — fetch it: just fetch-ci-images <run> {chip}")
    manifest = load(chip_dir)
    check_fresh(chip, manifest, os.environ.get("LP_CI_IMAGES_FORCE") == "1")
    env = {var: str((chip_dir / rel).resolve()) for var, rel in manifest["env"].items()}
    note(
        f"{chip}: {len(manifest['files'])} images from CI run {manifest.get('run_id') or '?'} "
        f"(commit {manifest['commit'][:12]}) — no firmware build"
    )
    return env


def expand_chips(arg: str) -> list[str]:
    if arg in ("", "all"):
        return list(CHIPS)
    chips = []
    for a in arg.split(","):
        c = chip_name(a)
        if not c:
            die(f"unknown chip '{a}' (known: {', '.join(CHIPS)})")
        chips.append(c)
    return chips


def cmd_env(args: list[str]) -> None:
    if len(args) != 1:
        die("usage: ci-images.py env <chip>", 2)
    if images_root() is None:
        return
    for chip in expand_chips(args[0]):
        for var, value in chip_env(chip).items():
            print(f"export {var}={shlex.quote(value)}")


def cmd_with(args: list[str]) -> None:
    if "--" not in args:
        die("usage: ci-images.py with <chip|all> -- <cmd...>", 2)
    i = args.index("--")
    spec, cmd = args[:i], args[i + 1 :]
    if not cmd:
        die("with: no command", 2)
    env = dict(os.environ)
    if images_root() is not None:
        for chip in expand_chips(spec[0] if spec else ""):
            env.update(chip_env(chip))
    os.execvpe(cmd[0], cmd, env)


def cmd_status(args: list[str]) -> None:
    root = images_root()
    if root is None:
        print("LP_CI_IMAGES is not set: every chip recipe builds its own firmware.")
        return
    for chip in [chip_name(a) or a for a in args] or list(CHIPS):
        d = root / chip
        if not d.is_dir():
            print(f"{chip}: not fetched")
            continue
        m = load(d)
        why = staleness(m) if any(f["kind"] == "tree" for f in m["files"]) else []
        print(
            f"{chip}: run {m.get('run_id')} commit {m['commit'][:12]}, {len(m['files'])} files — "
            + ("MATCHES this checkout's firmware sources" if not why else "STALE: " + "; ".join(why))
        )


# ── fetch ───────────────────────────────────────────────────────────────────


def gh_json(*args: str):
    out = subprocess.run(["gh", *args], capture_output=True, text=True, cwd=ROOT)
    if out.returncode != 0:
        die(f"gh {' '.join(args)} failed: {out.stderr.strip()}")
    return json.loads(out.stdout or "null")


def repo() -> str:
    return gh_json("repo", "view", "--json", "nameWithOwner")["nameWithOwner"]


def runs(query: str) -> list[dict]:
    data = gh_json("api", f"repos/{repo()}/actions/workflows/pre-merge.yml/runs?per_page=30&{query}")
    return data.get("workflow_runs", [])


def artifacts(run_id: int) -> dict[str, dict]:
    data = gh_json("api", f"repos/{repo()}/actions/runs/{run_id}/artifacts?per_page=100")
    return {a["name"]: a for a in data.get("artifacts", []) if not a.get("expired")}


def pick_run(target: str, chips: list[str]) -> tuple[int, dict[str, dict]]:
    wanted = {f"ci-images-{c}" for c in chips}

    def first_with_artifacts(candidates: list[dict], what: str) -> tuple[int, dict[str, dict]]:
        best = None
        for r in candidates:
            arts = artifacts(r["id"])
            have = wanted & arts.keys()
            if have == wanted:
                return r["id"], arts
            if have and best is None:
                best = (r["id"], arts)
        if best:
            return best
        die(f"no CI run for {what} has ci-images-* artifacts for {', '.join(chips)} (the emulator "
            "jobs are path-gated, and artifacts expire after 7 days)")

    if target in ("", "main"):
        return first_with_artifacts(runs("branch=main&event=push&status=success"), "main (green)")
    if re.fullmatch(r"\d{1,6}", target):
        pr = gh_json("pr", "view", target, "--json", "headRefOid,headRefName")
        cands = runs(f"head_sha={pr['headRefOid']}")
        cands += [r for r in runs(f"branch={pr['headRefName']}&event=pull_request") if r not in cands]
        return first_with_artifacts(cands, f"PR #{target}")
    if re.fullmatch(r"\d{7,}", target):
        return int(target), artifacts(int(target))
    if re.fullmatch(r"[0-9a-f]{7,40}", target):
        full = git("rev-parse", "--verify", "--quiet", f"{target}^{{commit}}", check=False).strip() or target
        if len(full) != 40:
            die(f"{target} is not a commit in this checkout; pass the full 40-character sha")
        return first_with_artifacts(runs(f"head_sha={full}"), f"commit {full[:12]}")
    die(f"cannot read '{target}' as main, a PR number, a run id or a commit sha", 2)


def cmd_fetch(args: list[str]) -> None:
    force = "--force" in args
    args = [a for a in args if a != "--force"]
    target, chips = "", []
    for a in args:
        c = chip_name(a)
        if c:
            chips.append(c)
        elif not target:
            target = a
        else:
            die(f"unexpected argument '{a}'", 2)
    chips = chips or list(CHIPS)
    run_id, arts = pick_run(target, chips)
    note(f"run {run_id}: https://github.com/{repo()}/actions/runs/{run_id}")
    stale_any = False
    dest_root = None
    for chip in chips:
        name = f"ci-images-{chip}"
        if name not in arts:
            note(f"{chip}: run {run_id} has no {name} artifact (job skipped by its path filter, or expired)")
            continue
        scratch = ROOT / "target" / "ci-images"
        scratch.mkdir(parents=True, exist_ok=True)
        tmp = tempfile.mkdtemp(prefix=f".{chip}-", dir=scratch)
        try:
            out = subprocess.run(["gh", "run", "download", str(run_id), "-n", name, "-D", tmp], cwd=ROOT)
            if out.returncode != 0:
                die(f"gh run download {run_id} -n {name} failed")
            manifest = load(Path(tmp))  # verifies every sha256
            dest_root = scratch / manifest["commit"][:12]
            dest = dest_root / chip
            if dest.exists():
                shutil.rmtree(dest)
            dest_root.mkdir(parents=True, exist_ok=True)
            shutil.move(tmp, dest)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        size = sum(f["bytes"] for f in manifest["files"])
        note(f"{chip}: {len(manifest['files'])} files, {size / 1e6:.1f} MB, sha256 verified → {dest}")
        why = staleness(manifest) if any(f["kind"] == "tree" for f in manifest["files"]) else []
        if why:
            stale_any = True
            head = git("rev-parse", "--short=12", "HEAD").strip()
            note(
                f"{chip}: STALE — built from different firmware sources than this checkout "
                f"(images: {manifest['commit'][:12]}; HEAD: {head}):\n  " + "\n  ".join(why)
            )
    if dest_root is None:
        die("nothing fetched")
    if stale_any and not force:
        die(
            "REFUSING: the images above were not built from this checkout's firmware sources, so a "
            "suite run against them would test someone else's firmware (the recipes refuse them too). "
            "Fetch the run for this HEAD, merge/rebase onto the images' commit, or re-run with "
            "--force and then set LP_CI_IMAGES_FORCE=1 to use them anyway."
        )
    print(f"\nexport LP_CI_IMAGES={dest_root}")
    if stale_any:
        print("export LP_CI_IMAGES_FORCE=1   # --force: these images are NOT this checkout's firmware")
    print(
        "then e.g.  just test-emu-esp32s3-boot   just test-emu-esp32v3-boot   just test-emu-c6-boot\n"
        "           just bless-chips esp32s3     (see docs/ci-images.md)"
    )


def main() -> None:
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__)
        return
    cmd, args = sys.argv[1], sys.argv[2:]
    if cmd == "pack":
        if len(args) != 2 or not chip_name(args[0]):
            die("usage: ci-images.py pack <chip> <out-dir>", 2)
        pack(chip_name(args[0]), Path(args[1]).resolve())
    elif cmd == "fetch":
        cmd_fetch(args)
    elif cmd == "env":
        cmd_env(args)
    elif cmd == "with":
        cmd_with(args)
    elif cmd == "status":
        cmd_status(args)
    else:
        die(f"unknown command '{cmd}'", 2)


if __name__ == "__main__":
    main()
