#!/usr/bin/env python3
"""Every `tw:` utility in the Studio web markup must generate a CSS rule.

Tailwind v4 compiles the `@theme` in `lp-app/lpa-studio-web/tailwind.css`
into utilities: `--color-card` becomes `tw:bg-card`, `tw:text-card`, and so
on. A utility built on a token the theme does NOT define (`tw:bg-panel`,
`tw:text-error-foreground`) is not an error anywhere: rustc sees a string,
Tailwind's scanner just skips the unknown class, and the element falls
through to transparent or inherited. The device roster card shipped with a
transparent background that way (its `tw:bg-panel` had never existed).

This gate scans `src/**/*.{rs,html}` for every literal `tw:...` token,
generates the stylesheet with the same pinned standalone Tailwind CLI that
`dx` uses (found in dx's tool cache, `$TAILWINDCSS`, `PATH`, or downloaded
once from the pinned release), and fails on any token with no rule.

Tokens that are not classes are skipped: doc-comment families (`tw:bg-*`),
format-string prefixes (`tw:bg-{tone}` scans as `tw:bg-`), and anything
whose class body is empty. A doc comment that names a real class is checked
like markup, which is the point: a comment naming a class that does not
exist is as misleading as markup using one.

Usage: check-tw-utilities.py [--css PATH] [--verbose]
  --css PATH  check against an already-generated stylesheet instead of
              generating one (e.g. lp-app/lpa-studio-web/assets/tailwind.css
              after a `dx build`).
"""

from __future__ import annotations

import argparse
import glob
import os
import platform
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CRATE = REPO / "lp-app" / "lpa-studio-web"
THEME = CRATE / "tailwind.css"

# The version dx 0.7.x pins (see `~/.dx/tools/tailwindcss-v<ver>` after any
# `dx serve`); keep in step with the dioxus-cli pin in pre-merge.yml.
TAILWIND_VERSION = "4.1.5"
RELEASE_URL = (
    "https://github.com/tailwindlabs/tailwindcss/releases/download/"
    f"v{TAILWIND_VERSION}/tailwindcss-{{platform}}"
)
DX_TOOL_DIR = Path.home() / ".dx" / "tools" / f"tailwindcss-v{TAILWIND_VERSION}"

# A literal `tw:` token: bracketed arbitrary values may hold anything but
# whitespace/`]`; outside brackets the token ends at whitespace, quotes,
# braces (format-string holes) or backticks (doc-comment code spans).
TOKEN_RE = re.compile(r"tw:(?:\[[^\]\s]*\]|[^\s\"'`\[\]{}])+")
# A generated selector: `.tw\:h-1\.5`, `.tw\:w-\[35\%\]`, `.tw\:@min-...`.
SELECTOR_RE = re.compile(r"\.(tw\\:(?:\\.|[A-Za-z0-9_-])+)")


def release_platform() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    arch = {"x86_64": "x64", "amd64": "x64", "arm64": "arm64", "aarch64": "arm64"}.get(machine)
    if system == "darwin" and arch:
        return f"macos-{arch}"
    if system == "linux" and arch:
        return f"linux-{arch}"
    sys.exit(f"check-tw-utilities: no pinned Tailwind CLI build for {system}/{machine}; set TAILWINDCSS")


def find_cli() -> Path:
    env = os.environ.get("TAILWINDCSS")
    if env:
        return Path(env)
    pinned = DX_TOOL_DIR / "tailwindcss"
    if pinned.is_file():
        return pinned
    # Any other dx-cached version beats a download: the theme is plain v4.
    for cached in sorted(glob.glob(str(Path.home() / ".dx" / "tools" / "tailwindcss-v*" / "tailwindcss"))):
        return Path(cached)
    on_path = shutil.which("tailwindcss")
    if on_path:
        return Path(on_path)
    url = RELEASE_URL.format(platform=release_platform())
    print(f"check-tw-utilities: downloading pinned Tailwind CLI v{TAILWIND_VERSION} -> {pinned}", file=sys.stderr)
    DX_TOOL_DIR.mkdir(parents=True, exist_ok=True)
    tmp = pinned.with_suffix(".part")
    urllib.request.urlretrieve(url, tmp)
    tmp.chmod(tmp.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    tmp.replace(pinned)
    return pinned


def generate_css() -> str:
    cli = find_cli()
    with tempfile.TemporaryDirectory() as tmp:
        out = Path(tmp) / "tailwind.css"
        subprocess.run(
            [str(cli), "-i", str(THEME), "-o", str(out)],
            cwd=CRATE,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return out.read_text()


def generated_classes(css: str) -> set[str]:
    return {re.sub(r"\\(.)", r"\1", m) for m in SELECTOR_RE.findall(css)}


def is_class_token(token: str) -> bool:
    if "*" in token:
        return False  # a family named in prose: `tw:bg-*`
    body = token[len("tw:"):]
    if not body or body.endswith(("-", ":")):
        return False  # a format-string prefix: `tw:bg-{tone}`, `tw:hover:{x}`
    return True


def source_tokens() -> dict[str, list[str]]:
    uses: dict[str, list[str]] = {}
    files = sorted(CRATE.glob("src/**/*.rs")) + sorted(CRATE.glob("src/**/*.html"))
    for path in files:
        text = path.read_text()
        for match in TOKEN_RE.finditer(text):
            token = match.group(0)
            # Prose punctuation after a class in a comment: "uses `tw:bg-card`."
            # is caught by the backtick rule; "(tw:bg-card)." is not.
            while token and token[-1] in ".,;)" and token.count("(") < token.count(")") + (token[-1] != ")"):
                token = token[:-1]
            if not is_class_token(token):
                continue
            line = text.count("\n", 0, match.start()) + 1
            uses.setdefault(token, []).append(f"{path.relative_to(REPO)}:{line}")
    return uses


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--css", type=Path, help="check against this generated stylesheet instead of generating one")
    parser.add_argument("--verbose", action="store_true", help="list every use of every missing token")
    args = parser.parse_args()

    css = args.css.read_text() if args.css else generate_css()
    present = generated_classes(css)
    uses = source_tokens()
    missing = {token: sites for token, sites in uses.items() if token not in present}

    print(f"check-tw-utilities: {len(uses)} distinct tw: tokens, {len(present)} generated classes, {len(missing)} missing")
    if not missing:
        return 0
    for token, sites in sorted(missing.items()):
        shown = sites if args.verbose else sites[:3]
        more = "" if len(sites) <= len(shown) else f" (+{len(sites) - len(shown)} more)"
        print(f"  {token}\n    {', '.join(shown)}{more}")
    print(
        "\nEach token above generates NO css rule (its theme token is not in "
        "lp-app/lpa-studio-web/tailwind.css, or the utility is misspelled). "
        "Map it to an existing token or add the token to the @theme block.",
        file=sys.stderr,
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
