use alloc::format;
use alloc::string::String;

use lpir::{CompilerConfig, LpirModule};
use lps_shared::{LpsModuleSig, TextureStorageFormat};
use lpvm::{LpvmCompileBudget, LpvmCompileStepResult, LpvmEngine};

use crate::compile_px_desc::{CompilePxDesc, ShaderFrontend, TextureBindingSpecs};
use crate::error::LpsError;
use crate::px_shader::LpsPxShader;

#[derive(Debug, Clone, Copy)]
pub struct ShaderCompileBudget {
    pub frontend_steps: usize,
    pub backend_steps: usize,
}

impl Default for ShaderCompileBudget {
    fn default() -> Self {
        Self {
            frontend_steps: usize::MAX,
            backend_steps: usize::MAX,
        }
    }
}

impl ShaderCompileBudget {
    pub const fn single_step() -> Self {
        Self {
            frontend_steps: 1,
            backend_steps: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderCompileStage {
    Frontend,
    Prepare,
    Backend,
    Done,
}

pub enum ShaderCompileStepResult {
    Pending,
    Finished(LpsPxShader),
    Failed(LpsError),
}

enum FrontendState<'src> {
    LpsGlsl(lps_glsl::CompileJob<'src>),
    Naga,
}

enum ShaderCompileState<'src, 'engine, E: LpvmEngine> {
    Frontend(FrontendState<'src>),
    Prepare {
        ir: LpirModule,
        meta: LpsModuleSig,
    },
    Backend {
        meta: LpsModuleSig,
        render_fn_index: usize,
        render_texture_fn_name: alloc::string::String,
        render_samples_fn_name: Option<alloc::string::String>,
        job: lpvm::BoxedLpvmCompileJob<'engine, E::Module, E::Error>,
    },
    Done,
}

pub struct ShaderCompileJob<'src, 'engine, E: LpvmEngine> {
    engine: &'engine E,
    glsl: &'src str,
    output_format: TextureStorageFormat,
    compiler_config: CompilerConfig,
    textures: TextureBindingSpecs,
    state: ShaderCompileState<'src, 'engine, E>,
}

impl<'src, 'engine, E: LpvmEngine> ShaderCompileJob<'src, 'engine, E>
where
    E::Module: 'static,
{
    pub fn new(engine: &'engine E, desc: CompilePxDesc<'src>) -> Self {
        let state = match desc.frontend {
            ShaderFrontend::LpsGlsl => {
                let options = lps_glsl::CompileOptions {
                    texture_specs: desc.textures.clone(),
                    texel_fetch_bounds: desc.compiler_config.texture.texel_fetch_bounds,
                };
                ShaderCompileState::Frontend(FrontendState::LpsGlsl(lps_glsl::CompileJob::new(
                    desc.glsl, options,
                )))
            }
            ShaderFrontend::Naga => ShaderCompileState::Frontend(FrontendState::Naga),
        };
        Self {
            engine,
            glsl: desc.glsl,
            output_format: desc.output_format,
            compiler_config: desc.compiler_config,
            textures: desc.textures,
            state,
        }
    }

    pub fn stage(&self) -> ShaderCompileStage {
        match self.state {
            ShaderCompileState::Frontend(_) => ShaderCompileStage::Frontend,
            ShaderCompileState::Prepare { .. } => ShaderCompileStage::Prepare,
            ShaderCompileState::Backend { .. } => ShaderCompileStage::Backend,
            ShaderCompileState::Done => ShaderCompileStage::Done,
        }
    }

    pub fn step(&mut self, budget: ShaderCompileBudget) -> ShaderCompileStepResult {
        let state = core::mem::replace(&mut self.state, ShaderCompileState::Done);
        match state {
            ShaderCompileState::Frontend(mut frontend) => match &mut frontend {
                FrontendState::LpsGlsl(job) => {
                    match job.step(lps_glsl::CompileBudget::steps(budget.frontend_steps)) {
                        lps_glsl::CompileStepResult::Pending => {
                            self.state = ShaderCompileState::Frontend(frontend);
                            ShaderCompileStepResult::Pending
                        }
                        lps_glsl::CompileStepResult::Failed(err) => {
                            ShaderCompileStepResult::Failed(LpsError::Parse(err.render(self.glsl)))
                        }
                        lps_glsl::CompileStepResult::Finished(output) => {
                            self.state = ShaderCompileState::Prepare {
                                ir: output.ir,
                                meta: output.meta,
                            };
                            ShaderCompileStepResult::Pending
                        }
                    }
                }
                FrontendState::Naga => match crate::engine::lower_glsl_with_naga(
                    self.glsl,
                    &self.textures,
                    &self.compiler_config,
                ) {
                    Ok((ir, meta)) => {
                        self.state = ShaderCompileState::Prepare { ir, meta };
                        ShaderCompileStepResult::Pending
                    }
                    Err(err) => ShaderCompileStepResult::Failed(err),
                },
            },
            ShaderCompileState::Prepare { mut ir, mut meta } => {
                if let Err(err) =
                    crate::texture_interface::validate_texture_interface(&meta, &self.textures)
                {
                    return ShaderCompileStepResult::Failed(err);
                }
                let render_fn_index =
                    match crate::engine::validate_render_sig(&meta, self.output_format) {
                        Ok(index) => index,
                        Err(err) => return ShaderCompileStepResult::Failed(err),
                    };
                let render_texture_fn_name = match crate::synth::synthesise_render_texture(
                    &mut ir,
                    &mut meta,
                    render_fn_index,
                    self.output_format,
                ) {
                    Ok(name) => name,
                    Err(err) => {
                        return ShaderCompileStepResult::Failed(LpsError::Compile(format!(
                            "synth render_texture: {err:?}"
                        )));
                    }
                };
                let render_samples_fn_name =
                    if self.output_format == TextureStorageFormat::Rgba16Unorm {
                        match crate::synth::synthesise_render_samples_rgba16(
                            &mut ir,
                            &mut meta,
                            render_fn_index,
                        ) {
                            Ok(name) => Some(name),
                            Err(err) => {
                                return ShaderCompileStepResult::Failed(LpsError::Compile(
                                    format!("synth render_samples: {err:?}"),
                                ));
                            }
                        }
                    } else {
                        None
                    };

                if let Some(job) = self.engine.start_compile_job(
                    ir.clone(),
                    meta.clone(),
                    self.compiler_config.clone(),
                ) {
                    self.state = ShaderCompileState::Backend {
                        meta,
                        render_fn_index,
                        render_texture_fn_name,
                        render_samples_fn_name,
                        job,
                    };
                    ShaderCompileStepResult::Pending
                } else {
                    match self
                        .engine
                        .compile_with_config(&ir, &meta, &self.compiler_config)
                    {
                        Ok(module) => {
                            match LpsPxShader::new(
                                module,
                                meta,
                                self.output_format,
                                render_fn_index,
                                render_texture_fn_name,
                                render_samples_fn_name,
                            ) {
                                Ok(shader) => ShaderCompileStepResult::Finished(shader),
                                Err(err) => ShaderCompileStepResult::Failed(LpsError::Compile(
                                    format!("{err}"),
                                )),
                            }
                        }
                        Err(err) => {
                            ShaderCompileStepResult::Failed(LpsError::Compile(format!("{err}")))
                        }
                    }
                }
            }
            ShaderCompileState::Backend {
                meta,
                render_fn_index,
                render_texture_fn_name,
                render_samples_fn_name,
                mut job,
            } => match job.step(LpvmCompileBudget::steps(budget.backend_steps)) {
                LpvmCompileStepResult::Pending => {
                    self.state = ShaderCompileState::Backend {
                        meta,
                        render_fn_index,
                        render_texture_fn_name,
                        render_samples_fn_name,
                        job,
                    };
                    ShaderCompileStepResult::Pending
                }
                LpvmCompileStepResult::Failed(err) => {
                    ShaderCompileStepResult::Failed(LpsError::Compile(format!("{err}")))
                }
                LpvmCompileStepResult::Finished(module) => {
                    match LpsPxShader::new(
                        module,
                        meta,
                        self.output_format,
                        render_fn_index,
                        render_texture_fn_name,
                        render_samples_fn_name,
                    ) {
                        Ok(shader) => ShaderCompileStepResult::Finished(shader),
                        Err(err) => {
                            ShaderCompileStepResult::Failed(LpsError::Compile(format!("{err}")))
                        }
                    }
                }
            },
            ShaderCompileState::Done => ShaderCompileStepResult::Failed(LpsError::Compile(
                String::from("compile job already finished"),
            )),
        }
    }
}
