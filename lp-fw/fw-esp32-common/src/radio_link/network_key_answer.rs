//! Handing a key-lookup answer to a network link's secure handshake — the
//! one step both answerers share: the link mux, with the engine's server's
//! answer, and core-only, with its own (`lpc_update`'s
//! `BoardSession::key_lookup`). Who decides the answer is theirs; this only
//! says it in the handshake's words.

use alloc::vec::Vec;

use lp_link::secure_channel::{KeyId, Psk, RefusalReason};
use lp_link::{Link, SelectiveRepeat};
use lpc_shared::transport::KeyAnswer;

/// Give `link`'s handshake for `key_id` the answer: its candidate PSKs, or
/// a refusal in the handshake's own words.
pub fn answer_key_lookup(link: &mut Link<SelectiveRepeat>, key_id: KeyId, answer: KeyAnswer) {
    match answer {
        KeyAnswer::Keys(psks) => {
            let psks: Vec<Psk> = psks.into_iter().map(Psk::new).collect();
            link.provide_keys(key_id, &psks);
        }
        KeyAnswer::Unknown => link.refuse(key_id, RefusalReason::UnknownKey, 0),
        KeyAnswer::Backoff { retry_after_ms } => {
            link.refuse(key_id, RefusalReason::Backoff, retry_after_ms);
        }
    }
}
