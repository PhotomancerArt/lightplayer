use crate::error::Error;
use crate::nodes::{NodeConfig, NodeRuntime};
use crate::output::{OutputChannelHandle, OutputFormat};
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use lp_model::nodes::output::OutputConfig;
use lp_shared::fs::fs_event::FsChange;

/// Output node runtime
pub struct OutputRuntime {
    /// Channel data buffer (DMX-style, sequential bytes)
    channel_data: Vec<u8>,
    /// Output channel handle from provider (None until initialized)
    channel_handle: Option<OutputChannelHandle>,
    /// GPIO pin number
    pin: u32,
    /// Output config (None until set)
    config: Option<OutputConfig>,
}

impl OutputRuntime {
    pub fn new() -> Self {
        Self {
            channel_data: Vec::new(),
            channel_handle: None,
            pin: 0,
            config: None,
        }
    }

    /// Set the output config
    pub fn set_config(&mut self, config: OutputConfig) {
        self.config = Some(config);
    }

    /// Get mutable slice to channel data, extending if needed
    pub fn get_buffer_mut(&mut self, start_ch: u32, ch_count: u32) -> &mut [u8] {
        let end = (start_ch + ch_count) as usize;
        if end > self.channel_data.len() {
            self.channel_data.resize(end, 0);
        }
        &mut self.channel_data[start_ch as usize..end]
    }

    /// Get channel data (for state extraction)
    pub fn get_channel_data(&self) -> &[u8] {
        &self.channel_data
    }

    /// Get the output config (for state extraction)
    pub fn get_config(&self) -> Option<&OutputConfig> {
        self.config.as_ref()
    }
}

impl NodeRuntime for OutputRuntime {
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error> {
        // Get config
        let config = self.config.as_ref().ok_or_else(|| Error::InvalidConfig {
            node_path: String::from("output"),
            reason: "Config not set".to_string(),
        })?;

        // Extract pin from config
        match config {
            OutputConfig::GpioStrip { pin } => {
                self.pin = *pin;
            }
        }

        // For now, use a default byte_count (will be calculated properly later from fixtures)
        // Default: 3 bytes for single RGB pixel
        let byte_count = 3u32;
        let format = OutputFormat::Ws2811;

        // Open output channel with provider
        let handle = ctx.output_provider().open(self.pin, byte_count, format)?;
        self.channel_handle = Some(handle);

        // Allocate buffer
        self.channel_data.resize(byte_count as usize, 0);

        Ok(())
    }

    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error> {
        // Flush buffer to provider if handle exists
        if let Some(handle) = self.channel_handle {
            ctx.output_provider().write(handle, &self.channel_data)?;
        }
        Ok(())
    }

    fn destroy(&mut self) -> Result<(), Error> {
        // Note: Provider cleanup should happen at a higher level
        // since destroy() doesn't have access to the provider context
        // For now, just clear the handle
        self.channel_handle = None;
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

        // Check if pin changed
        let old_pin = self.pin;
        match output_config {
            OutputConfig::GpioStrip { pin } => {
                if *pin != old_pin {
                    // Pin changed - need to reinitialize
                    // Close old channel if exists
                    if self.channel_handle.is_some() {
                        // Note: We don't have access to provider here to close
                        // The old channel will be cleaned up when provider is destroyed
                        self.channel_handle = None;
                    }

                    self.pin = *pin;
                    self.config = Some(output_config.clone());

                    // Reinitialize with new pin
                    let byte_count = 3u32; // Default for now
                    let format = OutputFormat::Ws2811;
                    let handle = ctx.output_provider().open(self.pin, byte_count, format)?;
                    self.channel_handle = Some(handle);
                    self.channel_data.resize(byte_count as usize, 0);
                } else {
                    // Just update config
                    self.config = Some(output_config.clone());
                }
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
