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

#[test]
fn test_unary_minus() {
    let source = r#"
        int test_neg() {
            return 10 + (-4);
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_neg")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, ()).expect("call");
    assert_eq!(result, 6);
}

#[test]
fn test_int_mul_and_assignment() {
    let source = r#"
        int test_mul_assign() {
            int x = 3;
            x = x * 4;
            return x;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_mul_assign")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, ()).expect("call");
    assert_eq!(result, 12);
}

#[test]
#[ignore = "Q32 mul: wasm validation error - expected i32 found i64; needs investigation"]
fn test_q32_float_mul() {
    let source = r#"
        float main() {
            float a = 2.0;
            float b = 3.0;
            return a * b;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "main")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, ()).expect("call");
    let expected: i32 = 6 * 65536;
    assert_eq!(result, expected);
}

#[test]
fn test_q32_float_add() {
    let source = r#"
        float main() {
            float a = 2.0;
            float b = 3.0;
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
        .get_func(&mut store, "main")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let result = func.call(&mut store, ()).expect("call");
    let expected: i32 = 5 * 65536;
    assert_eq!(result, expected);
}

#[test]
fn test_scalar_constructor_int() {
    let source = r#"
        int test_int() {
            return int(42);
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_int")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 42);
}

#[test]
fn test_ternary() {
    let source = r#"
        int test_ternary(int x) {
            return x > 0 ? 10 : 20;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_ternary")
        .expect("get_func")
        .typed::<i32, i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, 1).expect("call"), 10);
    assert_eq!(func.call(&mut store, -1).expect("call"), 20);
}

#[test]
fn test_logical_and_or() {
    let source = r#"
        bool test_and(bool a, bool b) {
            return a && b;
        }
        bool test_or(bool a, bool b) {
            return a || b;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let and_func = instance
        .get_func(&mut store, "test_and")
        .expect("get_func")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed");
    let or_func = instance
        .get_func(&mut store, "test_or")
        .expect("get_func")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed");
    assert_eq!(and_func.call(&mut store, (1, 1)).expect("call"), 1);
    assert_eq!(and_func.call(&mut store, (0, 1)).expect("call"), 0);
    assert_eq!(or_func.call(&mut store, (0, 0)).expect("call"), 0);
    assert_eq!(or_func.call(&mut store, (1, 0)).expect("call"), 1);
}

#[test]
fn test_xor() {
    let source = r#"
        bool test_xor(bool a, bool b) {
            return a ^^ b;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_xor")
        .expect("get_func")
        .typed::<(i32, i32), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, (1, 0)).expect("call"), 1);
    assert_eq!(func.call(&mut store, (0, 1)).expect("call"), 1);
    assert_eq!(func.call(&mut store, (1, 1)).expect("call"), 0);
}

#[test]
fn test_if_else() {
    let source = r#"
        int test_if(int x) {
            int r = 20;
            if (x > 0) {
                r = 10;
            }
            return r;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_if")
        .expect("get_func")
        .typed::<i32, i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, 1).expect("call"), 10);
    assert_eq!(func.call(&mut store, -1).expect("call"), 20);
}

#[test]
fn test_if_else_both_branches() {
    let source = r#"
        int test_if_else(int x) {
            if (x > 0)
                return 10;
            else
                return 20;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_if_else")
        .expect("get_func")
        .typed::<i32, i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, 1).expect("call"), 10);
    assert_eq!(func.call(&mut store, -1).expect("call"), 20);
}

