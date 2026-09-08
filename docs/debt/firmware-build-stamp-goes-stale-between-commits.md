---
status: carried
since: 2026-08
logged: 2026-09-08
area: lp-fw/fw-esp32c6/build.rs (and the other firmware build scripts)
related:
  - docs/debt/reference-images-are-not-reproducible-across-hosts.md
  - docs/adr/2026-09-06-hardware-validation-system.md
---
# A firmware image can carry a commit stamp older than the commit it was built at

**Shape** — `lp-fw/fw-esp32c6/build.rs` stamps `LP_BUILD_COMMIT` and
`LP_BUILD_DIRTY` from git, and the firmware puts them in its hello frame, so
Studio and every capture can say which source a running image came from.

But the script emits `cargo:rerun-if-changed` for the **package directory**
(and esp-hal's out dir). Emitting any `rerun-if-changed` replaces cargo's
default rule, so the script re-runs when something under `lp-fw/fw-esp32c6/`
changes — and a commit that touches only `lp-core/`, `lp-app/` or `lp-shader/`
does not. The firmware itself is rebuilt (those are real dependencies) but the
build script is fingerprint-fresh, so the new binary carries the **previous**
commit's stamp. It is not wrong about the code; it is wrong about the name of
the code.

**Carrying cost** — a wrong stamp in the one place the system uses to say what
it measured. Studio's device card shows it. A capture whose `firmware_commit`
check compares the hello's stamp against the commit the operator stated
refuses, or worse agrees when it should not. The validation runner is not
exposed — DD14 made `--commit` a required argument precisely so provenance is
stated rather than sniffed — but every human reading a hello frame is.

**Workarounds** — `touch lp-fw/fw-esp32c6/src/main.rs` before a build whose
stamp matters (the same incantation the feature-flip comments in that crate's
`Cargo.toml` already tell you to use). The reference-image script builds in a
detached worktree at a pinned commit, so its images are not affected.

**Incident log**

- 2026-09-07 (G3 sitting 1, DD35) — found while chasing a `firmware_commit`
  refusal on a fresh capture; raised into M8's sweep.
- 2026-09-08 (M8) — filed rather than fixed, with the two options costed
  below. A build-graph change to product firmware is not something to ride
  along on a milestone's closing PR.

**Exit criteria** — one of:

1. **Watch git.** `cargo:rerun-if-changed=<git-dir>/HEAD` plus the ref file
   HEAD points at. Correct, three lines, and it re-runs the script — and so
   relinks the firmware — on **every commit**, including the many that touch
   nothing it depends on. On a repo with dozens of commits a day that is a
   real tax on every local firmware build.
2. **Stop stamping at build time.** Move the commit out of the image and into
   the artefact's sidecar, where the flasher (which knows the tree it is
   flashing from) writes it. Costs the hello frame a field that several
   surfaces read, so it is an interface change, not a build fix.

Neither is obviously right, which is why this is debt and not a chip: pick one
deliberately, in a change that owns the consequence.
