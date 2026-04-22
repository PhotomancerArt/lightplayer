# M2 — lp-domain skeleton + foundational types — design

This is the design we agreed on with the user. It is the contract
every phase implements. Phases must read both this file and
[`00-notes.md`](./00-notes.md) before starting work.

## Scope of work

Stand up `lp-domain/lp-domain/` as a `no_std + alloc` crate
containing the foundational vocabulary of the LightPlayer domain
model:

1. **Identity & addressing types** — `Uid` (u32 newtype),
   `Name`, `NodePath`, `PropPath` (alias of
   `lps_shared::path`), `NodePropSpec`, `ArtifactSpec`,
   `ChannelName`.
2. **Quantity model** (per
   [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md))
   — `Kind` (open enum, 12 v0 variants), `Dimension`, `Unit`,
   `Colorspace`, `InterpMethod`, `Constraint`, `Shape`, `Slot`,
   `ValueSpec`, `TextureSpec`, `Binding`, `BindingResolver`
   (trait stub), `Presentation`, plus per-Kind impls.
3. **Trait surface** — `Node` trait, `Artifact` trait,
   `Migration` trait (signatures only), empty `Registry` shape.
4. **Re-exports from `lps-shared`** — `LpsType`,
   `LpsValueF32` re-exported as `LpsValue`,
   `TextureStorageFormat`, `TextureBuffer`. Plus an optional
   `schemars` feature on `lps-shared` for `LpsType`.
5. **schemars discipline** — every public type derives
   `JsonSchema`; recursive `Shape`/`Slot` round-trip is verified
   in M2 (early surfacing of any schemars issues).
6. **Tests** — path parsing, `Binding` serde round-trip,
   recursive `Slot`/`Shape` serde round-trip on hand-built
   values, `Kind::storage()` exhaustive check,
   `schemars::schema_for!` on every public type.

Out of scope (deferred to later milestones):

- Visual artifact types (M3).
- Audio / Beat / Touch / Motion / AudioFft / AudioLevel Kinds —
  added in M3 when first example demands them.
- TOML grammar for `Slot` (`shape = "scalar" | "array" |
  "struct"`, `[params]` implicit struct, `props`/`element`
  keywords) — M3.
- Schema codegen tooling (`lp-cli schema generate`, drift gates)
  — M4. The derives themselves land in M2.
- Migration registry implementation beyond trait shape — M5.
- `LpFs`-based artifact loader — M3.
- `BindingResolver` real impl — M3+.
- Render hookup, runtime behavior of any kind.
- Q32 specialization — F32-only for now.
- `lp-cli` wiring of any kind.

## File structure

```
lp-domain/
└── lp-domain/
    ├── Cargo.toml                          # NEW
    └── src/
        ├── lib.rs                          # NEW (re-exports + public surface)
        ├── types.rs                        # NEW (Uid, Name, NodePath, PropPath, NodePropSpec, ArtifactSpec, ChannelName)
        ├── kind.rs                         # NEW (Kind, Dimension, Unit, Colorspace, InterpMethod, constants)
        ├── constraint.rs                   # NEW (Constraint enum)
        ├── shape.rs                        # NEW (Shape, Slot)
        ├── value_spec.rs                   # NEW (ValueSpec, TextureSpec, materialize stub)
        ├── binding.rs                      # NEW (Binding, BindingResolver trait stub)
        ├── presentation.rs                 # NEW (Presentation enum)
        ├── node/
        │   └── mod.rs                      # NEW (Node trait)
        ├── schema/
        │   └── mod.rs                      # NEW (Artifact, Migration traits; empty Registry)
        └── artifact/
            └── mod.rs                      # NEW (placeholder; M3 fills with parse/load)

lp-shader/
└── lps-shared/
    ├── Cargo.toml                          # UPDATE: add serde (always); add schemars (optional feature)
    └── src/
        └── types.rs                        # UPDATE: derive serde + cfg-attr JsonSchema on LpsType + StructMember

Cargo.toml                                  # UPDATE: add lp-domain/lp-domain to members + default-members; add schemars to workspace.dependencies
```

## Conceptual architecture

### Five-layer model

```
                ┌────────────────────────────────────────────────┐
                │                lp-domain (M2)                  │
                │                                                │
                │   Slot { shape, label, description, bind, present } │
                │     │                                          │
                │     ▼                                          │
                │   Shape ::= Scalar { kind, constraint, default: ValueSpec } │
                │           | Array  { element: Box<Slot>, length, default: Option<ValueSpec> } │
                │           | Struct { fields: Vec<(Name,Slot)>, default: Option<ValueSpec> } │
                │     │                                          │
                │     ▼                                          │
                │   Kind ──→ storage()             ──→ LpsType   │
                │        ──→ dimension()           ──→ Dimension │
                │        ──→ default_constraint()  ──→ Constraint│
                │        ──→ default_presentation()──→ Presentation │
                │        ──→ default_bind()        ──→ Option<Binding> │
                │     │                                          │
                │     ▼                                          │
                │   ValueSpec ::= Literal(LpsValue)              │
                │              | Texture(TextureSpec)            │
                │                                                │
                └─────────────────┬──────────────────────────────┘
                                  │ depends on
                                  ▼
                ┌────────────────────────────────────────────────┐
                │            lps-shared (extended in M2)         │
                │   LpsType (+ serde + opt schemars)             │
                │   LpsValueF32 (re-exported as LpsValue)        │
                │   TextureStorageFormat, TextureBuffer          │
                └────────────────────────────────────────────────┘
```

### Identity & addressing types

