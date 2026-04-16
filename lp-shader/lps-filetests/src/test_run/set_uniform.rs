//! Apply `// set_uniform:` directives to a filetest instance.

use anyhow::Result;

use lps_shared::LpsModuleSig;

use crate::parse::SetUniform;
use crate::test_run::filetest_lpvm::FiletestInstance;
use crate::test_run::parse_assert::parse_glsl_value;

/// Write uniform values from directives into the instance VMContext (uniforms region).
pub fn apply_set_uniforms(
    inst: &mut FiletestInstance,
    sig: &LpsModuleSig,
    uniforms: &[SetUniform],
) -> Result<()> {
    if uniforms.is_empty() {
        return Ok(());
    }
    if sig.uniforms_type.is_none() {
        anyhow::bail!("set_uniform: module has no uniforms_type metadata");
    }

    for u in uniforms {
        let val = parse_glsl_value(&u.value_str)
            .map_err(|e| anyhow::anyhow!("set_uniform `{}`: parse value: {e}", u.name))?;
        inst.set_uniform(&u.name, &val)
            .map_err(|e| anyhow::anyhow!("set_uniform `{}`: {e}", u.name))?;
    }
    Ok(())
}
