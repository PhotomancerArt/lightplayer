# Phase 9 Design: Cleanup and validation

Plan reference: `2026-03-17-glsl-wasm-part-iii.md` Phase 9

## Goals

1. Run `cargo build` (full workspace)
2. Run `cargo test` (full workspace)
3. Run `cargo +nightly fmt`
4. Fix any warnings
5. Verify `just build-fw-esp32` still works
6. Update READMEs with current state

---

## 1. Build and test

**Commands:**

```bash
cargo build
cargo test
cargo build -p lps-wasm
cargo test -p lps-wasm
cargo test -p lps-filetests
```

**Fix:** Any compilation errors or test failures.

---

## 2. Formatting

**Command:** `cargo +nightly fmt` (or `cargo fmt` if toolchain supports it)

**Check:** `cargo +nightly fmt --check` to verify. No diff.

---

## 3. Warnings

**Policy:** No warnings. Run `cargo build 2>&1 | grep warning` and fix each.

**Common:** Unused imports, dead code, deprecated. Address all.

---

## 4. build-fw-esp32

**Command:** `just build-fw-esp32`

**Purpose:** Ensures lps changes don't break the ESP32 firmware build. If the workspace has this
target, it must succeed.

---

## 5. README updates

**lps-wasm/README.md:**

- Document supported features (scalars, vectors, control flow, builtins, LPFX)
- Builtin import mechanism (module "builtins", function names)
- Vector representation (multi-local)
- Q32 mode
- Limitations (no matrices, no arrays, no structs)

**lps-filetests/README.md:**

- Current wasm.q32 pass count
- Annotation patterns (@unimplemented(backend=wasm))
- How to run with --target wasm.q32

**lp-shader/README.md:**

- Crate table: update lps-wasm description
- Any new crates or changes

---

## 6. Validation script

**From plan:** Full validation command block:

```bash
cargo build
cargo test
cargo build -p lps-wasm
cargo test -p lps-wasm
cargo test -p lps-filetests
scripts/glsl-filetests.sh
cargo +nightly fmt --check
just build-fw-esp32
```

**Target:** All pass. wasm.q32 pass count in hundreds. Rainbow shader compiles and runs.

---

## File change summary

| File                                  | Changes                          |
|---------------------------------------|----------------------------------|
| Various                               | Fix warnings, formatting         |
| lp-shader/lps-wasm/README.md      | Document features, mechanism     |
| lp-shader/lps-filetests/README.md | Pass counts, annotation patterns |
| lp-shader/README.md                   | Crate table if needed            |
