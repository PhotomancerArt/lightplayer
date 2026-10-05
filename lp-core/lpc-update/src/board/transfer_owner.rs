//! Who owns a transfer (DM14, E6).
//!
//! The link that started or resumed a transfer owns it. Another link's offer
//! — the same transfer or any other, a heal included — is refused `N`/`B`
//! (with how far the transfer is) while the owner is **live**: up, and
//! heard from within [`SessionConfig::owner_quiet_ms`](super::SessionConfig)
//! (15 s; BLE supervision is 4 s, so that leaves margin). Once the owner has
//! gone down or quiet, the next link to offer takes the transfer over at the
//! first unwritten chunk, if it passes access for the transfer's kind. Data
//! from a link that is not the owner is ignored.
//!
//! A transfer restored from the progress record has no owner: the first
//! matching offer, from any link, resumes it.

use super::board_link::{BoardLink, LinkId};

/// Whether `owner` is live at `now_ms`: up, and heard from within
/// `quiet_ms`.
#[must_use]
pub(crate) fn owner_live(
    links: &[BoardLink],
    owner: Option<LinkId>,
    now_ms: u64,
    quiet_ms: u64,
) -> bool {
    let Some(owner) = owner else {
        return false;
    };
    links
        .iter()
        .find(|l| l.id == owner)
        .is_some_and(|l| now_ms.saturating_sub(l.last_rx_ms) < quiet_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::board_link::LinkTrust;

    fn link(id: u32, last_rx_ms: u64) -> BoardLink {
        BoardLink {
            id: LinkId(id),
            trust: LinkTrust::Untrusted,
            granted: None,
            last_rx_ms,
        }
    }

    #[test]
    fn live_means_up_and_recently_heard() {
        let links = [link(1, 1_000)];
        assert!(owner_live(&links, Some(LinkId(1)), 15_999, 15_000));
        assert!(
            !owner_live(&links, Some(LinkId(1)), 16_000, 15_000),
            "quiet"
        );
        assert!(!owner_live(&links, Some(LinkId(2)), 1_000, 15_000), "down");
        assert!(
            !owner_live(&links, None, 1_000, 15_000),
            "restored: no owner"
        );
    }
}
