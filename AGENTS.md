# AGENTS.md — LightPlayer Agent Instructions

## What LightPlayer Is

LightPlayer is an **embedded GLSL JIT shader execution system**. GLSL shaders
are compiled to native RISC-V machine code **on device** (ESP32-C6) at runtime,
then executed directly from RAM. This is analogous to MicroPython or Espruino,
but for GLSL and LED control.

The reference target is **ESP32-C6** (`riscv32imac-unknown-none-elf`), a
bare-metal `no_std` RISC-V microcontroller. The compiler backend runs **on the
microcontroller itself** — not on a host, not as a cross-compilation step.

## The Non-Negotiable Rule

**The on-device GLSL JIT compiler is the core product.** Every decision must
preserve the ability to compile and execute GLSL shaders on ESP32 at runtime.

If you are about to:

- Add `#[cfg(feature = "std")]` to any part of the compile/execute path
- Return a stub or error from `compile_shader` on embedded targets
- Make the compiler an opt-in feature on `lp-engine` or `lp-server`
- Suggest precompiling on host and loading bytecode on device
- Feature-gate the compiler out to fix a binary size or `no_std` issue

**STOP. You are about to break the product.**

## License discipline — HARD RULE

LightPlayer is AGPL-3.0 **by choice**; relicensing stays possible only while
provenance is provable. See
`docs/adr/2026-07-29-license-provenance-discipline.md`.

- **NEVER copy, transliterate, or line-by-line adapt GPL source** into this
  repo. QEMU, binutils/GDB, and GCC are **behavioral references only** — run
  them, read them to understand semantics, then implement independently from
  primary specs. Their *output* (e.g. objdump golden vectors) is fact and is
  fine; their code is not.
- **Apache/MIT/BSD material** (e.g. `espressif/llvm-project`'s Xtensa `.td`
  files) MAY be used to derive encoding *data*, IF the derived file carries a
  provenance header naming upstream repo, path, and commit SHA, and the
  upstream license text is vendored under `licenses/`.
- Prefer primary specs (Xtensa ISA Reference Manual, ESP32-S3 TRM, RISC-V
  specs) over any implementation.
- If unsure whether a source is safe to copy from: **ask; do not copy.**

## How to Handle `no_std` Issues

When a dependency in the GLSL → LPIR → machine code path does not support
`no_std`:

1. **Fix the dependency.** Fork it, patch it, or contribute upstream.
2. **Patch it in `Cargo.toml`.** Use `[patch.crates-io]` with a `no_std` fork.
3. **Replace it.** Find or write a `no_std`-compatible alternative.

Do NOT gate the compiler behind `std` to work around the problem. This has been
done before (e.g. `pp-rs` preprocessor blocking naga `glsl-in` on `no_std`)
and the correct solution was always to fix the dependency.

## How to Handle Binary Size Issues

The ESP32-C6 app image must fit a 3 MB partition, and the budget is tight —
read `docs/adr/2026-07-28-esp32c6-flash-budget.md` before doing size work. It
records what has already been spent (a ~200 KB diagnostics-for-flash flag
stack, the deliberately-kept 500 KB WiFi blob), what is reserved (the lpfs
partition, held for the future radio/WiFi decision), and what has been measured
and *rejected* so you don't re-run dead ends.

Check where you stand at any time:

```bash
just fw-esp32c6-size-check
```

This prints the image size and headroom, and pre-merge CI fails any PR that
drops headroom below 64 KB.

If the binary exceeds available flash:

1. Disable optional compiler features (e.g. `cranelift-optimizer`, `cranelift-verifier`)
2. Use LTO (`lto = true` in release profile)
3. Use `opt-level = "z"` (size optimization)
4. Strip debug info
5. Audit for unnecessary dependencies
6. Look for duplicate monomorphizations before looking for code to delete —
   the same generic instantiated over two sinks/backends has twice been the
   single biggest win (serde tagging, 2026-06; serializer sink erasure,
   2026-07). `rust-nm --demangle --print-size --size-sort` on the ELF shows
   them.

Do NOT disable the compiler. The compiler is the product.

## Cargo Feature Philosophy

