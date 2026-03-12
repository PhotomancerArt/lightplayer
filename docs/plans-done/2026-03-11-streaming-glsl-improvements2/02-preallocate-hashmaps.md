# Phase 2: Pre-allocate HashMaps

## Problem

The streaming function builds several HashMaps that grow incrementally as
functions are declared:

- `func_id_map: HashMap<String, FuncId>` — grows per function
- `old_func_id_map: HashMap<FuncId, String>` — grows per function
- `float_func_ids: HashMap<String, FuncId>` — grows per function
- `glsl_signatures: HashMap` — grows per function
- `cranelift_signatures: HashMap` — grows per function
- `jit_func_id_map: HashMap` — grows per function

With 11 functions + builtins, each HashMap rehashes multiple times as it grows.
The trace shows 6,488 bytes in 6 `RawTable::reserve_rehash` events under
`glsl_jit_streaming`, vs 660 bytes (1 rehash) in the batch path.

## Fix

Pre-allocate all HashMaps with known capacity:

```rust
let num_functions = sorted_names.len();
let num_builtins = BuiltinId::all().count();
let total_capacity = num_functions + num_builtins;

let mut func_id_map: HashMap<String, FuncId> = HashMap::with_capacity(total_capacity);
let mut old_func_id_map: HashMap<FuncId, String> = HashMap::with_capacity(total_capacity);
let mut float_func_ids: HashMap<String, FuncId> = HashMap::with_capacity(num_functions);
// ...
let mut glsl_signatures = HashMap::with_capacity(num_functions);
let mut cranelift_signatures = HashMap::with_capacity(num_functions);
```

Also pre-allocate the `sorted_functions` Vec:

```rust
let mut sorted_functions: Vec<StreamingFuncInfo<'_>> = Vec::with_capacity(num_functions);
```

## Expected savings

~5-6 KB from avoiding rehashes. Each rehash allocates a new backing array and
the old one is freed, but at peak both may be alive (the new array is allocated
before the old one is freed).

## Risk

Low. Pure optimization, no behavioral change.
