#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HardwareCapability {
    GpioOutput,
    GpioInput,
    Ws281xOutput,
    Rmt,
    Radio,
}