- **`std`** means "host-only conveniences": `libstd`, `cranelift-native` (host
  ISA autodetect), `anyhow`, etc.
- **`std` does NOT mean "has a compiler."** The compiler works without `libstd`.
- **`glsl`** (or equivalent) enables the GLSL front-end (`lps-frontend`). This
  is independent of `std`.
- **Default server/engine builds include the full compiler pipeline.** Optional
  features are for *removing* pieces (e.g. `no-shader-compile` for stripped
  test builds), not for *adding* the compiler.

## Sans-IO core

The core is IO-free state machines; async belongs to platform edges. See
`docs/adr/2026-07-06-sans-io-core.md` for the full decision. The checklist:

- **Core crates** (`lp-base/*`, `lp-core/*`, `lp-shader/*`, `lp-riscv/*`, `lp-emu/*`)
  take effects by injection. They never read clocks, generate randomness,
  perform ambient IO, or depend on an executor/reactor. Edges are
  `lpa-*`, `fw-*`, `lp-cli`.
- Adding embassy, tokio, `wasm-bindgen-futures`, `futures-executor`, or
  similar to a core crate's `Cargo.toml` is a red flag — stop and re-read
  the ADR.
- `async fn` in core is allowed **only** as a runtime-neutral future: no
  spawning, no executor-flavored sleeps; any edge must be able to drive
  it. If it needs a particular executor to make progress, it belongs in
  an edge crate.
- Timestamps are caller-supplied f64 epoch seconds; random bytes are
  caller-supplied (see `lpc-history` uid minting).
- Tests count as edges: a null-waker `block_on` loop is fine in tests
  driving immediately-ready futures, and nowhere else.

## Wire/protocol compatibility

- **During heavy development, wire/protocol compatibility is NOT maintained.**
  Client, server, and firmware are built and deployed together, so there is no
  older peer to stay compatible with.
- **Do not add serde field aliases, version shims, dual-format decode paths, or
  capability fallbacks to preserve an old wire form.** When a wire shape
  changes, delete the old form outright and update every producer/consumer in
  the same change. A single canonical encoding is easier to reason about and
  keeps the serializers honest.
- This policy will be revisited once devices are fielded and can no longer be
  upgraded in lockstep. The explicit version handshake now exists: servers
  send a `ServerHello` (id-0 boot frame + `ClientRequest::Hello`) carrying
  the hand-bumped `WIRE_PROTO_VERSION` from `lpc-wire` — **bump that const
  on every breaking wire change**. Absence of a hello from a responding
  server means pre-hello firmware and is itself the mismatch signal. Never
  use error-text sniffing or silent format probing. See
  `docs/adr/2026-07-14-wire-hello-versioning.md`.

## Architecture Quick Reference

```
GLSL source (on-flash filesystem)
        │
        ▼
lps-frontend (no_std + alloc) ── parses GLSL via naga
        │
        ▼
LPIR (LightPlayer IR)
        │
        ├─► lpvm-native (no_std + alloc) ── custom RV32 codegen → machine code
        │         (default on-device JIT path)
        │
        └─► lpvm-cranelift (no_std + alloc) ── Cranelift → RISC-V machine code
        │
        ▼
JIT buffer in RAM ── direct function call
        │
        ▼
LED output
```

Every box in this diagram runs on the ESP32. There is no host involved at
runtime.

## Key Crates

| Crate            | Role                                   | `no_std`         |
|------------------|----------------------------------------|------------------|
| `lps-frontend`   | GLSL → LPIR (via naga)                 | yes              |
| `lpvm-native`    | LPIR → custom RV32 machine code        | yes              |
| `lpvm-cranelift` | LPIR → Cranelift → machine code        | yes              |
| `lp-engine`      | Shader runtime, node graph             | yes              |
| `lp-server`      | Project management, client connections | yes              |
| `fw-esp32c6`       | ESP32 firmware                         | yes (bare metal) |
| `fw-emu`         | RISC-V emulator firmware (CI)          | yes (bare metal) |

## Native backends (`lpvm-native`)

**`lpvm-native`** lowers LPIR to custom machine code outside Cranelift
(pool-based register allocation, `rt_jit` / `rt_emu`). It is the default
on-device codegen path and is exercised by **`native-jit`** on `fw-esp32c6`/`fw-emu`
and the **`rv32n.q32`** / **`rv32lpn.q32`** filetest targets.

