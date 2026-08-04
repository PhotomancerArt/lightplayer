//! ESP32 OutputProvider implementation.
//!
//! The provider is the compatibility layer used by the engine. Hardware-specific
//! details live in capability drivers registered on the root `HardwareSystem`.

use alloc::boxed::Box;
use alloc::format;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::cell::RefCell;
use lp_collection::VecMap;

use lpc_hardware::OutputError;
use lpc_hardware::{
    HardwareEndpointError, HardwareSystem, HwEndpointSpec, WS281X_MAX_LEDS_PER_CHANNEL,
    Ws281xConfig, Ws281xOutput, ws281x_capped_byte_count,
};
use lpc_shared::DisplayPipeline;
use lpc_shared::output::{OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider};
const FRAME_INTERVAL_US: u64 = 16_667;
const MID_FRAME_US: u64 = 8_333;

/// Channels reserved up front in [`Esp32OutputProvider::channels`].
///
/// A **reservation, not a limit** — `VecMap` still grows past it. It exists so
/// that opening the Nth channel does not reallocate and memcpy the previous
/// N-1 `ChannelState`s while they are live, which is a transient peak of twice
/// the steady-state size on the one path that runs when the heap is already at
/// its high-water mark.
///
/// 8 is the widest RMT TX slot count in the family (the classic's eight,
/// before the block plan absorbs slots into wider windows). At today's
/// `ChannelState` size this reservation costs well under a kilobyte.
///
/// ⚠️ Historical note worth keeping: this used to matter enormously.
/// `ChannelState` carried a 3,084-byte inline white-point LUT, so the same
/// growth asked for 12,864 contiguous bytes and OOM'd the classic ESP32 with
/// 11,228 free — `docs/defects/2026-08-01-classic-rmt-open-fault.md`. The LUT
/// is gone; this guard remains because the growth pattern is what turned a
/// tight heap into a hard fault, and it will do so again for whatever the next
/// large per-channel field is.
const RESERVED_CHANNELS: usize = 8;

struct ChannelState {
    output: Box<dyn Ws281xOutput>,
    byte_count: u32,
    pipeline: DisplayPipeline,
    /// The channel's own rendered frame, alive between writes.
    ///
    /// It used to be a `Vec` allocated inside every `write`, which is fine
    /// while one handle is written at a time. It is not fine for a batched
    /// transmission, where every channel's bytes must stay alive across the
    /// whole batch — so the storage belongs to the channel, not to the call.
    /// Sized from the channel's granted `byte_count`, so this is the same
    /// memory the per-write allocation held, just held for longer.
    frame: Vec<u8>,
}

/// ESP32 OutputProvider implementation.
pub struct Esp32OutputProvider {
    hardware_system: Rc<HardwareSystem>,
    channels: RefCell<VecMap<i32, ChannelState>>,
    next_handle: RefCell<i32>,
}

impl Esp32OutputProvider {
    pub fn new(hardware_system: Rc<HardwareSystem>) -> Self {
        Self {
            hardware_system,
            channels: RefCell::new(VecMap::with_capacity(RESERVED_CHANNELS)),
            next_handle: RefCell::new(1),
        }
    }
}

