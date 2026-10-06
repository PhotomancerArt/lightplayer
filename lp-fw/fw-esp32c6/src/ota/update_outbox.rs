//! Channel-3 messages on their way to the USB host, in order, for one link
//! session: the session's answers wait here while the link has no room
//! (`UsbLinkShared::send_update` says `Later`) and are dropped when the
//! session they were for ends.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use fw_esp32_common::usb_link::{UpdateSend, UsbLinkShared};

/// Messages waiting for the link, and the session they belong to.
pub struct UpdateOutbox {
    queue: VecDeque<Vec<u8>>,
    generation: Option<u32>,
}

impl UpdateOutbox {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            generation: None,
        }
    }

    /// Queue `bytes` for the link's current session.
    pub fn push(&mut self, link: &UsbLinkShared, bytes: Vec<u8>) {
        self.follow_session(link);
        self.queue.push_back(bytes);
    }

    /// Send what the link takes now, in order.
    pub fn flush(&mut self, link: &UsbLinkShared) {
        self.follow_session(link);
        while let Some(next) = self.queue.front() {
            match link.send_update(next) {
                UpdateSend::Queued => {
                    self.queue.pop_front();
                }
                UpdateSend::Later => return,
                UpdateSend::NoSession => {
                    self.queue.clear();
                    return;
                }
                UpdateSend::TooBig => {
                    log::error!("[OTA] dropped a {} B message: too big", next.len());
                    self.queue.pop_front();
                }
            }
        }
    }

    /// A message held for an ended session would reach the next one.
    fn follow_session(&mut self, link: &UsbLinkShared) {
        let now = link.with_link(|l| l.generation());
        if self.generation != Some(now) {
            self.queue.clear();
            self.generation = Some(now);
        }
    }
}
