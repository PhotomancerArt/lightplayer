# Defect registry

A durable record of defects worth remembering. ADRs record decisions;
defects record failures. Where an ADR captures "we chose X among
plausible alternatives," a defect entry captures "the system did Y when
it should have done Z, and here is the mechanism" — so the same
mechanism is recognized the next time it dresses up in a different
symptom.

Entries live in this directory, one dated file each:
`YYYY-MM-DD-slug.md`, dated by when the defect was **found**.

## The filing bar

File a defect when at least one of these holds:

- It **reached a user or a hardware walk** — someone observed the
  failure outside a test run.
- It **revealed a contract or model gap** — the bug is evidence that
  two components disagree about an interface, or that the domain model
  conflates things it shouldn't.
- It **produced (or should have produced) a regression test** — if the
  fix deserved a named test, the failure deserves a record; if coverage
  was impossible, that gap is itself worth recording.
- The **lesson outlives the fix** — the entry would change how someone
  writes the next feature, not just how they read this diff.

Fix-forward trivialities — typos, off-by-ones caught in review, build
breakage — stay commit messages. The registry is for defects whose
*shape* recurs.

Write the entry **at fix time, riding the fix commit**: the same change
that fixes a qualifying bug adds its entry (and updates the index
below). `status: open` entries are legal and expected for
found-not-yet-fixed defects — hardware-walk and live-debugging findings
get a home immediately, before anyone decides when to fix them.

## Entry template

```markdown
---
status: fixed          # open | fixed | wontfix
found: YYYY-MM-DD      # how: hardware-walk | live-debugging | ci | e2e | report
fixed: <commit>        # absent while open. NOTE: an entry cannot cite
                       # its OWN commit (the hash doesn't exist yet, and
                       # amending changes it) — write `fixed: this change`
                       # at commit time and fill the real hash in the NEXT
                       # commit that touches the registry.
area: <crate/module>
class: <one from the vocabulary>
related: []            # other defects, ADRs, plan dirs
---
# <one-line title>

**Symptom** — what was observed, verbatim error text included.
**Root cause** — the mechanism, not the patch.
**Fix** — what changed and where (the commit is the diff; this is the shape).
**Regression coverage** — named tests, or "none: <why>".
**Lesson** — one paragraph; what this implies beyond the fix.
```

## Class vocabulary

Every entry carries a `class` — the failure's mechanism, not its
surface. The vocabulary is extensible: add a class when a defect
genuinely fits none of these, and define it here in one line.

- **`backend-contract-divergence`** — two implementations of one
  contract disagree on details only real hardware surfaces.
- **`lifecycle-ownership`** — two layers both believe they own a
  resource's lifecycle.
- **`partial-knowledge-loss`** — an error path discards facts already
  learned.
- **`policy-leak`** — one context's policy applied in another.
- **`assumed-context`** — code presumes state instead of asking the
  source of truth.
- **`state-conflation`** — one state models two different facts.
- **`stand-in-divergence`** — a stand-in (placeholder, mock, fallback)
  meant to be equivalent to what it replaces diverges in a dimension the
  substitution didn't model.
- **`inline-emit-stack-imbalance`** — a code-emitter leaves the operand
  stack unbalanced, and a downstream construct hides it from validation.
- **`untested-path`** — a variant of a fixed bug survives in a sibling
  code path the fix and its tests never reached.
- **`stale-measurement`** — a cached measurement outlives its validity
  because the events that invalidate it aren't all observed.
- **`budget-exhaustion`** — a hard resource budget is enforced only by
  a tool outside CI, so growth crosses the limit silently and the wall
  surfaces on whoever builds next.
- **`ungated-variant`** — a build configuration no gate ever compiles,
  so upstream API drift accumulates in it invisibly until someone
  reaches for it.
- **`nondeterministic-capture`** — a capture records one of several
  reachable renderings, because the thing being photographed had not
  reached a single settled state.
- **`config-masked-defect`** — shared code is correct only under
  incidental properties of the *one* configuration that exercises it, so
  no test can falsify it until a second configuration arrives.
