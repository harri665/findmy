//! Offline-finding advertisement format.
//!
//! A beacon spreads its 28-byte public key over two places:
//!
//! - the BLE **random static address**: `key[0..6]`, with the top two bits of
//!   the first byte forced to `0b11` (required for a random static address);
//! - Apple **manufacturer data**: the remaining `key[6..28]`, plus the two
//!   bits the address overwrote.
//!
//! ```text
//! 1E FF 4C 00 12 19 <status> <key[6..28]: 22 bytes> <key[0] >> 6> <hint>
//! ```
//!
//! That is exactly 31 bytes, the whole legacy advertising payload, so there is
//! no room for a Flags AD structure. Beacons must advertise non-connectable.

use crate::{AdvKey, KEY_LEN};

/// Total advertising payload length (legacy advertising maximum).
pub const ADV_LEN: usize = 31;

/// Apple's Bluetooth SIG company identifier, little-endian on the air.
pub const APPLE_COMPANY_ID: u16 = 0x004C;
/// Apple's offline-finding message type inside the manufacturer data.
pub const OFFLINE_FINDING_TYPE: u8 = 0x12;
/// Length of the offline-finding message body.
const OFFLINE_FINDING_LEN: u8 = 0x19;

/// Status byte bits. Apple uses the upper bits for battery level.
pub mod status {
    pub const BATTERY_FULL: u8 = 0b0000_0000;
    pub const BATTERY_MEDIUM: u8 = 0b0100_0000;
    pub const BATTERY_LOW: u8 = 0b1000_0000;
    pub const BATTERY_CRITICAL: u8 = 0b1100_0000;
    pub const BATTERY_MASK: u8 = 0b1100_0000;
}

/// An encoded advertisement, ready to hand to a BLE controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Advertisement {
    /// Random static address, most significant byte first (`address[0]` is the
    /// byte shown first in `C3:11:22:33:44:55`).
    pub address: [u8; 6],
    /// Raw advertising data (one manufacturer-specific AD structure).
    pub data: [u8; ADV_LEN],
}

impl Advertisement {
    /// Address in HCI byte order (least significant byte first), as
    /// `LE Set Random Address` and friends expect it.
    pub fn address_hci(&self) -> [u8; 6] {
        let mut a = self.address;
        a.reverse();
        a
    }
}

/// Build the advertisement for `key`.
pub fn encode(key: &AdvKey, status: u8) -> Advertisement {
    let mut address = [0u8; 6];
    address.copy_from_slice(&key[0..6]);
    address[0] |= 0b1100_0000;

    let mut data = [0u8; ADV_LEN];
    data[0] = (ADV_LEN - 1) as u8; // AD length: everything after this byte
    data[1] = 0xFF; // AD type: manufacturer specific data
    data[2..4].copy_from_slice(&APPLE_COMPANY_ID.to_le_bytes());
    data[4] = OFFLINE_FINDING_TYPE;
    data[5] = OFFLINE_FINDING_LEN;
    data[6] = status;
    data[7..29].copy_from_slice(&key[6..KEY_LEN]);
    data[29] = key[0] >> 6;
    data[30] = 0x00; // hint

    Advertisement { address, data }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// No Apple offline-finding manufacturer data in the payload.
    NotOfflineFinding,
    /// AD structures run past the end of the payload.
    Malformed,
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            DecodeError::NotOfflineFinding => "no offline-finding advertisement in payload",
            DecodeError::Malformed => "malformed advertising data",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}

/// A decoded advertisement: the public key and the status byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub key: AdvKey,
    pub status: u8,
}

/// Rebuild the public key the way a finder device does, from the advertiser
/// `address` (most significant byte first) and the raw advertising `payload`.
/// The payload may contain other AD structures as well.
pub fn decode(address: &[u8; 6], payload: &[u8]) -> Result<Decoded, DecodeError> {
    let body = find_offline_finding(payload)?;
    // body = <status> <22 key bytes> <top bits> [<hint>]
    let mut key = [0u8; KEY_LEN];
    key[0] = (address[0] & 0b0011_1111) | ((body[23] & 0b11) << 6);
    key[1..6].copy_from_slice(&address[1..6]);
    key[6..KEY_LEN].copy_from_slice(&body[1..23]);
    Ok(Decoded { key, status: body[0] })
}

/// Walk the AD structures and return the offline-finding message body.
fn find_offline_finding(payload: &[u8]) -> Result<&[u8], DecodeError> {
    let mut rest = payload;
    while let Some((&len, tail)) = rest.split_first() {
        let len = len as usize;
        if len == 0 {
            break; // zero padding ends the significant part
        }
        if tail.len() < len {
            return Err(DecodeError::Malformed);
        }
        let (ad, next) = tail.split_at(len);
        // ad = <type> <company id: 2> <of type> <of len> <body...>
        if ad.len() >= 5
            && ad[0] == 0xFF
            && u16::from_le_bytes([ad[1], ad[2]]) == APPLE_COMPANY_ID
            && ad[3] == OFFLINE_FINDING_TYPE
        {
            let body = &ad[5..];
            let declared = ad[4] as usize;
            // Need at least status + 22 key bytes + top bits.
            if declared < 24 || body.len() < 24 {
                return Err(DecodeError::Malformed);
            }
            return Ok(body);
        }
        rest = next;
    }
    Err(DecodeError::NotOfflineFinding)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_key() -> AdvKey {
        let mut k = [0u8; KEY_LEN];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(37).wrapping_add(0x9B);
        }
        k
    }

    #[test]
    fn layout() {
        let key = sample_key();
        let adv = encode(&key, status::BATTERY_LOW);
        assert_eq!(&adv.data[..6], &[0x1E, 0xFF, 0x4C, 0x00, 0x12, 0x19]);
        assert_eq!(adv.data[6], status::BATTERY_LOW);
        assert_eq!(adv.address[0] >> 6, 0b11, "random static address");
        assert_eq!(&adv.address[1..], &key[1..6]);
        assert_eq!(&adv.data[7..29], &key[6..]);
        assert_eq!(adv.data[29], key[0] >> 6);
        let mut le = adv.address;
        le.reverse();
        assert_eq!(adv.address_hci(), le);
    }

    #[test]
    fn round_trip_all_top_bits() {
        for top in 0..4u8 {
            let mut key = sample_key();
            key[0] = (key[0] & 0x3F) | (top << 6);
            let adv = encode(&key, 0);
            let d = decode(&adv.address, &adv.data).unwrap();
            assert_eq!(d.key, key);
            assert_eq!(d.status, 0);
        }
    }

    #[test]
    fn decode_with_flags_prefix() {
        let key = sample_key();
        let adv = encode(&key, 0x10);
        let mut payload = [0u8; 34];
        payload[..3].copy_from_slice(&[0x02, 0x01, 0x06]);
        payload[3..].copy_from_slice(&adv.data);
        assert_eq!(decode(&adv.address, &payload).unwrap().key, key);
    }

    #[test]
    fn decode_rejects_other_data() {
        let addr = [0xC0, 1, 2, 3, 4, 5];
        // iBeacon: Apple company id but type 0x02.
        let ibeacon = [0x1A, 0xFF, 0x4C, 0x00, 0x02, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(decode(&addr, &ibeacon), Err(DecodeError::NotOfflineFinding));
        assert_eq!(decode(&addr, &[0x05, 0xFF, 0x4C]), Err(DecodeError::Malformed));
        assert_eq!(decode(&addr, &[]), Err(DecodeError::NotOfflineFinding));
    }
}