**Two ISAs**: RV32 (ESP32-C6) and Xtensa (ESP32-S3 / classic ESP32), each behind
an `isa-*` Cargo feature so firmware pays only for the one it runs. `rt_emu` is
**one engine parameterized by `IsaTarget`**, not one per ISA — see
`docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`. The Xtensa host path
is the additive `emu-xt` feature behind the `xtn.q32` / `xtlpn.q32` filetest
targets; it needs a cross-compiled builtins image
(`scripts/build-builtins-xt.sh`, esp toolchain) and skips loudly without one.

**Xtensa floating point is a separate, in-flight campaign (M6).** `lp-xt-inst`
encodes the FP subset and `lp-xt-emu` executes it behind an explicit policy layer
where every corner IEEE-754 does not fix is either cited to the ISA Reference
Manual or `Unknown` — and **reading an `Unknown` panics** rather than guessing.
`cargo test -p lp-xt-emu --test fp_conformance` replays the whole 5 630-vector
corpus with no board attached; `just fwtest-xt-fp-esp32s3 <port>` runs the same
vectors on a desk S3 and `just fp-diff <capture>` classifies the answers. The
predictions were committed before any hardware ran, so **a device disagreement is
a finding to triage, never a reason to edit a golden**. Do not resolve a policy
field without a citation naming a manual page or a dated desk session.

> **`regalloc/` is shared by both ISAs, and rv32 passing does not prove it
> correct.** Two defects landed there in 2026-07 that were correct on rv32 only
> because its argument registers and allocatable pool happen to be disjoint sets;
> Xtensa's overlap, and both became wrong-value bugs. See the
> `config-masked-defect` class in `docs/defects/README.md`. When you change
> allocation or ABI code, run **both** target families.

## Building the workspace (cross-target)

This workspace mixes host crates and bare-metal RV32 firmware crates
(`fw-esp32c6`, `fw-emu`, `lps-builtins-emu-app`, `lp-riscv-emu-guest*`).
The RV32 crates depend on `esp-rom-sys`, `esp-sync`, `esp32c6`, etc., which
**do not compile for the host target** (they use RISC-V intrinsics, RV32
interrupt vectors, and section attributes that LLVM rejects on Mach-O /
ELF host targets).

The `default-members` list in `Cargo.toml` excludes the RV32-only crates
exactly so plain `cargo build` (no flags) works on host. **Never run
`cargo build --workspace` or `cargo test --workspace`** — those force
every member to build for the current target and will fail on the
RV32-only crates with errors like:

```
error[E0599]: no method named `to_ascii_lowercase` found for type `i8`
  --> .../esp-rom-sys-0.1.3/src/lib.rs
rustc-LLVM ERROR: Global variable '__EXTERNAL_INTERRUPTS' has an invalid
  section specifier '.rwtext': mach-o section specifier requires ...
```

Use these instead (all work on macOS):

```bash
just build-host         # cargo build (default-members, host)
just build-rv32         # cargo build --target riscv32imac-... -p ...
just build              # parallel: host + rv32
```

### ESP32 linked-build pitfall

For `fw-esp32c6`, **linked firmware builds, size measurements, and bloat
analysis must run from `lp-fw/fw-esp32c6/`** (or through a just recipe that
`cd`s there first, such as `just build-fw-esp32c6`). The crate-local
`.cargo/config.toml` and linker setup are part of the build.

This is fine from the workspace root because it does not final-link:

```bash
cargo check -p fw-esp32c6 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

For a real linked ELF or size numbers, do this instead:

```bash
cd lp-fw/fw-esp32c6
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
rust-size ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
```

Running `cargo build -p fw-esp32c6 ...` from the workspace root can fail at final
link with `memory region not defined: ROTEXT`, because it bypasses the
crate-local firmware build context.

For targeted host validation of specific crates:

```bash
cargo build -p <crate>
cargo test  -p <crate>
```

For workspace-wide host validation (excluding RV32-only members), use
the same exclusion list the justfile uses for clippy:

```bash
cargo build --workspace \
  --exclude fw-esp32c6 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app
