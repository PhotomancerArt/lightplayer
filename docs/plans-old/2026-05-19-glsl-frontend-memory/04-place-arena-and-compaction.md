# Phase 4: Arena-Backed Places And Compact Segments

## Scope Of Phase

Move assignable places into arena IDs and compact obvious place metadata.

In scope:

- Add `PlaceId` storage to `HirArena`.
- Change assign/read/inc-dec/writeback nodes to store `PlaceId`.
- Change `PlaceSegment::Index` to store `ExprId` instead of `Box<HirExpr>`.
- Compact swizzle lane storage.
- Narrow lane and byte offset integer types where the value ranges are naturally small.
- Update typechecking and lowering to use `PlaceId`.

Out of scope:

- Removing all strings from places.
- Full identifier interning.
- Reworking texture binding semantics.
- Replacing recursive typechecking with an explicit worklist.

## Code Organization Reminders

- Keep place typing in `lp-shader/lps-glsl/src/hir/place.rs` and `typeck.rs`.
- Keep place lowering in `lp-shader/lps-glsl/src/lower/place/*` and `lower/ops/place_*`.
- If a compact lane-list type is added, place it near arena/place concepts and reuse it for expression swizzles where possible.
- Put tests at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-shader/lps-glsl/src/hir/arena.rs`
- `lp-shader/lps-glsl/src/hir/types.rs`
- `lp-shader/lps-glsl/src/hir/place.rs`
- `lp-shader/lps-glsl/src/hir/typeck.rs`
- `lp-shader/lps-glsl/src/hir/builtin_out.rs`
- `lp-shader/lps-glsl/src/lower.rs`
- `lp-shader/lps-glsl/src/lower/place/*`
- `lp-shader/lps-glsl/src/lower/ops/place_*`

Expected arena API additions:

```rust
impl HirArena {
    pub fn push_place(&mut self, place: HirPlace) -> PlaceId;
    pub fn place(&self, id: PlaceId) -> &HirPlace;
    pub fn place_mut(&mut self, id: PlaceId) -> &mut HirPlace;
}
```

Change expression variants:

```rust
HirExprKind::PlaceRead { target: PlaceId }
HirExprKind::Assign { target: PlaceId, value: ExprId }
HirExprKind::IncDec { target: PlaceId, op: IncDecOp, prefix: bool }
```

Change writebacks:

```rust
pub struct HirUserCallWriteback {
    pub arg_index: usize,
    pub target: PlaceId,
    pub ty: LpsType,
    pub copy_in: bool,
}
```

Change place segments:

```rust
pub enum PlaceSegment {
    Field {
        name: String,
        ty: LpsType,
        lane_offset: u8,
        lane_count: u8,
        byte_offset: u16,
    },
    Swizzle {
        fields: String,
        lanes: SmallLaneList,
        ty: LpsType,
    },
    Index {
        index: ExprId,
        ty: LpsType,
    },
}
```

Add a compact lane-list type:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmallLaneList {
    len: u8,
    lanes: [u8; 4],
}
```

It should support:

- construction from `&[usize]` or an iterator with diagnostics.
- iteration as `usize`.
- slice-like access for lowering.

Keep `fields: String` for swizzles for now if diagnostics still use it. If no production path uses the string after typing, removing it is allowed, but only with tests proving diagnostic/texture behavior remains good.

Keep `Field { name: String }` for now because texture operand lowering builds sampler paths from field names. A future symbol/path representation can revisit this.

Check integer narrowing carefully:

- `lane_offset` and `lane_count` should fit in `u8` because scalar/vector lanes are small.
- `byte_offset` may fit in `u16` for current GLSL aggregate layout, but use checked conversion with a diagnostic or internal error if the value is larger.
- Do not silently truncate any offset.

Expected outcome:

- `HirExprKind` no longer contains inline `HirAssignTarget`.
- `HirAssignTarget` may disappear or become a compatibility alias around `PlaceId`.
- Indexed places no longer allocate a boxed expression.
- Swizzles no longer allocate a lane `Vec`.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If hardware is attached:

```bash
just demo-esp32c6-check basic
```

Update `measurements.md` with:

- New `HirExpr` / `HirExprKind` / `HirPlace` sizes if those names still exist.
- New stack hotspots.
- Device memory trace summary.
