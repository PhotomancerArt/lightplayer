#!/usr/bin/env python3
"""Migrate filetest annotations: remove // target, convert [expect-fail] to @unimplemented()."""

import os
import re
import sys

FILETESTS_DIR = os.path.join(os.path.dirname(__file__), "..", "lps-filetests", "filetests")

# Files known to work on wasm (none - wasm backend has many gaps).
# Previously listed files were incorrect; use @unimplemented or @unsupported as needed.
WASM_OK_FILES = set()


def migrate_file(path: str, content: str, rel_path: str) -> tuple[str, bool]:
    """Migrate a single file. Returns (new_content, changed)."""
    lines = content.splitlines(keepends=True)
    new_lines = []
    changed = False
    i = 0

    file_annotation_added = False
    needs_wasm_annotation = (
        not rel_path.endswith(".gen.glsl")
        and rel_path not in WASM_OK_FILES
        and not rel_path.startswith("wasm/")  # will be moved
    )

    while i < len(lines):
        line = lines[i]

        # Remove // target lines
        if re.match(r"^\s*//\s*target\s+\S+", line):
            changed = True
            i += 1
            continue

        # Convert [expect-fail] on run lines to @unimplemented() on preceding line
        if "// run:" in line or "// #run:" in line:
            if "[expect-fail]" in line:
                # Add @unimplemented() before this line
                indent = line[: len(line) - len(line.lstrip())]
                new_lines.append(f"{indent}// @unimplemented()\n")
                new_line = re.sub(r"\s*\[expect-fail\]\s*", " ", line).rstrip() + "\n"
                new_lines.append(new_line)
                changed = True
                i += 1
                continue

        # Add file-level @unimplemented(backend=wasm) after // test run
        if (
            not file_annotation_added
            and needs_wasm_annotation
            and re.match(r"^\s*//\s*test\s+run\s*$", line.strip())
        ):
            new_lines.append(line)
            new_lines.append("// @unimplemented(backend=wasm)\n")
            file_annotation_added = True
            changed = True
            i += 1
            continue

        new_lines.append(line)
        i += 1

    return "".join(new_lines), changed


def main():
    total = 0
    migrated = 0
    for root, _dirs, files in os.walk(FILETESTS_DIR):
        for f in files:
            if not f.endswith(".glsl"):
                continue
            path = os.path.join(root, f)
            rel = os.path.relpath(path, FILETESTS_DIR)
            total += 1
            with open(path, "r") as fp:
                content = fp.read()
            new_content, changed = migrate_file(path, content, rel)
            if changed:
                with open(path, "w") as fp:
                    fp.write(new_content)
                migrated += 1
                print(rel)
    print(f"\nMigrated {migrated}/{total} files", file=sys.stderr)


if __name__ == "__main__":
    main()
