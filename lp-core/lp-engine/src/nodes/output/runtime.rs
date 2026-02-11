use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::output::{OutputChannelHandle, OutputFormat, OutputProvider};
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_model::{
    nodes::output::{OutputConfig, OutputDriverOptionsConfig, OutputState},
    project::FrameId,
};
use lp_shared::DisplayPipelineOptions;
use lp_shared::fs::fs_event::FsChange;

/// Output node runtime
pub struct OutputRuntime {
    /// Channel data buffer (16-bit RGB, 3 u16s per pixel)
    channel_data: Vec<u16>,
    /// Output channel handle from provider (None until initialized)
    channel_handle: Option<OutputChannelHandle>,
    /// GPIO pin number
    pin: u32,
    /// Output config (None until set)
    config: Option<OutputConfig>,
    pub state: OutputState,
}

impl OutputRuntime {
    pub fn new() -> Self {
        Self {
            channel_data: Vec::new(),
            channel_handle: None,
            pin: 0,
            config: None,
            state: OutputState::new(FrameId::default()),
        }
    }

    /// Set the output config
    pub fn set_config(&mut self, config: OutputConfig) {
        self.config = Some(config);
    }

    /// Get mutable slice to channel data, extending if needed (16-bit channels)
    pub fn get_buffer_mut(&mut self, start_ch: u32, ch_count: u32) -> &mut [u16] {
        let end = (start_ch + ch_count) as usize;
        if end > self.channel_data.len() {
            self.channel_data.resize(end, 0);
        }
        &mut self.channel_data[start_ch as usize..end]
    }

    /// Get channel data (for state extraction). Returns u8 (high byte per channel) for client sync.
    pub fn get_channel_data(&self) -> Vec<u8> {
        self.channel_data.iter().map(|v| (v >> 8) as u8).collect()
    }

    /// Get the output config (for state extraction)
    pub fn get_config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

fn to_display_pipeline_options(cfg: &OutputDriverOptionsConfig) -> DisplayPipelineOptions {
    DisplayPipelineOptions {
        lum_power: cfg.lum_power,
        white_point: cfg.white_point,
        brightness: cfg.brightness.clamp(0.0, 1.0),
        interpolation_enabled: cfg.interpolation_enabled,
        dithering_enabled: cfg.dithering_enabled,
        lut_enabled: cfg.lut_enabled,
    }
}

fn options_for_open(cfg: &OutputConfig) -> Option<DisplayPipelineOptions> {
    match cfg {
        OutputConfig::GpioStrip {
            options: Some(opts),
            ..
        } => Some(to_display_pipeline_options(opts)),
        _ => None,
    }
}

impl NodeRuntime for OutputRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // Get config
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: String::from("output"),
            reason: "Config not set".to_string(),
        })?;

        // Extract pin and options from config
        match config {
            OutputConfig::GpioStrip { pin, .. } => {
                self.pin = *pin;
            }
        }

        // For now, use a default byte_count (will be calculated properly later from fixtures)
        // byte_count = 8-bit output size (num_leds * 3). Default: 1 LED
        let byte_count = 3u32;
        let format = OutputFormat::Ws2811;

        // Open output channel with provider
        let handle =
            ctx.output_provider()
                .open(self.pin, byte_count, format, options_for_open(config))?;
        self.channel_handle = Some(handle);

        // Allocate 16-bit buffer: num_leds * 3 u16s
        let num_leds = (byte_count / 3) as usize;
        self.channel_data.resize(num_leds * 3, 0);

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        // Update state with current channel data
        let frame_id = ctx.frame_id();
        self.state
            .channel_data
            .set(frame_id, self.get_channel_data());

        // Flush buffer to provider if handle exists
        if let Some(handle) = self.channel_handle {
            ctx.output_provider().write(handle, &self.channel_data)?;
        }
        Ok(())
    }

    fn destroy(&mut self, output_provider: Option<&dyn OutputProvider>) -> Result<(), Error> {
        if let (Some(provider), Some(handle)) =
            (output_provider, core::mem::take(&mut self.channel_handle))
        {
            provider.close(handle).map_err(|e| Error::Other {
                message: alloc::format!("Failed to close output channel: {e}"),
            })?;
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }

    fn update_config(
        &mut self,
        new_config: Box<dyn NodeConfig>,
        ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Downcast to OutputConfig
        let output_config = new_config
            .as_any()
            .downcast_ref::<OutputConfig>()
            .ok_or_else(|| Error::InvalidConfig {
                node_path: String::from("output"),
                reason: "Config is not an OutputConfig".to_string(),
            })?;

        match output_config {
            OutputConfig::GpioStrip { pin, .. } => {
                // Close existing channel before reopen (update_config is only called when config changed)
                if let Some(handle) = self.channel_handle {
                    ctx.output_provider()
                        .close(handle)
                        .map_err(|e| Error::Other {
                            message: alloc::format!("Failed to close output channel: {e}"),
                        })?;
                    self.channel_handle = None;
                }

                self.pin = *pin;
                self.config = Some(output_config.clone());

                let byte_count = 3u32;
                let format = OutputFormat::Ws2811;
                let handle = ctx.output_provider().open(
                    self.pin,
                    byte_count,
                    format,
                    options_for_open(output_config),
                )?;
                self.channel_handle = Some(handle);
                let num_leds = (byte_count / 3) as usize;
                self.channel_data.resize(num_leds * 3, 0);
            }
        }

        Ok(())
    }

    fn handle_fs_change(
        &mut self,
        _change: &FsChange,
        _ctx: &dyn NodeInitContext,
    ) -> Result<(), Error> {
        // Outputs don't currently support loading from files
        // This is a no-op for now
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_runtime_creation() {
        let runtime = OutputRuntime::new();
        let _boxed: alloc::boxed::Box<dyn NodeRuntime> = alloc::boxed::Box::new(runtime);
    }
}
