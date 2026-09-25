//! Serial ClientTransport implementation for emulator (test-specific)
//!
//! Runs the emulator synchronously when sending/receiving messages.
//! When sending a message, runs the emulator until it yields a response.

use async_trait::async_trait;
use hashbrown::HashMap;
use log;
use lp_emu_core::MemoryAccessKind;
use lp_riscv_elf::format_backtrace;
use lp_riscv_emu::Riscv32Emulator;
use lpc_wire::{PackOptIn, WireChunk, WireServerMessage, WireStream};
use lpc_wire::{TransportError, json, messages::ClientMessage};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Serial ClientTransport that communicates with firmware running in emulator
///
/// Runs the emulator synchronously - when sending a message, runs until yield.
/// This is simpler than async task approach and fails fast if no response.
pub struct SerialEmuClientTransport {
    /// Emulator instance (shared, mutex-protected)
    emulator: Arc<Mutex<Riscv32Emulator>>,
    /// The board's stream, split frame-first (JSON lines and packed frames).
    wire: WireStream,
    /// Chunks split off the stream and not yet handed out.
    pending: VecDeque<WireChunk>,
    /// When to ask the board to pack (never, on `fw-emu`: its hello names no
    /// dictionary).
    opt_in: PackOptIn,
    started: Instant,
    /// Symbol map for backtrace (optional)
    symbol_map: Option<HashMap<String, u32>>,
    /// Code end address for backtrace symbolication
    code_end: u32,
}

impl SerialEmuClientTransport {
    /// Create a new serial client transport
    ///
    /// # Arguments
    /// * `emulator` - Shared reference to the emulator
    pub fn new(emulator: Arc<Mutex<Riscv32Emulator>>) -> Self {
        Self {
            emulator,
            wire: WireStream::new(),
            pending: VecDeque::new(),
            opt_in: PackOptIn::wanting(crate::wire_encoding_env::requested_wire_encoding()),
            started: Instant::now(),
            symbol_map: None,
            code_end: 0,
        }
    }

    /// Enable backtrace on emulator errors using ELF symbol info
    pub fn with_backtrace(mut self, symbol_map: HashMap<String, u32>, code_end: u32) -> Self {
        self.symbol_map = Some(symbol_map);
        self.code_end = code_end;
        self
    }

    /// Read the next complete message from serial output.
    ///
    /// `M!{json}` lines and packed frames alike; every other line is a server
    /// log and skipped.
    fn read_message(&mut self) -> Result<Option<WireServerMessage>, TransportError> {
        // Drain serial output from emulator
        let output = {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.drain_serial_output()
        };

        if !output.is_empty() {
            log::trace!(
                "SerialEmuClientTransport::read_message: Drained {} bytes from serial output",
                output.len()
            );
            let pending = &mut self.pending;
            self.wire.push(&output, |chunk| pending.push_back(chunk));
        }

        while let Some(chunk) = self.pending.pop_front() {
            let frame = match chunk {
                WireChunk::Line(line) => {
                    log::trace!(
                        "SerialEmuClientTransport: Skipping non-message line ({} bytes)",
                        line.len()
                    );
                    continue;
                }
                WireChunk::Error(error) => {
                    log::warn!("SerialEmuClientTransport: {error}");
                    continue;
                }
                WireChunk::Desync(dropped) => {
                    log::warn!(
                        "SerialEmuClientTransport: packed reply dropped ({} B): {}",
                        dropped.wire_len,
                        dropped.reason
                    );
                    let now_ms =
                        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
                    if let Some(ask) = self.opt_in.desynced(now_ms) {
                        let line = json::to_serial_line(&ask)
                            .map_err(|e| TransportError::Serialization(e.to_string()))?;
                        self.emulator
                            .lock()
                            .map_err(|_| TransportError::ConnectionLost)?
                            .serial_write(line.as_bytes());
                    }
                    continue;
                }
                WireChunk::Frame(frame) => frame,
            };
            let message = match json::from_str::<WireServerMessage>(&frame.json) {
                Ok(message) => message,
                Err(e) => {
                    log::warn!(
                        "SerialEmuClientTransport: Failed to parse M! line: {e} | {}",
                        frame.json
                    );
                    continue;
                }
            };
            log::debug!(
                "SerialEmuClientTransport: Received message id={} ({} bytes): M!{}",
                message.id,
                frame.json.len(),
                frame.json
            );
            let now_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
            let step = self.opt_in.observe(&message, frame.is_packed(), now_ms);
            if let Some(ask) = step.send {
                let line = json::to_serial_line(&ask)
                    .map_err(|e| TransportError::Serialization(e.to_string()))?;
                self.emulator
                    .lock()
                    .map_err(|_| TransportError::ConnectionLost)?
                    .serial_write(line.as_bytes());
            }
            if step.deliver {
                return Ok(Some(message));
            }
        }

        if self.wire.pending_bytes() > 0 {
            log::trace!(
                "SerialEmuClientTransport: Partial message buffered ({} bytes)",
                self.wire.pending_bytes()
            );
        }
        Ok(None)
    }

