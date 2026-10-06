//! The machine's endpoints: the host half of every engaged capability seam,
//! addressed `<board>/<seam>` (`lp_emu_esp_common::seam::SeamEndpoint`).
//!
//! Made when a chip start engages a capability seam, one per seam, in
//! engaged order; a guest names one by its index and owns bit `1 << index`
//! of the wake pending word. A host reaches them through
//! [`Esp32C6Machine::seam_endpoint_mut`] (to queue inbound events) and a
//! medium through [`Esp32C6Machine::seam_endpoints_mut`]. Nothing here is
//! static: two machines in one process each hold their own.

use lp_emu_esp_common::ParticipantId;
use lp_emu_esp_common::seam::{EndpointId, Engaged, SeamEndpoint};
use lp_seam::SeamKind;

use super::seam_wake_stats::WakeStats;
use crate::machine::Esp32C6Machine;

impl Esp32C6Machine {
    /// This machine's name on a seam medium. Set it before the run starts
    /// (endpoints made later carry it); the lockstep runner names each of its
    /// machines by its slot.
    pub fn set_seam_board(&mut self, board: ParticipantId) {
        self.seams.board = board;
        for e in &mut self.seams.endpoints {
            e.id.board = board;
        }
    }

    /// Every endpoint this chip start engaged.
    pub fn seam_endpoints(&self) -> &[SeamEndpoint] {
        &self.seams.endpoints
    }

    /// The same, for a medium to deliver into and drain from.
    pub fn seam_endpoints_mut(&mut self) -> &mut [SeamEndpoint] {
        &mut self.seams.endpoints
    }

    /// One endpoint by its id.
    pub fn seam_endpoint(&self, id: EndpointId) -> Option<&SeamEndpoint> {
        self.seams.endpoints.iter().find(|e| e.id == id)
    }

    /// One endpoint by its id, to queue inbound events on.
    pub fn seam_endpoint_mut(&mut self, id: EndpointId) -> Option<&mut SeamEndpoint> {
        self.seams.endpoints.iter_mut().find(|e| e.id == id)
    }

    /// Each endpoint's wake counters, beside it. Emulated figures.
    pub fn seam_wake_lines(&self) -> Vec<String> {
        self.seams
            .endpoints
            .iter()
            .zip(&self.seams.wake_stats)
            .map(|(e, s)| s.line(&e.id.to_string(), e.refused()))
            .collect()
    }

    /// Make the endpoints for a chip start's engaged capability seams.
    pub(crate) fn seam_make_endpoints(&mut self, engaged: &Engaged) {
        let board = self.seams.board;
        let config = self.seams.pacer_config;
        for imp in engaged
            .engaged
            .iter()
            .filter(|i| i.kind == SeamKind::Capability)
        {
            let bit = 1u32 << self.seams.endpoints.len().min(31);
            let id = EndpointId {
                board,
                seam: imp.label,
            };
            self.seams
                .endpoints
                .push(SeamEndpoint::new(id, bit, config));
            self.seams.wake_stats.push(WakeStats::default());
        }
        self.seams.pending = engaged.table.pending;
    }

    /// `test=take`'s answer: copy up to `cap` bytes from endpoint `endpoint`
    /// into the guest buffer at `buf` — the buffer the call handed over, the
    /// only guest memory a seam answer writes — and return how many.
    pub(crate) fn seam_take(&mut self, endpoint: u32, buf: u32, cap: u32) -> u32 {
        let Some(e) = self.seams.endpoints.get_mut(endpoint as usize) else {
            return 0;
        };
        let bytes = e.take(cap as usize);
        if bytes.is_empty() || !self.poke_bytes(buf, &bytes) {
            return 0;
        }
        if let Some(s) = self.seams.wake_stats.get_mut(endpoint as usize) {
            s.bytes_taken += bytes.len() as u64;
        }
        bytes.len() as u32
    }
}