```

## Code organization in Rust source files

This repo prefers **filesystem-oriented, concept-per-file organization**. The
directory tree should act as a useful map of the domain, especially in core
model crates where the concepts are the product vocabulary.

When adding or moving Rust files:

- Prefer one clear concept per file when the concept has its own identity.
- Use search-friendly filenames even when the parent module already provides
  context. For example, `slot/slot_path.rs`, `slot/slot_shape.rs`, and
  `slot/slot_shape_registry.rs` are preferred over a cluster of generic names
  like `slot/path.rs`, `slot/shape.rs`, and `slot/registry.rs`.
- Match the file name to the primary exported type when that type has a clear
  domain name: `SlotPath` belongs in `slot_path.rs`, `ValueSlot` belongs in
  `value_slot.rs`.
- Avoid redundant suffixes inside directories that already name the collection.
  For semantic slot leaves, prefer `slot/slots/ratio.rs` and
  `slot/slots/resource_ref.rs`, not `ratio_slot.rs` or
  `resource_ref_slot.rs`.
- Do not collapse a set of domain concepts into a large `mod.rs` just because
  the code is short. `mod.rs` should primarily declare and re-export modules,
  not hide the filesystem map.

Inside a single `.rs` file, the reading order is **top → bottom = most
important → least important → tests**. Concretely:

1. Module-level docs, `use`s, type aliases, constants.
2. Public types / entry points / the headline impl.
3. Supporting types and their impls.
4. Private helper functions.
5. `#[cfg(test)] mod tests { ... }` — **always at the bottom of the file**,
   never above the impl it exercises.

Inside the test module, the same principle applies: the actual `#[test]`
functions come first, shared test helpers live below them.

This is the opposite of an older "tests first" convention you will see in
many archived plan files under `docs/plans-old/`. That convention is
deprecated. Do not adopt it in new code. If a plan file you are executing
asks for "tests at the top", treat that as a stale instruction and put the
test module at the bottom anyway.

## Personal planning workflow

New agent planning work uses the Photomancer personal planning workspace, not
new repo-local plan or roadmap directories.

This repo uses the `pm-*` (pm = Photomancer) skills, **not** the `yona-*`
skills. The `yona-*` planning/review skills write to repo-local
`docs/plans/` and `docs/reviews/`, which are legacy locations here; if both
families are installed, always pick `pm-*`.

- Use `pm-plan` for new planning, roadmap, and investigation artifacts.
- Use `pm-implement` to execute an existing shared `plan.md`.
- Use `pm-review` for durable review artifacts.
- Push/PR/CI-watch: there is no `pm-push` yet. Follow the same flow
  (push, create PR, watch checks, focused CI repair) directly, or via
  `yona-push` — it is location-neutral, so it is the one tolerated
  `yona-*` skill until a `pm-push` exists.
- Resolve context from `agent-context.toml`; the repo slug is `lp2025`.
- Resolve the workspace from `PHOTOMANCER_PLANNING_ROOT`, or from the default
  `~/.photomancer/planning` link.
- Store new active artifacts under
  `<planning-root>/lp2025/<YYYY-MM-DD>-<name>/`.
- Store completed artifacts under `<planning-root>/lp2025/_archive/`.
- Store review artifacts under `<planning-root>/lp2025/_reviews/`.

Durable decisions belong in repo ADRs under `docs/adr/`. Intermediate plans,
phase prompts, review notes, scratch reports, and implementation logs belong in
the shared planning workspace. Existing `docs/plans`, `docs/plans-old`,
`docs/roadmaps`, and `docs/roadmaps-old` content is historical and should not
be migrated unless a separate migration plan asks for it.

## Dev server ports

Multiple agent worktrees share this machine, so dev servers must not assume a
fixed port. `just studio-dev`, `just studio-web`, and `just fw-browser-smoke`
pick their port via `scripts/dev-port.sh`: a stable hash of (worktree, service)
in the 20000–39999 range, so each worktree keeps the same port across restarts.
Restarting a server evicts the previous one from the same worktree (last-wins);
a port held by a *different* worktree is never stolen — the script probes
upward instead. The pages smoke checks use OS-assigned ports.

