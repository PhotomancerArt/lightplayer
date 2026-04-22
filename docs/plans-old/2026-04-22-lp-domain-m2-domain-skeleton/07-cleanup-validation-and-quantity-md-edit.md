# Phase 7 — cleanup, validation, and `quantity.md` edit (Q15)

> Read [`00-notes.md`](./00-notes.md) and
> [`00-design.md`](./00-design.md) before starting.
>
> **Depends on:** All previous phases (1–6) complete and clean.

## Scope of phase

Final pass before commit:

1. Update [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md)
   §6 (Slot definition) and §12 #1 (non-negotiable) per the **Q15
   Option A** decision (composed Shapes carry
   `Option<ValueSpec>` for round-trip fidelity; scalar Shapes
   carry mandatory `ValueSpec`).
2. Sweep the diff for stray TODOs, debug prints, commented-out
   code, scratch files. Remove anything that isn't part of the
   intentional `TODO(M3)` / `TODO(M5)` / `TODO(M3+)` /
   `TODO(quantity widening)` markers.
3. Run the full validation matrix (per `.cursorrules` "CI gate"
   discipline) and fix every warning.
4. Run `cargo +nightly fmt` on all changed files.
5. Confirm the doc-comment cross-link from `lp-domain/lib.rs`
   to `quantity.md` still points at the right path.

