# GLSL Fast Math Mode - Phases

1. **01-add-glsl-opts-and-shader-config** - Create GlslOpts, add to ShaderConfig
2. **02-add-fast-math-to-glsl-options-and-transform** - GlslOptions.fast_math, Q32Transform.fast_math, wire through compiler
3. **03-implement-fast-math-in-arithmetic-converters** - Conditional iadd/isub in convert_fadd/convert_fsub
4. **04-use-glsl-opts-in-shader-runtime** - ShaderRuntime builds GlslOptions from config.glsl_opts
5. **05-enable-fast-math-in-esp32-demo** - Add glsl_opts to rainbow.shader node.json
6. **06-cleanup-and-validation** - Cleanup, full check/test, summary, commit
