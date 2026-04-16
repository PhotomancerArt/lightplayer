# GLSL → WASM Playground: Phases

## i. Extract lps-frontend

Mechanical refactor. Move shared frontend code out of lps-compiler
into lps-frontend. Rename lps-compiler to lps-cranelift.
Update all internal imports. All existing tests pass unchanged.
No new functionality.

## ii. WASM codegen foundation + filetest infrastructure

Create lps-wasm with scaffolding: module builder, codegen context,
trivial shader support. Modularize filetest runner for multiple runtimes.
Add wasmtime-based runner. First filetest passing on WASM backend.

## iii. WASM codegen: rainbow shader feature completeness

Incrementally add GLSL features to the WASM codegen until rainbow.shader
compiles and runs correctly via wasmtime. Guided by filetests.

## iv. Playground: builtins WASM + web page + end-to-end demo

Compile lps-builtins to .wasm. Build playground wasm-pack project.
Create HTML page. Wire up execution loop. Rainbow shader renders in
browser.