    /// Run emulator until yield
    fn run_until_yield(&mut self) -> Result<(), TransportError> {
        const MAX_STEPS_PER_ITERATION: u64 = 500_000_000;

        // Run emulator until yield
        let result = {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.run_until_yield(MAX_STEPS_PER_ITERATION)
        };

        match result {
            Ok(_) => {
                log::trace!("SerialEmuClientTransport: Emulator yielded");
                Ok(())
            }
            Err(e) if e.is_profile_stop() => {
                // Profile gate fired Stop while we were driving the emulator
                // toward a yield (typically during teardown RPCs like
                // stopAllProjects). This is a clean, expected condition —
                // the emulator deliberately stopped and will not produce a
                // response. Log quietly without the full state dump.
                log::debug!("SerialEmuClientTransport: {e}");
                Err(TransportError::Other(format!("{e}")))
            }
            Err(e) => {
                // Print emulator state on error for debugging
                if let Ok(emu) = self.emulator.lock() {
                    log::error!("Emulator error in run_until_yield: {e:?}");
                    if let Some(ref symbol_map) = self.symbol_map {
                        if let Some(regs) = e.regs() {
                            let addrs = emu.unwind_backtrace(e.pc(), regs);
                            let bt = format_backtrace(&addrs, symbol_map, self.code_end);
                            log::error!("Backtrace:\n{bt}");
                        }
                    }
                    // InstructionFetch hint: identify jump source and dump vtable/GOT if applicable
                    if let lp_riscv_emu::EmulatorError::InvalidMemoryAccess {
                        address,
                        kind: MemoryAccessKind::InstructionFetch,
                        regs,
                        ..
                    } = &e
                    {
                        let bad_addr = *address;
                        let reg_names = [
                            "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0",
                            "a1", "a2", "a3", "a4", "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6",
                            "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
                        ];
                        let mut hint_regs = Vec::new();
                        for (i, &v) in regs.iter().enumerate() {
                            if i == 0 {
                                continue; // x0 is always 0
                            }
                            let v32 = v as u32;
                            if v32 == bad_addr || (v32 & !1) == (bad_addr & !1) {
                                hint_regs.push((i, reg_names[i], v32));
                            }
                        }
                        if !hint_regs.is_empty() {
                            let reg_desc: String = hint_regs
                                .iter()
                                .map(|(i, n, v)| format!("{n} (x{i})=0x{v:08x}"))
                                .collect::<Vec<_>>()
                                .join(", ");
                            log::error!(
                                "InstructionFetch hint: Bad PC 0x{bad_addr:08x} likely from indirect jump. \
                                 Registers holding this value: {reg_desc}. \
                                 May indicate bad vtable/GOT entry or unresolved relocation."
                            );
                            // Find Load that populated this value and dump memory at that address
                            for log in emu.get_logs().iter().rev() {
                                if let lp_riscv_emu::InstLog::Load {
                                    addr, rd_new, rd, ..
                                } = log
                                {
                                    let rd_new_u32 = *rd_new as u32;
                                    if rd_new_u32 == bad_addr
                                        || (rd_new_u32 & !1) == (bad_addr & !1)
                                    {
                                        let load_addr = *addr & !3;
                                        if let Some(dump) = emu.dump_memory_hex(load_addr, 32) {
                                            log::error!(
                                                "Memory at load source (0x{addr:08x}, {rd} received 0x{rd_new_u32:08x}):\n{dump}"
                                            );
                                        }
                                        break; // Only dump for first (most recent) matching load
                                    }
                                }
                            }
                        }
                    }
                    log::error!("Emulator state:\n{}", emu.dump_state());
                    log::error!(
                        "Last {} instructions:\n{}",
                        emu.get_logs().len(),
                        emu.format_logs()
                    );
                    if let Some(regs) = e.regs() {
                        log::error!("Registers at error: {regs:?}");
                    }
                }
                Err(TransportError::Other(format!("Emulator error: {e:?}")))
            }
        }
    }
}

#[async_trait]
impl crate::transport::ClientTransport for SerialEmuClientTransport {
    async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
        // Frame as one `M!{json}\n` line (the shared framer).
        let line = json::to_serial_line(&msg)
            .map_err(|e| TransportError::Serialization(format!("JSON serialize error: {e}")))?;
        let total_bytes = line.len();

        log::debug!(
            "SerialEmuClientTransport: Sending message id={} ({} bytes): {}",
            msg.id,
            total_bytes,
            line.trim_end()
        );

        let data = line.into_bytes();

        log::trace!(
            "SerialEmuClientTransport: Writing {total_bytes} bytes to emulator serial input"
        );

        // Add to emulator's serial input buffer
        {
            let mut emu = self
                .emulator
                .lock()
                .map_err(|_| TransportError::ConnectionLost)?;
            emu.serial_write(&data);
        }

        log::trace!("SerialEmuClientTransport: Message written to serial buffer");

        Ok(())
    }

    async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
        log::debug!("SerialEmuClientTransport::receive: Waiting for message");

        // Check if we already have a message buffered
        if let Some(msg) = self.read_message()? {
            log::debug!(
                "SerialEmuClientTransport::receive: Found message in buffer id={}",
                msg.id
            );
            log::trace!(
                "SerialEmuClientTransport::receive: Message content: {}",
                json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
            );
            return Ok(msg);
        }

        // No message available, run emulator until yield
        // The firmware should have processed the message and sent a response
        self.run_until_yield()?;

        // Check for message after yield
        if let Some(msg) = self.read_message()? {
            log::debug!(
                "SerialEmuClientTransport::receive: Found message after yield id={}",
                msg.id
            );
            log::trace!(
                "SerialEmuClientTransport::receive: Message content: {}",
                json::to_string(&msg).unwrap_or_else(|_| "<failed to serialize>".to_string())
            );
            return Ok(msg);
        }

        // No message after yield - firmware should have sent response before yielding
        Err(TransportError::Other(
            "Emulator yielded but no response message received".to_string(),
        ))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        // Nothing to close for emulator transport
        Ok(())
    }
}