- **`split-source-of-truth`** — one fact is derived two ways from two
  sources, both derivations are used, and nothing checks they agree.
  Each producer can have a passing test asserting its own opinion; what
  is untested is the *hand-off*. Note this classes the **defect**,
  whereas `config-masked-defect` classes the **masking** — the two are
  orthogonal axes and a given entry may sit on both (see the note below).
- **`unsynchronized-shared-artifact`** — two steps share a filesystem
  artifact, but the lock that would order them is scoped narrower than
  the artifact, so a reader observes a writer's intermediate state.
- **`opt-in-degradation`** — an absent or unusable input is modelled as a
  legal degraded *value* rather than an error, so the intended graceful
  behaviour (skip, fall back, no-op) holds only for consumers that
  explicitly interrogate it. Consumers that do not get the raw failure,
  worded as the subject's fault and surfacing at first use rather than at
  the point of substitution. The population of callers grows; the guard
  does not.
- **`model-conflation`** — a model represents two things the real system
  keeps separate as one resource, so they contend for something that on
  hardware they never share. Presents as a *capacity* failure in the units
  of the conflated resource, which is why it invites resolutions that all
  preserve the wrong model (make it bigger, split it, shrink what goes in
  it). The diagnostic question is not "how do we make it fit" but "why are
  these in the same place here when they are not on the device".
- **`invented-encoding`** — a binary format's numbering (an instruction
  encoding, a relocation type) is inferred from the shape of its
  neighbours instead of read from the spec, and the invented value
  collides with, or is renamed from, a real one. Toolchain output is the
  only falsifier, so the collision surfaces whenever a compiler first
  emits the stolen form — which is usually at some optimization
  threshold nothing in the suite crosses.
- **`incomplete-subset`** — a normative instruction/opcode subset is
  assembled by enumerating what an external tool (objdump, an
  assembler) is believed to assign, and the enumeration silently omits
  a member that tool actually does assign — so the gap survives
  exactly as long as nothing exercises that one member. Real toolchain
  output (a sequence transcription, not memory) is the only falsifier.
- **`retired-surface-still-reachable`** — a surface believed replaced is
  still rendered, because its replacement can be absent and the old
  surface is the fallback branch.

## Index

Grouped by class, because a class that keeps recurring is the
model-smell signal: one `backend-contract-divergence` is a bug, two in
a week is an argument for a conformance suite. When a class accumulates
entries, say so out loud — that is an architecture finding, not a
bookkeeping fact.

Saying it out loud: **`config-masked-defect` took five entries on
2026-07-30**, all in `lpvm-native`, all latent for the entire life of the
rv32-only era, and all made observable within hours of the Xtensa corpus
landing. The finding is not "the allocator had bugs" — it is that a
single-configuration test suite cannot falsify configuration-dependent
code, however large it is (31,587 rv32 cases did not). The mitigation is
a second configuration that overlaps where the first is disjoint, which
is what the Xtensa targets now are.

Four of the five are in the shared register allocator. The fifth — the
integer div-by-zero trap — is worth separating, because it says the class
is not confined to `regalloc/`. That one is in *lowering*, and the
incidental property it leaned on was not a register layout but a
**hardware semantic**: RV32M defines `x / 0` and `x % 0`, so emitting the
bare divide was correct on rv32 for free. Its falsifying test also already
existed — the corpus has pinned that contract for as long as it has
existed; what was missing was a backend to run it against, plus
documentation that told the backend author the guard obligation was
somebody else's. The generalizable rule: when a contract is satisfied for
free on the reference target, that is exactly when it must be stated as an
obligation behind a named capability hook, because nothing in the code
will ever remind you it was a choice.

Saying it out loud again, one axis over: **`split-source-of-truth` and
`config-masked-defect` are describing the same 2026-07-30 defects from two
directions.** `xtensa-sret-pointer-clobber` (`FuncAbi::allocatable` computed
the withheld register, `RegPool` ignored it) and `jit-sret-return-count-zero`
(`ret_count` from the IR, `is_sret` from the ABI) are the *same bug twice* —
one fact, two derivations, no check on the hand-off. In both, the producer had
a passing unit test asserting its own opinion; neither had a test on the
consumer honouring it.

