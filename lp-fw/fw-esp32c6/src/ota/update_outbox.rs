//! Channel-3 messages on their way to one host link, in order, for one link
//! session: the session's answers wait here while the link has no room
//! ([`UpdateLinks::send`] says `Later`) and are dropped when the session
//! they were for ends — a link reset, or the link gone. The edge keeps one
//! per link it has answered.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use fw_esp32_common::update_send::UpdateSend;
use lpc_update::board::LinkId;

use super::update_links::UpdateLinks;

/// Messages waiting for one link, and the session they belong to.
pub struct UpdateOutbox {
    pub link: LinkId,
    queue: VecDeque<Vec<u8>>,
    generation: Option<u32>,
}

impl UpdateOutbox {
    pub const fn new(link: LinkId) -> Self {
        Self {
            link,
            queue: VecDeque::new(),
            generation: None,
        }
    }

    /// Queue `bytes` for the link's current session.
    pub fn push(&mut self, links: &UpdateLinks, bytes: Vec<u8>) {
        self.follow_session(links);
        self.queue.push_back(bytes);
    }

    /// Send what the link takes now, in order.
    pub fn flush(&mut self, links: &UpdateLinks) {
        self.follow_session(links);
        while let Some(next) = self.queue.front() {
            match links.send(self.link, next) {
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

    /// Nothing is waiting.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// A message held for an ended session would reach the next one.
    fn follow_session(&mut self, links: &UpdateLinks) {
        let now = links.generation(self.link);
        if self.generation != now {
            self.queue.clear();
            self.generation = now;
        }
    }
}
