//! Basic unit tests for lp-glsl-wasm using wasmtime.

use lp_glsl_wasm::{WasmOptions, glsl_wasm};

#[test]
fn test_compile_int_add() {
    let source = r#"
        int test_add() {
            return 1 + 2;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    assert!(!module.bytes.is_empty());
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0].name, "test_add");
    assert!(module.exports[0].params.is_empty());
    assert_eq!(module.exports[0].results.len(), 1);

    // Execute with wasmtime
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_add")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, ()).expect("call");
    assert_eq!(result, 3);
}

#[test]
fn test_compile_add_params() {
    let source = r#"
        int add_ints(int a, int b) {
            return a + b;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_ints")
        .expect("get_func")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, (10, 20)).expect("call");
    assert_eq!(result, 30);
}
