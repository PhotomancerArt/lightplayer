# Phase 6: CLI and Integration

## Scope

Wire up new pipeline to `shader-rv32fa` command and add basic CLI.

## Implementation

### 1. Add `shader-rv32fa` command to lp-cli

Update `lp-cli/src/commands/shader_rv32/handler.rs`:

```rust
pub fn run(args: &ShaderRv32Args) -> Result<(), NativeError> {
    match args.backend {
        ShaderBackend::Linear => run_linear(args),
        ShaderBackend::Fast => run_fast(args), // New
    }
}

fn run_fast(args: &ShaderRv32Args) -> Result<(), NativeError> {
    use lpvm_native::isa::rv32fa::debug::physinst;
    use lpvm_native::isa::rv32fa::alloc::allocate;
    use lpvm_native::isa::rv32fa::emit::PhysEmitter;

    // Parse and lower
    let lowered = parse_and_lower(args)?;

    if args.show_vinst {
        eprintln!("=== VInst ===");
        for inst in &lowered.vinsts {
            eprintln!("{}", lpvm_native::debug::vinst::format_vinst(inst));
        }
    }

    // Allocate (straight-line only for now)
    let physinsts = allocate(&lowered.vinsts, lowered.is_sret, lowered.params, lowered.returns)?;

    if args.show_physinst {
        eprintln!("=== PhysInst ===");
        for inst in &physinsts {
            eprintln!("{}", physinst::format(inst));
        }
    }

    // Emit
    let mut emitter = PhysEmitter::new();
    for inst in &physinsts {
        emitter.emit(inst);
    }
    let code = emitter.finish();

    // Output disassembly
    if args.disassemble {
        disassemble_fast(&code)?;
    }

    Ok(())
}
```

### 2. Update args

```rust
#[derive(Clone, Copy, Debug, Default, clap::ValueEnum)]
pub enum ShaderBackend {
    #[default]
    Linear,
    Fast,
}

#[derive(Args)]
pub struct ShaderRv32Args {
    #[arg(short, long, value_enum)]
    pub backend: Option<ShaderBackend>,

    #[arg(long)]
    pub show_vinst: bool,

    #[arg(long)]
    pub show_physinst: bool,  // New

    #[arg(short, long)]
    pub disassemble: bool,

    pub input: PathBuf,
}
```

### 3. Add disassembler for fast output

```rust
fn disassemble_fast(code: &[u8]) -> Result<(), NativeError> {
    use riscv_disassembler::disassemble;
    let text = disassemble(code, 0)?;
    eprintln!("=== Disassembly ===");
    eprintln!("{}", text);
    Ok(())
}
```

## Validate

```bash
cargo check -p lp-cli
cargo run -p lp-cli -- shader-rv32 --backend fast debug1.glsl --show-vinst --show-physinst
```