impl OutputProvider for Esp32OutputProvider {
    fn open(
        &self,
        endpoint: &HwEndpointSpec,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, OutputError> {
        let options = options.unwrap_or_default();
        log::debug!(
            "Esp32OutputProvider::open: endpoint={endpoint}, byte_count={byte_count}, format={format:?}"
        );

        if format != OutputFormat::Ws2811 {
            log::warn!("Esp32OutputProvider::open: Unsupported format: {format:?}");
            return Err(OutputError::InvalidConfig {
                reason: format!("Unsupported format: {format:?}"),
            });
        }
        if byte_count < 3 {
            log::warn!("Esp32OutputProvider::open: byte_count {byte_count} too small");
            return Err(OutputError::InvalidConfig {
                reason: "byte_count must be at least 3 (one LED)".into(),
            });
        }

        let (byte_count, truncated) = ws281x_capped_byte_count(byte_count);
        if truncated {
            log::warn!(
                "Esp32OutputProvider::open: endpoint={endpoint} asked for more than \
                 {WS281X_MAX_LEDS_PER_CHANNEL} LEDs; truncating to {byte_count} bytes"
            );
        }
        let output = self
            .hardware_system
            .open_ws281x_by_spec(endpoint, Ws281xConfig::new(byte_count))
            .map_err(endpoint_error_to_output_error)?;
        let pipeline = DisplayPipeline::new(byte_count / 3, options.clone()).map_err(|error| {
            OutputError::InvalidConfig {
                reason: format!("DisplayPipeline allocation failed: {error}"),
            }
        })?;

        let handle_id = *self.next_handle.borrow();
        *self.next_handle.borrow_mut() += 1;
        let handle = OutputChannelHandle::new(handle_id);

        log::info!(
            "Esp32OutputProvider::open: Opened channel handle={handle_id}, endpoint={endpoint}, byte_count={byte_count}"
        );

        let mut frame = Vec::new();
        frame.resize(((byte_count / 3) * 3) as usize, 0);

        self.channels.borrow_mut().insert(
            handle_id,
            ChannelState {
                output,
                byte_count,
                pipeline,
                frame,
            },
        );

        Ok(handle)
    }

    fn write(&self, handle: OutputChannelHandle, data: &[u16]) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        log::debug!(
            "Esp32OutputProvider::write: handle={}, data_len={}",
            handle_id,
            data.len()
        );

        let mut channels = self.channels.borrow_mut();
        let channel = channels.get_mut(&handle_id).ok_or_else(|| {
            log::warn!("Esp32OutputProvider::write: Invalid handle {handle_id}");
            OutputError::InvalidHandle { handle: handle_id }
        })?;

        let mut num_leds = (channel.byte_count / 3) as usize;
        let expected_len = num_leds * 3;

        if data.len() > expected_len {
            let (new_byte_count, truncated) = capped_byte_count_for_len(data.len());
            // A frame past the cap keeps `data.len() > expected_len` true forever;
            // only act (and warn) when the granted size actually changes, so the
            // steady state neither re-resizes nor logs per frame.
            if new_byte_count != channel.byte_count {
                if truncated {
                    log::warn!(
                        "Esp32OutputProvider::write: handle={handle_id} grew past \
                         {WS281X_MAX_LEDS_PER_CHANNEL} LEDs; truncating to {new_byte_count} bytes"
                    );
                }
                channel.output.resize(Ws281xConfig::new(new_byte_count))?;
                channel.pipeline.resize(new_byte_count / 3);
                channel.byte_count = new_byte_count;
                num_leds = (channel.byte_count / 3) as usize;
            }
        } else if data.len() < expected_len {
            return Err(OutputError::DataLengthMismatch {
                expected: expected_len as u32,
                actual: data.len(),
            });
        }

        // The channel owns its frame storage, so a resize is the only time
        // this allocates and a steady-state write allocates nothing at all.
        channel.frame.resize(num_leds * 3, 0);

        channel.pipeline.write_frame(0, data);
        channel.pipeline.write_frame(FRAME_INTERVAL_US, data);
        let ChannelState {
            output,
            pipeline,
            frame,
            ..
        } = channel;
        pipeline.tick(MID_FRAME_US, frame);

        output.write(frame)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), OutputError> {
        let handle_id = handle.as_i32();
        self.channels
            .borrow_mut()
            .remove(&handle_id)
            .ok_or_else(|| OutputError::InvalidHandle { handle: handle_id })?;
        Ok(())
    }

    fn hardware_generation(&self) -> u64 {
        self.hardware_system.registry().generation()
    }
}

fn capped_byte_count_for_len(data_len: usize) -> (u32, bool) {
    ws281x_capped_byte_count(((data_len / 3) * 3) as u32)
}