#[test]
fn test_for_loop() {
    let source = r#"
        int test_for() {
            int sum = 0;
            for (int i = 0; i < 5; i = i + 1) {
                sum = sum + i;
            }
            return sum;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_for")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 10);
}

#[test]
fn test_while_loop() {
    let source = r#"
        int test_while() {
            int n = 5;
            int sum = 0;
            while (n > 0) {
                sum = sum + n;
                n = n - 1;
            }
            return sum;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_while")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 15);
}

#[test]
fn test_break_continue() {
    let source = r#"
        int test_break() {
            int sum = 0;
            for (int i = 0; i < 10; i = i + 1) {
                if (i >= 5) break;
                sum = sum + i;
            }
            return sum;
        }
        int test_continue() {
            int sum = 0;
            for (int i = 0; i < 10; i = i + 1) {
                if (i % 2 == 0) continue;
                sum = sum + i;
            }
            return sum;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let break_func = instance
        .get_func(&mut store, "test_break")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    let cont_func = instance
        .get_func(&mut store, "test_continue")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(break_func.call(&mut store, ()).expect("call"), 10);
    assert_eq!(cont_func.call(&mut store, ()).expect("call"), 25);
}

#[test]
fn test_user_function_call() {
    let source = r#"
        int add(int a, int b) {
            return a + b;
        }
        int test_call() {
            return add(3, 7);
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_call")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 10);
}

#[test]
fn test_void_function_call() {
    let source = r#"
        void noop() {
        }
        int test_void() {
            noop();
            return 1;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_void")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 1);
}

#[test]
fn test_vec2_constructor() {
    let source = r#"
        vec2 make_vec2() {
            return vec2(1.0, 2.0);
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "make_vec2")
        .expect("get_func")
        .typed::<(), (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, ()).expect("call");
    assert!((x - 1.0).abs() < 1e-5);
    assert!((y - 2.0).abs() < 1e-5);
}

#[test]
fn test_vec2_broadcast() {
    let source = r#"
        vec2 broadcast(float s) {
            return vec2(s);
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "broadcast")
        .expect("get_func")
        .typed::<f32, (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, 3.0).expect("call");
    assert!((x - 3.0).abs() < 1e-5);
    assert!((y - 3.0).abs() < 1e-5);
}

#[test]
fn test_vec3_from_vec2_scalar() {
    let source = r#"
        vec3 make_vec3(float x, float y, float z) {
            return vec3(vec2(x, y), z);
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "make_vec3")
        .expect("get_func")
        .typed::<(f32, f32, f32), (f32, f32, f32)>(&store)
        .expect("typed");
    let (x, y, z) = func.call(&mut store, (1.0, 2.0, 3.0)).expect("call");
    assert!((x - 1.0).abs() < 1e-5);
    assert!((y - 2.0).abs() < 1e-5);
    assert!((z - 3.0).abs() < 1e-5);
}

#[test]
fn test_const_variable() {
    let source = r#"
        int test_const() {
            const int n = 5;
            return n;
        }
    "#;
    let options = WasmOptions::default();
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "test_const")
        .expect("get_func")
        .typed::<(), i32>(&store)
        .expect("typed");
    assert_eq!(func.call(&mut store, ()).expect("call"), 5);
}

#[test]
fn test_vec2_component_access() {
    let source = r#"
        float get_y(vec2 v) {
            return v.y;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "get_y")
        .expect("get_func")
        .typed::<(f32, f32), f32>(&store)
        .expect("typed");
    let result = func.call(&mut store, (1.0, 7.5)).expect("call");
    assert!((result - 7.5).abs() < 1e-5);
}

#[test]
fn test_vec3_swizzle() {
    let source = r#"
        vec2 get_yx(vec3 v) {
            return v.yx;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "get_yx")
        .expect("get_func")
        .typed::<(f32, f32, f32), (f32, f32)>(&store)
        .expect("typed");
    let (a, b) = func.call(&mut store, (1.0, 2.0, 3.0)).expect("call");
    assert!((a - 2.0).abs() < 1e-5);
    assert!((b - 1.0).abs() < 1e-5);
}

#[test]
fn test_vec2_add_via_components() {
    let source = r#"
        vec2 add_vec2(vec2 a, vec2 b) {
            return vec2(a.x + b.x, a.y + b.y);
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_vec2")
        .expect("get_func")
        .typed::<(f32, f32, f32, f32), (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, (1.0, 2.0, 3.0, 4.0)).expect("call");
    assert!((x - 4.0).abs() < 1e-5);
    assert!((y - 6.0).abs() < 1e-5);
}

#[test]
fn test_vec2_add_direct() {
    let source = r#"
        vec2 add_direct(vec2 a, vec2 b) {
            return a + b;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_direct")
        .expect("get_func")
        .typed::<(f32, f32, f32, f32), (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, (1.0, 2.0, 3.0, 4.0)).expect("call");
    assert!((x - 4.0).abs() < 1e-5);
    assert!((y - 6.0).abs() < 1e-5);
}

#[test]
fn test_vec2_scalar_mul() {
    let source = r#"
        vec2 scale(vec2 v, float s) {
            return v * s;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "scale")
        .expect("get_func")
        .typed::<(f32, f32, f32), (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, (2.0, 3.0, 4.0)).expect("call");
    assert!((x - 8.0).abs() < 1e-5);
    assert!((y - 12.0).abs() < 1e-5);
}

#[test]
fn test_vec2_equal() {
    let source = r#"
        bool vec2_equal(vec2 a, vec2 b) {
            return a == b;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "vec2_equal")
        .expect("get_func")
        .typed::<(f32, f32, f32, f32), i32>(&store)
        .expect("typed");
    assert_eq!(
        func.call(&mut store, (1.0, 2.0, 1.0, 2.0)).expect("call"),
        1
    );
    assert_eq!(
        func.call(&mut store, (1.0, 2.0, 1.0, 3.0)).expect("call"),
        0
    );
}

#[test]
fn test_vec2_compound_assignment() {
    let source = r#"
        vec2 add_accum(vec2 a, vec2 b) {
            vec2 r = a;
            r = r + b;
            return r;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "add_accum")
        .expect("get_func")
        .typed::<(f32, f32, f32, f32), (f32, f32)>(&store)
        .expect("typed");
    let (x, y) = func.call(&mut store, (1.0, 2.0, 3.0, 4.0)).expect("call");
    assert!((x - 4.0).abs() < 1e-5);
    assert!((y - 6.0).abs() < 1e-5);
}

#[test]
fn test_vec2_not_equal() {
    let source = r#"
        bool vec2_ne(vec2 a, vec2 b) {
            return a != b;
        }
    "#;
    let options = WasmOptions {
        float_mode: lp_glsl_frontend::FloatMode::Float,
        ..Default::default()
    };
    let module = glsl_wasm(source, options).expect("compile");
    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &module.bytes).expect("wasm module");
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).expect("instantiate");
    let func = instance
        .get_func(&mut store, "vec2_ne")
        .expect("get_func")
        .typed::<(f32, f32, f32, f32), i32>(&store)
        .expect("typed");
    assert_eq!(
        func.call(&mut store, (1.0, 2.0, 1.0, 2.0)).expect("call"),
        0
    );
    assert_eq!(
        func.call(&mut store, (1.0, 2.0, 1.0, 3.0)).expect("call"),
        1
    );
}