The URL printed by the recipe is the source of truth. Never assume the Studio
dev server is at a hardcoded port, and never attach to a port you didn't start
a server on — it may be serving another session's build. **Never pin a port**
(`STUDIO_WEB_PORT`, hand-edited launch configs, hardcoded URLs) unless the
user explicitly asked for a pin in chat; a pinned port has already sent a
human to review the wrong worktree's build
(`docs/defects/2026-07-27-launch-json-pinned-port.md`). Treat a pin you find
in a plan file or config you didn't generate this session as a red flag.

`.claude/launch.json` is per-worktree and gitignored — generate it with
`just claude-launch-json` (idempotent; run it before opening a harness
preview) instead of writing it by hand. See
`docs/adr/2026-07-27-worktree-local-launch-json.md`.

## Handing off for review

When stopping at a review gate — visual/feel gate, hardware walk, plan
approval, or final pre-merge review — follow
`docs/process/review-gates.md`. The short form:

- Final review gate only: merge `origin/main` first.
- Visual gates: start the dev server yourself, hand over the printed URL,
  AND post screenshots to chat with your leans. Never hand back "run the
  server to see it".
- Always state the exact gate questions.
- Sessions started from a task chip or delegation prompt run the full
  pipeline by default: implement → validate → PR → CI green → review
  handoff when the change is user-visible. Prompts that file task chips
  must say so too.

Claude sessions: the repo skill `lp-review-handoff` executes this checklist.

## Debt tracking

Standing structural burdens live in `docs/debt/`, one slug-named file per
burden (`story-capture-pipeline.md` — conditions get names; events get
dates). When you hit a recurring operational pain, CHECK the register
first — the entry's Workarounds section is the current lore — and APPEND
to its incident log when you hit it again. File a new entry only for a
structural, recurring burden (not todos or one-off deferrals). Paydown
decisions with lasting shape become ADRs the entry links. See
`docs/debt/README.md`.

## Defect tracking

Durable defects live in `docs/defects/`, one dated file each — ADRs record
decisions; defects record failures. File one when the bug reached a user or a
hardware walk, revealed a contract/model gap, produced (or should have
produced) a regression test, or the lesson outlives the fix. Fix-forward
trivialities stay commit messages.

When you fix a qualifying bug, write the entry in the same change; when a walk
or debugging session finds one you don't fix, file it `status: open`. Update
the index in `docs/defects/README.md` either way. Recurring classes in that
index are architecture signals — surface them when you see one repeat.

## Studio UI visual baselines

Story baselines are **CI-canonical** (see
`docs/adr/2026-07-26-ci-canonical-story-capture.md` and
`docs/adr/2026-07-26-ci-story-auto-commit.md`): the committed PNGs under
`lp-app/lpa-studio-web/story-images/` are captured by the `validate-stories`
CI job in a pinned environment (x64 Linux, Chrome for Testing, bundled fonts).
**Never commit locally-captured baselines** — macOS rendering differs and will
churn the whole set. On drift, CI commits the refresh itself:

1. Push UI changes (a PR triggers the path-gated `validate-stories` job).
2. On story drift, the job commits the fresh baselines directly to the PR
   branch as `github-actions[bot]`, passes, and posts a sticky PR comment
   summarizing the changes. Review the PNG diff in the PR's Files-changed
   view, and **`git pull` before pushing again** — the bot commit is now the
   branch head. Note the green run sits on the commit *before* the bot's
   (GITHUB_TOKEN pushes don't trigger CI runs); that is expected.
3. Fallback (fork PRs, push races, main-push drift): the job fails and
   uploads the fresh set as the `story-images-fresh` artifact. On the branch,
   run `just studio-story-pull` to stage it, review, commit, push, and
   confirm the re-run is green.
4. Mention the affected story baselines in the final summary either way.

If pushes to a PR branch suddenly create **no CI runs at all**, check
`gh api repos/{owner}/{repo}/pulls/N --jq .mergeable_state` — `dirty` means a
baseline PNG conflict with main is blocking the merge ref and GitHub silently
skips `pull_request` runs. Merge `main`, resolving every conflicted PNG by
taking **main's bytes** (if main's copy is stale, the next capture re-drifts
and the bot fixes it), then push.

