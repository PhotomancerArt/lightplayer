# Function Declarations and Calls

## Function declarations

LPIR distinguishes three declaration forms.

### 1. Imported functions

```
import @module::name(param_types) -> return_type
```

The callee name is module-qualified: the `::` separator distinguishes imports from local functions. The module prefix (`std.math`, `lp.q32`, `lpfx`, and others) tells the emitter which provider resolves the import.

### 2. Local functions

```
func @name(params) -> return_type { body }
```

### 3. Entry function

```
entry func @name(params) -> return_type { body }
```

`entry` marks the runtime entry point: the function the host invokes as the shader. A module has at most one entry function.

All functions (entry or not) are visible and callable by the host in JIT and test contexts. Whether a symbol is exported or how it is exposed is an emitter concern (for example, WASM may export all defined functions; a Cranelift JIT may expose all symbols). LPIR does not carry a separate visibility annotation for this.

## Call operation

A single `call` opcode is used for both local and imported callees.

Examples (text form — **user** operands only; VM context `v0:ptr` is implicit for callees that use it):

```
v5:f32 = call @my_helper(v1, v2)                    ; local, single return
call @void_func(v1, v2)                              ; void
v5:f32, v6:f32, v7:f32 = call @vec3_fn(v1)          ; multi-return
v8:f32 = call @std.math::fsin(v1)                   ; import: first arg is the user value, not v0
```

The internal `call` opcode’s argument list begins with `v0` when the callee’s convention requires VM context (all local shader functions; imports only when `needs_vmctx` is set). The text parser inserts that operand; the printer hides it so authors never duplicate `v0` in parentheses.

Import calls use the full qualified name at the call site, matching the `import` declaration.

## Multi-return

Multi-return is supported for scalarized vector and matrix results. There is no fixed upper bound on tuple arity in the IR.

Target mapping:

| Target     | Behavior |
|------------|----------|
| WebAssembly | Multi-value returns (core spec). |
| Cranelift  | Multi-return for small arities; larger arities may use `StructReturn` or an equivalent ABI. Each backend’s `GlslExecutable` defines the concrete ABI. |

If a target cannot represent the required return arity, the emitter reports an error; it does not silently truncate or drop components. In typical scalarized GLSL, tuple sizes stay small (for example, `vec4` → four `f32`, `mat4` → sixteen `f32`).

## Recursion

Call graphs may be cyclic; recursion is permitted. GLSL 4.50 core permits recursion. Stack overflow from unbounded recursion is implementation-defined termination, not undefined behavior. Embedders may enforce limits (fuel, stack depth, timeouts).

## Import resolution

The emitter is configured with **providers** keyed by import module name. Examples:

| Module    | Role |
|-----------|------|
| `std.math` | Standard math builtins; WASM may lower to browser `libm`-style imports; Cranelift may use libcalls or intrinsics. |
| `lp.q32`   | Q32 fixed-point helpers; available only when the emitter runs in Q32 mode. |
| `lpfx`     | LPFX (Lygia) builtins; available only when an LPFX provider is configured. |

If a module required by the IR has no provider, the emitter reports an error. If a call’s argument or result types do not match the imported signature, the emitter reports an error.
