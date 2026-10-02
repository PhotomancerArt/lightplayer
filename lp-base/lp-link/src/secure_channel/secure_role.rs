//! Which end of the Noise handshake a secure link plays.

use crate::secure_channel::key_id::KeyId;
use crate::secure_channel::psk::Psk;

#[derive(Clone, Debug)]
pub enum SecureRole {
    /// The client: it names its key in its first SYN and proves it holds the
    /// PSK. [`KeyId::ANONYMOUS`] with [`Psk::ANONYMOUS`] asks for an
    /// encrypted session that authenticates nobody.
    Initiator { key_id: KeyId, psk: Psk },
    /// The device: it holds no key up front, and asks its edge for the PSKs
    /// that go with the key id it hears
    /// ([`SecureEvent::KeyLookup`](crate::secure_channel::SecureEvent::KeyLookup)).
    Responder,
}