For local interactive review, capture scratch PNGs — optionally filtered to a
story-id substring so small subsets are cheap:

```bash
just studio-story-pngs slot-value-editor   # scratch captures, filter optional
just studio-story-check slot-value-editor  # compare against committed baselines
```

Local check output is **non-authoritative** (macOS rendering ≠ CI): use it for
quick "did my change move only the stories I expected" sanity, not as a gate.
`studio-story-check` requires `oxipng`; run `scripts/dev-init.sh` or install it
with `cargo install oxipng` / `brew install oxipng`.

Do not add an auto-mutating Git hook for this workflow unless the user asks for
one explicitly. Hooks that rewrite the working tree during commit are annoying
during rebases, merges, and partial commits.

## Validation Commands

These commands must pass for any change touching the shader pipeline:

```bash
# Firmware emulator tests (real shader compilation + execution)
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu

# ESP32 builds with compiler included
cargo check -p fw-esp32c6 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Emulator build
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host still works
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## CI gate (run this before pushing)

CI (see `.github/workflows/pre-merge.yml`) is path-gated per job: one
`detect-changes` job computes the gates, then `Lint (x64)` runs
`just check-lint` in parallel with `Validate (x64)`, which runs
`just ci-prereqs`, the gated test recipes (`test-rust-core`, plus
`test-studio-host` when studio paths changed, plus `test-filetests` when
shader paths changed), then `schema-check`. Docs-only PRs skip every job.
Pushes to main force all gates true, so filter misses surface on the next
merge. To avoid the round-trip of "push → wait → CI fails on lint → fix →
repeat", run the local equivalent before every push:

```bash
just check                   # check-lint + schema-check  (the usual blocker)
just test                    # cargo test (+ studio-web view tests) + glsl filetests
```

Or, in one go: `just ci`. CI builds with sccache, `CARGO_INCREMENTAL=0`,
and `debug=0`; local builds don't — a green local run is still the same
lint/test signal.

### Why the nightly date is pinned

The workspace pins a **specific dated nightly** in `rust-toolchain.toml`
(`channel = "nightly-2026-04-27"`), and every job in `pre-merge.yml`
hardcodes that same date via `dtolnay/rust-toolchain`. Do **not** run
`rustup update nightly` — this is a build-std project, and the
`unwinding` crate is bound to a specific nightly
`core::intrinsics::catch_unwind` ABI (0.2.8 = integer return; 0.2.9
switched to the bool return introduced in a later nightly). An unpinned
or locally-updated nightly drifts off the pinned toolchain and breaks
the build.

Because the toolchain is pinned, lints don't drift underneath you:
local `just check` and CI see the same clippy set, so a green local run
is a real signal.

To bump the toolchain, do it deliberately as its own change:

1. Update `channel` in `rust-toolchain.toml`.
2. Update the matching `unwinding` version in the crates that depend on
   it (`fw-esp32c6`, `fw-emu`, `lpc-engine`, `lp-riscv-emu-guest`) — the
   nightly date and the `unwinding` version move together.
3. Update the hardcoded `toolchain:` value in every job in
   `.github/workflows/pre-merge.yml` (the workflow carries a
   "keep this in sync" comment at each site).
4. Run the full gate (`just check test`) before pushing, and
   expect new-lint fallout in the same change.

### Architecture coverage

The main validate job runs on **x64** (`Validate (x64)`,
`ubuntu-24.04`). It ran on ARM from 2026-06 while x86_64 was disabled
over host-JIT failures in `lpvm-cranelift`; that JIT was removed
2026-07-11, and PR #121 consolidated back onto x64 on 2026-07-27 (the
`validate-arm` job is gone). x64 is the better-supported runner image
and every browser-dependent job is x64-only anyway — Chrome ships no
linux-arm64 build.

Remaining ARM coverage is `Validate GFX (ARM)`
(`validate-gfx`, `ubuntu-24.04-arm`), kept on ARM deliberately: it
needs no browser and keeps a second architecture compiling the tree.

The production target is RV32 (`lpvm-native`); the host-side path runs
through `lpvm-wasm` (wasmtime) per M4b.