They are filed under different classes because the existing entries are classed
by *how they survived* (register-layout accident vs a `cfg` boundary no host
test can cross) rather than by *what went wrong*. That is a real distinction
worth keeping — but it splits a recurrence across buckets, which defeats the
point of grouping. If a fourth of these lands, collapse the axes: class by the
disagreement, record the masking mechanism as a field.

A sharper sub-lesson from the fourth: three of the four were the *same
invariant* — a call-boundary register transfer must behave as a parallel
move — applied at three of its four sites. Each fix was correct and
under-scoped. When a fix establishes an invariant, enumerate every place
it applies before closing the entry; here that enumeration was one
sentence (arguments in, returns out; registers and stack).

**The 2026-08-01 entry moves the masking axis off the ISA.**
`xtlpn-f32-loses-writes-to-value-parameters` is the same shape — shared code
whose fast path was safe only for the configurations anyone ran — but what
masked it was the **frontend**, not the register layout: Naga copies parameters
into fresh locals, `lps-glsl` reuses the parameter's own vreg, and only the
second shape can make a lowering shortcut read a stale copy. So the mitigation
generalizes past "a second ISA": the falsifying configuration is the *product*
of the axes a compile is parameterized by (frontend × ISA × float mode), and a
target that exists in the matrix but not in the suite is a configuration nothing
can falsify. It needed all three axes at once, and it was found within hours of
the combination first being registered as a target.

