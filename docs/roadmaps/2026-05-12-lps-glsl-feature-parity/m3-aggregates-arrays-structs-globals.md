# M3: Aggregates, Arrays, Structs, and Globals

## Objective

Add the aggregate type system needed for real GLSL programs: arrays, structs, nested access, and mutable globals where filetests require them.

## Features

- array types and constructors
- fixed-size array indexing
- struct declarations
- struct constructors
- member access
- arrays of structs and structs containing arrays as required by filetests
- global variables beyond constants
- const/global initialization rules used by the corpus
- nested lvalues combining member, index, and swizzle paths

## Implementation Notes

This milestone is mostly a type and storage representation milestone.

The HIR should represent aggregate values explicitly enough for semantic checks, while lowering can choose a flattened scalar/vector storage strategy when emitting LPIR. Keep the flattening in lowering, not in syntax.

Add a type registry or equivalent semantic context for:

- named structs
- array element/count metadata
- aggregate field layout
- constructor compatibility

Nested lvalue support is the key risk. Once a target like `foo.items[i].color.xy` can be typed and lowered as a path, most aggregate assignment follows naturally.

## Filetest Gate

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise array struct global const uniform
```

Keep earlier slices live:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl scalar vec operators control function lpfn
```

## Done

- array and struct filetests pass or have explicit, justified skips
- nested aggregate reads and writes work
- global initialization behavior matches the existing frontend for supported cases
- aggregate lowering remains small enough for firmware use

