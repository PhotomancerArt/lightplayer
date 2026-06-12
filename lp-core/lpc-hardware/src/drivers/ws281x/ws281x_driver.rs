use alloc::boxed::Box;

use crate::{DisplayPipelineOptions, OutputError};

use crate::{HwDriver, HwEndpoint, HardwareEndpointError, HwEndpointId};

#[derive(Debug, Clone)]
pub struct Ws281xConfig {
    byte_count: u32,
    display_options: Option<DisplayPipelineOptions>,
}

impl Ws281xConfig {
    pub fn new(byte_count: u32, display_options: Option<DisplayPipelineOptions>) -> Self {
        Self {
            byte_count,
            display_options,
        }
    }

    pub fn byte_count(&self) -> u32 {
        self.byte_count
    }

    pub fn display_options(&self) -> Option<&DisplayPipelineOptions> {
        self.display_options.as_ref()
    }

    pub fn display_options_cloned(&self) -> Option<DisplayPipelineOptions> {
        self.display_options.clone()
    }
}

pub trait Ws281xOutput {
    fn write(&mut self, data: &[u16]) -> Result<(), OutputError>;

    fn resize(&mut self, config: Ws281xConfig) -> Result<(), OutputError>;
}

pub trait Ws281xDriver: HwDriver {
    fn endpoints(&self) -> alloc::vec::Vec<HwEndpoint>;

    fn open(
        &self,
        endpoint_id: &HwEndpointId,
        config: Ws281xConfig,
    ) -> Result<Box<dyn Ws281xOutput>, HardwareEndpointError>;
}