| Class | Date | Entry | Status | Area |
| --- | --- | --- | --- | --- |
| opt-in-degradation | 2026-08-01 | [xt-builtins-image-strands-just-test](2026-08-01-xt-builtins-image-strands-just-test.md) | fixed | justfile (`ci-prereqs`/`test`) + build-builtins-xt.sh + lpvm-native tests |
| config-masked-defect | 2026-08-01 | [xtlpn-f32-loses-writes-to-value-parameters](2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md) | fixed | lpvm-native lowering (lower_f32.rs) |
| precision-loss-at-a-seam | 2026-08-01 | [gamma-8bit-choke](2026-08-01-gamma-8bit-choke.md) | fixed | lpc-engine fixture node |
| misattributed-symptom | 2026-08-01 | [classic-rmt-open-fault](2026-08-01-classic-rmt-open-fault.md) | fixed | lpc-shared DisplayPipeline + fw-esp32-common provider |
| capacity-regression | 2026-08-01 | [classic-heap-regression-after-f32-merge](2026-08-01-classic-heap-regression-after-f32-merge.md) | **open** | unattributed (3 f32 PRs are the candidates) |
| test-rig-lies-about-its-subject | 2026-08-01 | [xt-pipeline-rigs-declare-param-types-as-return-types](2026-08-01-xt-pipeline-rigs-declare-param-types-as-return-types.md) | fixed (f32 rig; Q32 rig outstanding) | lpvm-native tests + lpir::builder |
| model-conflation | 2026-08-01 | [xt-f32-builtins-exhaust-the-emulator-code-region](2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md) | fixed | lp-xt-emu (board/memory) + lps-builtins-xt-app + lpvm-native/rt_emu |
| upstream-toolchain-limitation | 2026-08-01 | [xtensa-backend-cannot-select-float-constant-pool](2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md) | **open** (worked around) | lps-builtins + esp Rust toolchain |
| invented-encoding | 2026-07-31 | [zexth-encoding-steals-xori-128](2026-07-31-zexth-encoding-steals-xori-128.md) | fixed | lp-riscv-inst (encode/decode) + lp-riscv-emu (executor) |
| invented-encoding | 2026-07-31 | [elf-loader-riscv-reloc-numbering](2026-07-31-elf-loader-riscv-reloc-numbering.md) | **open** | lp-riscv-elf (relocations) |
| partial-knowledge-loss | 2026-07-31 | [elf-loader-drops-relocation-addends](2026-07-31-elf-loader-drops-relocation-addends.md) | fixed | lp-riscv-elf (relocations) |
| incomplete-subset | 2026-07-31 | [mksadj-missing-from-fp-subset](2026-07-31-mksadj-missing-from-fp-subset.md) | fixed | lp-xt/lp-xt-inst (FP subset) |
| split-source-of-truth | 2026-07-30 | [jit-sret-return-count-zero](2026-07-30-jit-sret-return-count-zero.md) | fixed | lpvm-native/rt_jit (module.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-call-argument-clobber](2026-07-30-xtensa-call-argument-clobber.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-sret-pointer-clobber](2026-07-30-xtensa-sret-pointer-clobber.md) | fixed | lpvm-native/regalloc (pool.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-stack-arg-staged-over](2026-07-30-xtensa-stack-arg-staged-over.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-two-value-return-clobber](2026-07-30-xtensa-two-value-return-clobber.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-integer-div-by-zero-trap](2026-07-30-xtensa-integer-div-by-zero-trap.md) | fixed | lpvm-native lowering (lower.rs) |
| config-masked-defect | 2026-07-31 | [opt-z-missed-rmt-drain-deadline](2026-07-31-opt-z-missed-rmt-drain-deadline.md) | fixed | workspace release profile / fw-esp32s3 |
| backend-contract-divergence | 2026-07-30 | [q32-native-vs-wasmtime-last-bit](2026-07-30-q32-native-vs-wasmtime-last-bit.md) | **open** | lpvm-native / lpvm-wasm (Q32 execution) |
| backend-contract-divergence | 2026-07-17 | [deletedir-error-shape](2026-07-17-deletedir-error-shape.md) | fixed | lpa-server + lpa-client |
| backend-contract-divergence | 2026-07-22 | [littlefs-listdir-doubled](2026-07-22-littlefs-listdir-doubled.md) | fixed | fw-esp32/fs |
| backend-contract-divergence | 2026-07-27 | [created-package-unloadable](2026-07-27-created-package-unloadable.md) | fixed | lpa-studio-core/library |
| budget-exhaustion | 2026-07-28 | [esp32c6-app-partition-overflow](2026-07-28-esp32c6-app-partition-overflow.md) | **open** (mitigated −42 KB) | lp-fw/fw-esp32 (partitions) |
| ungated-variant | 2026-07-28 | [fw-esp32-harnesses-rotted-uncompiled](2026-07-28-fw-esp32-harnesses-rotted-uncompiled.md) | fixed | lp-fw/fw-esp32c6 (src/tests/ + cfg gates) |
| ungated-variant | 2026-07-30 | [stacked-prs-get-no-ci](2026-07-30-stacked-prs-get-no-ci.md) | fixed | .github/workflows/pre-merge.yml (trigger) |
| lifecycle-ownership | 2026-07-16 | [browser-serial-endpoint-lost](2026-07-16-browser-serial-endpoint-lost.md) | fixed | lpa-link/registry |
| lifecycle-ownership | 2026-07-22 | [flash-session-map-deleted](2026-07-22-flash-session-map-deleted.md) | fixed | lpa-link/browser-serial |
| state-conflation | 2026-07-17 | [unreadable-masqueraded-as-empty](2026-07-17-unreadable-masqueraded-as-empty.md) | fixed | lpa-studio-core/roster |
| state-conflation | 2026-07-22 | [read-failure-vs-unreadable-content](2026-07-22-read-failure-vs-unreadable-content.md) | **open** | lpa-studio-core/roster |
| state-conflation | 2026-07-26 | [worker-poisoned-instance-reuse](2026-07-26-worker-poisoned-instance-reuse.md) | fixed | fw-browser + lpa-link/browser-worker |
| state-conflation | 2026-07-28 | [playlist-entry-selection](2026-07-28-playlist-entry-selection.md) | fixed | lpa-studio-core/project (node face derivation) |
| assumed-context | 2026-07-17 | [storage-slot-assumed](2026-07-17-storage-slot-assumed.md) | fixed | lpa-studio-core/places |
| assumed-context | 2026-07-23 | [deploy-dialog-ignores-running-project](2026-07-23-deploy-dialog-ignores-running-project.md) | fixed | lpa-studio-core/device |
| assumed-context | 2026-07-27 | [launch-json-pinned-port](2026-07-27-launch-json-pinned-port.md) | fixed | dev tooling (launch.json + dev-port.sh) |
| assumed-context | 2026-07-30 | [vacuity-guard-tripped-on-color](2026-07-30-vacuity-guard-tripped-on-color.md) | fixed | .github/workflows/pre-merge.yml (Xtensa gate) |
| partial-knowledge-loss | 2026-07-22 | [identity-lost-on-failed-read](2026-07-22-identity-lost-on-failed-read.md) | fixed | lpa-studio-core/places+studio |
| partial-knowledge-loss | 2026-07-23 | [reconnect-transient-twin-card](2026-07-23-reconnect-transient-twin-card.md) | fixed | lpa-studio-core/home + device |
| policy-leak | 2026-07-17 | [hardware-attach-opened-editor](2026-07-17-hardware-attach-opened-editor.md) | fixed | lpa-studio-core/studio |
| stand-in-divergence | 2026-07-23 | [popover-open-resizes-card](2026-07-23-popover-open-resizes-card.md) | fixed | lpa-studio-web/base/popover |
| stand-in-divergence | 2026-07-27 | [story-check-tolerance-ignores-amplitude](2026-07-27-story-check-tolerance-ignores-amplitude.md) | **open** | lpa-studio-web/scripts + CI |
| nondeterministic-capture | 2026-07-28 | [overview-composite-capture-races](2026-07-28-overview-composite-capture-races.md) | fixed | lpa-studio-web story capture (overview composites) |
| retired-surface-still-reachable | 2026-07-28 | [retired-device-pane-still-reachable](2026-07-28-retired-device-pane-still-reachable.md) | fixed | lpa-studio-core/home + studio_shell |
| stale-measurement | 2026-07-30 | [deploy-compiles-previous-upload](2026-07-30-deploy-compiles-previous-upload.md) | **fixed** (CLI-side; hardware confirmation pending P7) | lp-cli (upload observability) |
| stale-measurement | 2026-07-26 | [popover-outline-stale-on-content-resize](2026-07-26-popover-outline-stale-on-content-resize.md) | fixed | lpa-studio-web/base/popover |
| stale-measurement | 2026-07-27 | [code-editor-gutter-misaligned](2026-07-27-code-editor-gutter-misaligned.md) | fixed | lpa-studio-web/base/code_editor |
| inline-emit-stack-imbalance | 2026-07-27 | [wasm-q32-fabs-stack-leak](2026-07-27-wasm-q32-fabs-stack-leak.md) | fixed | lpvm-wasm emit (+ lpvm-cranelift trunc) |
| untested-path | 2026-07-27 | [cranelift-q32-floor-ceil](2026-07-27-cranelift-q32-floor-ceil.md) | fixed | lpvm-cranelift q32_emit (rv32c) |
| silent-drop | 2026-07-28 | [flash-progress-never-reached-the-ui](2026-07-28-flash-progress-never-reached-the-ui.md) | fixed | lpa-studio-core (actor/controller) |
| silent-drop | 2026-07-31 | [loader-silently-drops-unparseable-nodes](2026-07-31-loader-silently-drops-unparseable-nodes.md) | fixed | lpc-engine loader + flush + virtual ws281x |
| unbounded-restatement | 2026-07-28 | [tick-error-restated-every-frame](2026-07-28-tick-error-restated-every-frame.md) | fixed | lpa-server (advance_frame) |
| unsynchronized-shared-artifact | 2026-07-29 | [builtins-elf-uplift-race](2026-07-29-builtins-elf-uplift-race.md) | fixed | justfile `test` + lpvm-cranelift/build.rs |
| missing-coverage | 2026-07-29 | [uniform-struct-array-runtime-index](2026-07-29-uniform-struct-array-runtime-index.md) | fixed | examples/effects/meteor + lps-frontend lowering |

## Predecessor: `docs/bugs/`

Two ad-hoc pre-registry writeups live in `docs/bugs/` (2026-03 JIT
filetest segfault, cranelift rv32 ld instruction). They stay where they
are as historical record; new entries belong here.