This phase is `sub-agent: supervised` — dispatch as a sub-agent
but stay paired closely (review immediately, don't batch).

**In scope:**

- Edits to `docs/design/lightplayer/quantity.md` (sections 6
  and 12 only; do not rewrite the whole file).
- Diff sweep for shortcuts.
- Validation matrix.
- Formatter run.

**Out of scope:**

- Writing `summary.md` — the main agent does that **after**
  this phase passes review.
- The git commit itself — main agent does that.
- Moving the plan dir to `plans-old/` — main agent does that.
- Any new feature work.

## Code Organization Reminders

- N/A this phase — it's cleanup.

## Sub-agent Reminders

- Do **not** commit.
- Do **not** add new features. Only delete / clean / format /
  edit `quantity.md`.
- Do **not** suppress warnings or `#[allow(...)]` problems away
  — fix them.
- Do **not** disable, skip, or weaken existing tests.
- The intentional `TODO(M3)` / `TODO(M3+)` / `TODO(M5)` /
  `TODO(quantity widening)` / `TODO(phase 5)` markers are
  **deliberate** and should stay. Remove only TODOs that don't
  match those forms (i.e. transient notes left by phase
  sub-agents during development).
- If you find a real bug while sweeping, **stop and report** —
  do not fix and ship.
- Report back: list of files changed, validation matrix output,
  list of TODOs found and what you did with each.

## Implementation Details

### 1. `quantity.md` §6 edit

Find the `Slot` definition block in §6:

```rust
pub enum Shape {
    Scalar { kind: Kind, constraint: Constraint },
    Array  { element: Box<Slot>, length: u32 },
    Struct { fields:  Vec<(String, Slot)> },   // ordered
}

pub struct Slot {
    pub shape:       Shape,
    pub default:     ValueSpec,            // mandatory; see §7
    pub label:       Option<String>,
    pub description: Option<String>,
    pub bind:        Option<Binding>,      // §8
    pub present:     Option<Presentation>, // §9; absent = Kind default
}
```

Replace with:

```rust
pub enum Shape {
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,                    // mandatory on Scalar
    },
    Array {
        element: Box<Slot>,
        length: u32,
        default: Option<ValueSpec>,            // None ⇒ derive from element
    },
    Struct {
        fields: Vec<(String, Slot)>,           // ordered
        default: Option<ValueSpec>,            // None ⇒ derive from fields
    },
}

pub struct Slot {
    pub shape:       Shape,
    pub label:       Option<String>,
    pub description: Option<String>,
    pub bind:        Option<Binding>,      // §8
    pub present:     Option<Presentation>, // §9; absent = Kind default
}

impl Slot {
    /// What the GPU/serializer sees.
    pub fn storage(&self) -> LpsType;

    /// Single load+bind-time check.
    pub fn validate(&self, v: &LpsValue) -> Result<(), DomainError>;

    /// Materialize the default value. Scalar reads `default` directly;
    /// composed Shapes use the override if present, otherwise derive
    /// from children.
    pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue;
}
```

(Keep the existing `impl Slot { storage / validate }` lines that
were below the struct; just add `default_value` as shown.)

Find and update the existing "Defaults for compositions"
subsection at the end of §6 (lines ~325–329):

Before:

```
### Defaults for compositions

A composition's `default` (`ValueSpec::Literal(LpsValue::Struct{...})`
or `Array`) may be omitted in TOML and *computed* from children's
defaults at load time. The in-memory `Slot` always carries one.
```

After:

```
### Defaults for compositions

A composition's `default` is **literally optional** —
`Shape::Array` and `Shape::Struct` carry
`default: Option<ValueSpec>`. `Scalar` carries a mandatory
`ValueSpec`.

- `default = Some(spec)` ⇒ materialize from `spec` (the only
  way to express aggregate-level overrides like the array
  preset in §10).
- `default = None`        ⇒ derive at load time:
  - `Array`  ⇒ `LpsValue::Array(N copies of element.default_value())`.
  - `Struct` ⇒ `LpsValue::Struct(field_name → field.default_value())`.

Round-trip parity is automatic via
`#[serde(skip_serializing_if = "Option::is_none")]` on the
composed-default fields: a TOML file that omitted `default` on a
struct gets re-saved without one.
```

### 2. `quantity.md` §12 #1 edit

Find non-negotiable #1 in §12:

```
1. **Every Slot has a `default: ValueSpec`.** No `Option<>`. Init-
   order ambiguity is the can of worms we're closing.
```

Replace with:

```
1. **Every Slot can produce a default value at materialize
   time.** `Scalar` Shapes carry a mandatory `ValueSpec`.
   Composed Shapes (`Array`, `Struct`) carry an
   `Option<ValueSpec>` override and derive a default from their
   children when `None`. Round-trip preserves whether the
   composed default was explicit. Init-order ambiguity is still
   closed: `Slot::default_value(ctx)` always returns an
   `LpsValue`.
```

### 3. Diff sweep

From the repo root:

```bash
git diff --diff-filter=AM -- 'lp-domain/**/*.rs' 'lp-shader/lps-shared/**/*.rs' \
  | grep -nE 'TODO|XXX|FIXME|dbg!|println!|eprintln!|unimplemented!|todo!|//\s*scratch'
```

Examine every match.

- **Keep**: `TODO(M3)`, `TODO(M3+)`, `TODO(M4)`, `TODO(M5)`,
  `TODO(quantity widening)`, `TODO(phase 5)` — these are
  deliberate, scoped to a future milestone or known follow-up.
- **Remove**: any other TODO/FIXME/XXX, all `dbg!()`, all
  `println!`/`eprintln!` outside of legitimate error paths
  (there shouldn't be any in lp-domain), `unimplemented!()`,
  `todo!()`, scratch comments.

If something looks borderline (e.g. a `TODO` left by a sub-agent
that doesn't match the deliberate forms), **stop and report** —
the main agent decides whether to keep or kill it.

### 4. Validation matrix

```bash
cargo check -p lps-shared
cargo check -p lps-shared --features schemars
cargo test  -p lps-shared
cargo test  -p lps-shared --features schemars
cargo check -p lp-domain
cargo check -p lp-domain --features std
cargo check -p lp-domain --features schema-gen
cargo test  -p lp-domain
cargo test  -p lp-domain --features schema-gen
```

All must pass with **zero warnings**. The `.cursorrules` CI gate
is `rustup update nightly && just check`. If `just check` is
quick, run it too — it catches lints that nightly toolchain
adds (e.g. `float_literal_f32_fallback`, `manual_clamp`,
`clone_on_copy`, `allow_attributes_without_reason`).

### 5. Formatter

```bash
cargo +nightly fmt
```

Then:

```bash
git diff --stat
```

Confirm the formatter touched only what we expect (the new files
and the small `quantity.md` block). If it touched unrelated files,
revert those — they belong in a separate commit.

### 6. Cross-link sanity

Open `lp-domain/lp-domain/src/lib.rs` and confirm the
`docs/design/lightplayer/quantity.md` link at the top still
resolves correctly from this file (relative path `../../../docs/...`).

## Definition of done

- `quantity.md` §6 reflects the Option-A `Shape` shape (defaults
  on the variants, not on `Slot`); the "Defaults for
  compositions" subsection rewritten.
- `quantity.md` §12 non-negotiable #1 rewritten per the new
  contract.
- Diff sweep complete; no stray non-deliberate TODOs / debug
  output.
- Validation matrix passes with zero warnings.
- `cargo +nightly fmt` clean.
- Cross-link from `lib.rs` doc comment still resolves.
- No commit.

Report back with: list of files changed, the matrix output
summary, the list of TODOs you encountered (deliberate vs.
removed), and whether `just check` was run + its result.