```
Uid(u32)                            — runtime-only handle, Copy + Eq + Hash + Ord + Display
Name(String)                        — `[A-Za-z0-9_]+`, used in path segments
NodePath(Vec<NodePathSegment>)      — slash-joined `<name>.<type>` segments
NodePathSegment { name: Name, ty: Name }
PropPath = lps_shared::path         — `field.foo[0].x` (re-exported)
NodePropSpec { node: NodePath, prop: PropPath }   — joined by `#` in display form
ArtifactSpec(String)                — opaque v0 (file-relative); parsing in M3
ChannelName(String)                 — `<kind>/<dir>/<channel>[/<sub>...]` convention only
```

### Trait surface (signatures only, no concrete impls beyond test stubs)

```
Node          : { fn uid() -> Uid; fn path() -> &NodePath;
                 fn get_property(&self, &PropPath) -> Result<LpsValue>;
                 fn set_property(&mut self, &PropPath, LpsValue) -> Result<()> }

Artifact      : { const KIND: &'static str; const CURRENT_VERSION: u32 }
                 (M5 will add DeserializeOwned + JsonSchema bound)

Migration     : { const KIND: &'static str; const FROM: u32;
                  fn migrate(value: &mut toml::Value) }

BindingResolver : trait stub for compose-time validation (M3+ implements)

Registry      : empty struct, methods come in M5
```

### Key interactions

1. **Slot storage projection.** `Slot::storage() -> LpsType` walks
   the `Shape`. `Scalar { kind, .. }` → `kind.storage()`;
   `Array { element, length, .. }` → `LpsType::Array { element:
   Box::new(element.storage()), len: *length }`;
   `Struct { fields, .. }` → `LpsType::Struct { name: None,
   members: <Slot.storage() per field> }`. This is the bridge to
   GPU truth.

2. **Default resolution (Q15 — Option A).** Every `Slot` *can
   produce* a default. Scalar Shapes carry a mandatory
   `default: ValueSpec`. Composed Shapes carry an
   `Option<ValueSpec>` override:

   ```rust
   impl Slot {
       pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue {
           match &self.shape {
               Shape::Scalar { default, .. } => default.materialize(ctx),
               Shape::Array { element, length, default } => match default {
                   Some(d) => d.materialize(ctx),
                   None => LpsValue::Array(
                       (0..*length).map(|_| element.default_value(ctx)).collect(),
                   ),
               },
               Shape::Struct { fields, default } => match default {
                   Some(d) => d.materialize(ctx),
                   None => LpsValue::Struct {
                       name: None,
                       fields: fields.iter()
                           .map(|(n, s)| (n.0.clone(), s.default_value(ctx)))
                           .collect(),
                   },
               },
           }
       }
   }
   ```

   Round-trip parity is automatic via
   `#[serde(skip_serializing_if = "Option::is_none")]` on the
   composed-default fields.

3. **Binding resolution order.** Slot's explicit `bind` →
   `Kind::default_bind()` → fall back to default. Encoded as a
   small helper on `Slot`; the real `BindingResolver` trait
   (compose-time channel-Kind validation) is just a stub in M2.

4. **schemars discipline.** Every public type derives
   `JsonSchema`. Recursive `Shape`/`Slot` get an M2 smoke test:
   `schemars::schema_for!(Slot)` must succeed without panic and
   produce a non-degenerate schema. If it chokes, the documented
   fallback chain kicks in (manual impl → hand-written → drop
   schema generation).

5. **`lps-shared` extension.** `LpsType` and `StructMember` get
   serde `Serialize/Deserialize` always-on (needed because
   `Shape::Scalar` embeds `LpsType` in the constraint plumbing).
   `JsonSchema` is opt-in via `lps-shared`'s new `schemars`
   feature so int-only firmware doesn't pay for it.

## Notable design decisions (carried into the implementation)

These come from the user's chat answers + `00-notes.md`:

- **`Uid` is `u32`, not a base-62 string** — embedded perf.
  Runtime-only; M2 doesn't define an allocator.
- **Shape default field placement** (Q15 — Option A) — composed
  Shape variants carry `default: Option<ValueSpec>` for round-
  trip fidelity; scalar carries mandatory `ValueSpec`. Slot loses
  its top-level `default` field.
- **No serde on `LpsValueF32`** — defaults flow through
  `ValueSpec`, which serializes its own way; raw `LpsValue` serde
  isn't needed for M2 and would force decisions that belong to M3.
- **Tests inline** — M2 is design surface; integration tests come
  in M3.
- **No `lp-cli` wiring** — schemars derives land here, but
  `lp-cli schema generate` is M4's job.

## Phase split

```
1. lps-shared extension                                  [sub-agent: yes,  parallel: -]
2. lp-domain crate skeleton + identity types             [sub-agent: yes,  parallel: -]   (depends on 1)
3. Quantity model leaves: kind + constraint              [sub-agent: yes,  parallel: 4]   (depends on 2)
4. Trait surface: node + schema + artifact               [sub-agent: yes,  parallel: 3]   (depends on 2)
5. Quantity model composition: shape + slot + value_spec + binding + presentation  [sub-agent: yes, parallel: -]   (depends on 3+4)
6. schemars verification + cargo-check matrix            [sub-agent: yes,  parallel: -]   (depends on 5)
7. Cleanup, validation, and quantity.md edit             [sub-agent: supervised]
```

Phase 5 is the heaviest; it intentionally bundles the composition
types because they all share one mental model and splitting forces
two sub-agents to duplicate context.

Phase 7 owns the `quantity.md` §6 + §12 edits per Q15 and writes
`summary.md`.