fn endpoint_error_to_output_error(error: HardwareEndpointError) -> OutputError {
    match error {
        HardwareEndpointError::Hardware { error } => OutputError::Hardware { error },
        other => OutputError::InvalidConfig {
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    //! Defense-in-depth backstop, tested directly: the engine seam
    //! (`lpc-engine`'s `wire_slice`) is what guarantees parity across
    //! providers now (P3), but `Esp32OutputProvider` keeps its own
    //! `WS281X_MAX_LEDS_PER_CHANNEL` check on top, and that check gets its
    //! own coverage here, at the raised 1024-lamp bound.

    use alloc::rc::Rc;
    use alloc::vec;

    use lpc_hardware::{
        HardwareSystem, HwManifest, HwRegistry, OutputError, WS281X_MAX_LEDS_PER_CHANNEL,
    };
    use lpc_shared::output::{OutputChannelHandle, OutputFormat, OutputProvider};

    use super::Esp32OutputProvider;

    fn provider() -> Esp32OutputProvider {
        let registry = Rc::new(HwRegistry::new(HwManifest::virtual_single_rmt_gpio_board()));
        Esp32OutputProvider::new(Rc::new(HardwareSystem::with_virtual_drivers(registry)))
    }

    fn open_ws281x(provider: &Esp32OutputProvider, byte_count: u32) -> OutputChannelHandle {
        provider
            .open(
                &lpc_hardware::HwEndpointSpec::from_static("ws281x:local:D10"),
                byte_count,
                OutputFormat::Ws2811,
                None,
            )
            .expect("ws281x:local:D10 opens on the virtual single-RMT board")
    }

    /// A channel authored for 1500 lamps must grant exactly the shared cap —
    /// `WS281X_MAX_LEDS_PER_CHANNEL * 3` bytes, 1024 lamps now that P3 raised
    /// it from 256 — never the full request. Proven by capacity, not by
    /// reaching into the opaque `Box<dyn Ws281xOutput>`: a write one lamp
    /// short of the granted size must still be rejected as a length
    /// mismatch, and a write of exactly the granted size must succeed.
    #[test]
    fn open_caps_a_request_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        let handle = open_ws281x(&provider, 1500 * 3);

        let short = vec![0u16; (cap_bytes - 3) as usize];
        assert!(
            matches!(
                provider.write(handle, &short),
                Err(OutputError::DataLengthMismatch { expected, .. }) if expected == cap_bytes
            ),
            "the channel must be sized to exactly the capped byte count, not the requested one"
        );

        let exact = vec![0u16; cap_bytes as usize];
        provider
            .write(handle, &exact)
            .expect("a write of exactly the capped size succeeds");
    }

    /// 375 and 1024 lamps both sit at or under the cap: `open` must grant
    /// exactly what was asked, with no truncation.
    #[test]
    fn open_grants_the_full_request_at_or_under_the_shared_bound() {
        for lamps in [375u32, WS281X_MAX_LEDS_PER_CHANNEL as u32] {
            let provider = provider();
            let byte_count = lamps * 3;
            let handle = open_ws281x(&provider, byte_count);

            let exact = vec![0u16; byte_count as usize];
            provider.write(handle, &exact).unwrap_or_else(|error| {
                panic!("{lamps}-lamp channel should not be capped: {error}")
            });
        }
    }

    /// The write-time grow path caps identically to the open-time path: a
    /// channel that opened small and then receives a larger frame is capped
    /// to the same 1024-lamp bound, not the frame's own size.
    #[test]
    fn write_caps_a_grown_frame_above_the_shared_bound_to_exactly_1024_lamps() {
        let provider = provider();
        let cap_bytes = (WS281X_MAX_LEDS_PER_CHANNEL * 3) as u32;
        let handle = open_ws281x(&provider, 3);

        let grown = vec![0u16; (1500 * 3) as usize];
        provider
            .write(handle, &grown)
            .expect("a grown frame is capped, not rejected");

        let short = vec![0u16; (cap_bytes - 3) as usize];
        assert!(
            matches!(
                provider.write(handle, &short),
                Err(OutputError::DataLengthMismatch { expected, .. }) if expected == cap_bytes
            ),
            "the channel must have grown to exactly the capped byte count, not the frame's own size"
        );
    }
}
