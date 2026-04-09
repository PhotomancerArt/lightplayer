## Phase 6: `filetest_lpvm` wire-up

### Scope

Integrate `lpvm-native` with `emu` feature into filetest execution.

**`lps-filetests/Cargo.toml`:**
```toml
lpvm-native = { path = "../lpvm-native", optional = true, features = ["emu"] }
```

**`test_run/filetest_lpvm.rs`:**
- Add `CompiledShader::Native(NativeEmuModule)` variant
- Add `FiletestInstance::Native(NativeEmuInstance)` variant
- Implement `module_sig()`, `instantiate()`, `call()`, `call_q32_flat()`, `debug_state()` on new variants
- In `compile_glsl()`, add match arm for `Backend::Rv32lp`:
  ```rust
  Backend::Rv32lp => {
      let opts = NativeCompileOptions { float_mode: fm, .. };
      let engine = NativeEmuEngine::new(opts);
      Ok(CompiledShader::Native(engine.compile(&ir, &meta)?))
  }
  ```

### Code organization

Follow existing pattern for `Jit` and `Emu` variants.

### Tests

```bash
cargo check -p lps-filetests
cargo test -p lps-filetests --lib
```
