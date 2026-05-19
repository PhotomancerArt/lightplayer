use alloc::vec::Vec;
use core::fmt;

use super::{RadioChannelId, RadioDeviceId, RadioEventId};

pub const RADIO_WIRE_MAGIC: u16 = 0x4c50;
pub const RADIO_WIRE_VERSION: u8 = 1;
pub const RADIO_MAX_PAYLOAD_LEN: usize = 64;
pub const RADIO_WIRE_HEADER_LEN: usize = 17;
pub const RADIO_MAX_PACKET_LEN: usize = RADIO_WIRE_HEADER_LEN + RADIO_MAX_PAYLOAD_LEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadioMessageKind {
    ButtonPress,
    Custom(u8),
}

impl RadioMessageKind {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::ButtonPress => 1,
            Self::Custom(value) => value,
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::ButtonPress,
            other => Self::Custom(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadioMessage {
    source_device_id: RadioDeviceId,
    event_id: RadioEventId,
    channel_id: RadioChannelId,
    kind: RadioMessageKind,
    payload: Vec<u8>,
}

impl RadioMessage {
    pub fn new(
        source_device_id: RadioDeviceId,
        event_id: RadioEventId,
        channel_id: RadioChannelId,
        kind: RadioMessageKind,
        payload: impl Into<Vec<u8>>,
    ) -> Result<Self, RadioPacketError> {
        let payload = payload.into();
        if payload.len() > RADIO_MAX_PAYLOAD_LEN {
            return Err(RadioPacketError::PayloadTooLarge {
                len: payload.len(),
                max: RADIO_MAX_PAYLOAD_LEN,
            });
        }
        Ok(Self {
            source_device_id,
            event_id,
            channel_id,
            kind,
            payload,
        })
    }

    pub fn button_press(
        source_device_id: RadioDeviceId,
        event_id: RadioEventId,
        channel_id: RadioChannelId,
    ) -> Self {
        Self {
            source_device_id,
            event_id,
            channel_id,
            kind: RadioMessageKind::ButtonPress,
            payload: Vec::new(),
        }
    }

    pub const fn source_device_id(&self) -> RadioDeviceId {
        self.source_device_id
    }

    pub const fn event_id(&self) -> RadioEventId {
        self.event_id
    }

    pub const fn channel_id(&self) -> RadioChannelId {
        self.channel_id
    }

    pub const fn kind(&self) -> RadioMessageKind {
        self.kind
    }

    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn encode(&self, out: &mut [u8; RADIO_MAX_PACKET_LEN]) -> usize {
        out[0..2].copy_from_slice(&RADIO_WIRE_MAGIC.to_le_bytes());
        out[2] = RADIO_WIRE_VERSION;
        out[3] = self.kind.as_u8();
        out[4..8].copy_from_slice(&self.source_device_id.as_u32().to_le_bytes());
        out[8..12].copy_from_slice(&self.event_id.as_u32().to_le_bytes());
        out[12..16].copy_from_slice(&self.channel_id.as_u32().to_le_bytes());
        out[16] = self.payload.len() as u8;
        let end = RADIO_WIRE_HEADER_LEN + self.payload.len();
        out[RADIO_WIRE_HEADER_LEN..end].copy_from_slice(&self.payload);
        end
    }

    pub fn decode(packet: &[u8]) -> Result<Self, RadioPacketError> {
        if packet.len() < RADIO_WIRE_HEADER_LEN {
            return Err(RadioPacketError::PacketTooShort {
                len: packet.len(),
                min: RADIO_WIRE_HEADER_LEN,
            });
        }

        let magic = u16::from_le_bytes([packet[0], packet[1]]);
        if magic != RADIO_WIRE_MAGIC {
            return Err(RadioPacketError::WrongMagic { magic });
        }
        if packet[2] != RADIO_WIRE_VERSION {
            return Err(RadioPacketError::UnsupportedVersion { version: packet[2] });
        }

        let payload_len = usize::from(packet[16]);
        if payload_len > RADIO_MAX_PAYLOAD_LEN {
            return Err(RadioPacketError::PayloadTooLarge {
                len: payload_len,
                max: RADIO_MAX_PAYLOAD_LEN,
            });
        }
        let expected_len = RADIO_WIRE_HEADER_LEN + payload_len;
        if packet.len() != expected_len {
            return Err(RadioPacketError::LengthMismatch {
                expected: expected_len,
                actual: packet.len(),
            });
        }

        Self::new(
            RadioDeviceId::new(u32::from_le_bytes([
                packet[4], packet[5], packet[6], packet[7],
            ])),
            RadioEventId::new(u32::from_le_bytes([
                packet[8], packet[9], packet[10], packet[11],
            ])),
            RadioChannelId::new(u32::from_le_bytes([
                packet[12], packet[13], packet[14], packet[15],
            ])),
            RadioMessageKind::from_u8(packet[3]),
            packet[RADIO_WIRE_HEADER_LEN..expected_len].to_vec(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadioPacketError {
    PacketTooShort { len: usize, min: usize },
    WrongMagic { magic: u16 },
    UnsupportedVersion { version: u8 },
    PayloadTooLarge { len: usize, max: usize },
    LengthMismatch { expected: usize, actual: usize },
}

impl fmt::Display for RadioPacketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PacketTooShort { len, min } => {
                write!(
                    f,
                    "radio packet too short: got {len}, expected at least {min}"
                )
            }
            Self::WrongMagic { magic } => {
                write!(f, "radio packet has wrong magic: 0x{magic:04x}")
            }
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported radio packet version: {version}")
            }
            Self::PayloadTooLarge { len, max } => {
                write!(f, "radio payload too large: got {len}, max {max}")
            }
            Self::LengthMismatch { expected, actual } => {
                write!(
                    f,
                    "radio packet length mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_press_round_trips_through_wire_packet() {
        let message = RadioMessage::button_press(
            RadioDeviceId::new(0x1234_5678),
            RadioEventId::new(42),
            RadioChannelId::new(7),
        );
        let mut packet = [0u8; RADIO_MAX_PACKET_LEN];
        let len = message.encode(&mut packet);

        let decoded = RadioMessage::decode(&packet[..len]).unwrap();

        assert_eq!(decoded.source_device_id(), RadioDeviceId::new(0x1234_5678));
        assert_eq!(decoded.event_id(), RadioEventId::new(42));
        assert_eq!(decoded.channel_id(), RadioChannelId::new(7));
        assert_eq!(decoded.kind(), RadioMessageKind::ButtonPress);
        assert_eq!(decoded.payload(), b"");
    }

    #[test]
    fn decode_rejects_wrong_magic() {
        let mut packet = [0u8; RADIO_MAX_PACKET_LEN];
        let len = RadioMessage::button_press(
            RadioDeviceId::new(1),
            RadioEventId::new(2),
            RadioChannelId::new(3),
        )
        .encode(&mut packet);
        packet[0] = 0;
        packet[1] = 0;

        assert!(matches!(
            RadioMessage::decode(&packet[..len]),
            Err(RadioPacketError::WrongMagic { .. })
        ));
    }

    #[test]
    fn decode_rejects_unsupported_version() {
        let mut packet = [0u8; RADIO_MAX_PACKET_LEN];
        let len = RadioMessage::button_press(
            RadioDeviceId::new(1),
            RadioEventId::new(2),
            RadioChannelId::new(3),
        )
        .encode(&mut packet);
        packet[2] = RADIO_WIRE_VERSION + 1;

        assert!(matches!(
            RadioMessage::decode(&packet[..len]),
            Err(RadioPacketError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn message_rejects_over_large_payload() {
        let payload = alloc::vec![0u8; RADIO_MAX_PAYLOAD_LEN + 1];

        assert!(matches!(
            RadioMessage::new(
                RadioDeviceId::new(1),
                RadioEventId::new(2),
                RadioChannelId::new(3),
                RadioMessageKind::Custom(99),
                payload
            ),
            Err(RadioPacketError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn decode_rejects_payload_length_mismatch() {
        let mut packet = [0u8; RADIO_MAX_PACKET_LEN];
        let len = RadioMessage::new(
            RadioDeviceId::new(1),
            RadioEventId::new(2),
            RadioChannelId::new(3),
            RadioMessageKind::Custom(99),
            b"abc".to_vec(),
        )
        .unwrap()
        .encode(&mut packet);
        packet[16] = 4;

        assert!(matches!(
            RadioMessage::decode(&packet[..len]),
            Err(RadioPacketError::LengthMismatch { .. })
        ));
    }
}
