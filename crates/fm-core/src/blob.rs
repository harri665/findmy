//! Provisioning blob: the one format every beacon reads its keys from, whether
//! it arrives over serial (ESP32), a flash page (nRF52), a file (Linux) or an
//! IOCTL (Windows driver).
//!
//! ```text
//! offset size  field
//!      0    4  magic "FMKT"
//!      4    1  version (1)
//!      5    1  key mode (schedule::KeyMode)
//!      6    4  rotation interval, seconds (u32 LE)
//!     10    2  key count N (u16 LE)
//!     12    1  status byte to advertise
//!     13    1  reserved (0)
//!     14  28M  payload: M = N public keys, or M = 1 seed in Derived mode
//! 14+28M    4  CRC-32 of everything before it (u32 LE)
//! ```
//!
//! Static and Pool blobs carry only public keys; private keys never leave the
//! server. A Derived blob carries the 28-byte seed (a secret, see `derive`) and
//! N is the size of the window of derived keys the beacon cycles through.

use crate::crc32::crc32;
use crate::schedule::KeyMode;
use crate::{AdvKey, KEY_LEN};

pub const MAGIC: [u8; 4] = *b"FMKT";
pub const VERSION: u8 = 1;
pub const HEADER_LEN: usize = 14;
const CRC_LEN: usize = 4;
/// Upper bound on keys per blob, so a corrupt count can't make firmware read
/// past its flash partition. 2048 keys at 15 min is ~3 weeks before wrapping.
pub const MAX_KEYS: u16 = 2048;

/// Number of 28-byte payload entries for a blob with `key_count` in its header.
const fn payload_entries(mode: KeyMode, key_count: u16) -> usize {
    match mode {
        KeyMode::Derived => 1,
        _ => key_count as usize,
    }
}

/// Total blob size for `key_count` keys (for Derived: a window of that many keys).
pub const fn blob_len(mode: KeyMode, key_count: u16) -> usize {
    HEADER_LEN + payload_entries(mode, key_count) * KEY_LEN + CRC_LEN
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlobError {
    TooShort,
    BadMagic,
    UnsupportedVersion(u8),
    UnknownKeyMode(u8),
    NoKeys,
    TooManyKeys(u16),
    LengthMismatch { expected: usize, actual: usize },
    BadCrc,
}

impl core::fmt::Display for BlobError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BlobError::TooShort => f.write_str("blob shorter than header"),
            BlobError::BadMagic => f.write_str("not a findmy key blob (bad magic)"),
            BlobError::UnsupportedVersion(v) => write!(f, "unsupported blob version {v}"),
            BlobError::UnknownKeyMode(m) => write!(f, "unknown key mode {m}"),
            BlobError::NoKeys => f.write_str("blob contains no keys"),
            BlobError::TooManyKeys(n) => write!(f, "{n} keys exceeds the maximum of {MAX_KEYS}"),
            BlobError::LengthMismatch { expected, actual } => {
                write!(f, "blob length {actual}, header says {expected}")
            }
            BlobError::BadCrc => f.write_str("blob CRC mismatch"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BlobError {}

/// A validated blob, borrowed from wherever it was read (no allocation).
#[derive(Clone, Copy, Debug)]
pub struct Blob<'a> {
    pub mode: KeyMode,
    pub interval_s: u32,
    pub status: u8,
    count: u16,
    payload: &'a [u8],
}

impl<'a> Blob<'a> {
    /// Validate and parse. Trailing bytes after the CRC are rejected so a
    /// truncated or oversized read is caught.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, BlobError> {
        if bytes.len() < HEADER_LEN + CRC_LEN {
            return Err(BlobError::TooShort);
        }
        if bytes[0..4] != MAGIC {
            return Err(BlobError::BadMagic);
        }
        if bytes[4] != VERSION {
            return Err(BlobError::UnsupportedVersion(bytes[4]));
        }
        let mode = KeyMode::from_u8(bytes[5]).ok_or(BlobError::UnknownKeyMode(bytes[5]))?;
        let interval_s = u32::from_le_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        let count = u16::from_le_bytes([bytes[10], bytes[11]]);
        if count == 0 {
            return Err(BlobError::NoKeys);
        }
        if count > MAX_KEYS {
            return Err(BlobError::TooManyKeys(count));
        }
        let expected = blob_len(mode, count);
        if bytes.len() != expected {
            return Err(BlobError::LengthMismatch { expected, actual: bytes.len() });
        }
        let (body, crc) = bytes.split_at(expected - CRC_LEN);
        if crc32(body) != u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]) {
            return Err(BlobError::BadCrc);
        }
        Ok(Blob { mode, interval_s, status: bytes[12], count, payload: &body[HEADER_LEN..] })
    }

    /// Validate only the header and return the full blob length, so firmware
    /// can read the header first and then exactly the right number of bytes.
    pub fn peek_len(header: &[u8]) -> Result<usize, BlobError> {
        if header.len() < HEADER_LEN {
            return Err(BlobError::TooShort);
        }
        if header[0..4] != MAGIC {
            return Err(BlobError::BadMagic);
        }
        let mode = KeyMode::from_u8(header[5]).ok_or(BlobError::UnknownKeyMode(header[5]))?;
        let count = u16::from_le_bytes([header[10], header[11]]);
        if count > MAX_KEYS {
            return Err(BlobError::TooManyKeys(count));
        }
        Ok(blob_len(mode, count))
    }

    /// Number of keys the beacon cycles through (for Derived: the window size).
    pub fn key_count(&self) -> u16 {
        self.count
    }

    /// Stored public key `index`, wrapping around so a stale index is always
    /// valid. `None` in Derived mode, where the keys are computed from the seed
    /// (see `adv_key`); the payload there is secret and is never returned.
    pub fn key(&self, index: u16) -> Option<AdvKey> {
        if self.mode == KeyMode::Derived {
            return None;
        }
        let i = (index % self.count) as usize * KEY_LEN;
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&self.payload[i..i + KEY_LEN]);
        Some(k)
    }

    /// The seed of a Derived blob.
    pub fn seed(&self) -> Option<AdvKey> {
        (self.mode == KeyMode::Derived).then(|| {
            let mut s = [0u8; KEY_LEN];
            s.copy_from_slice(&self.payload[..KEY_LEN]);
            s
        })
    }

    /// Advertisement key at `index` in any mode (derives it in Derived mode).
    #[cfg(feature = "crypto")]
    pub fn adv_key(&self, index: u16) -> AdvKey {
        match self.seed() {
            Some(seed) => crate::derive::derive_keypair(&seed, (index % self.count) as u32).adv_key(),
            None => self.key(index).expect("non-derived blob"),
        }
    }

    /// Stored keys. Empty in Derived mode.
    pub fn keys(&self) -> impl Iterator<Item = AdvKey> + '_ {
        let n = if self.mode == KeyMode::Derived { 0 } else { self.count };
        (0..n).filter_map(move |i| self.key(i))
    }
}

/// Build a Static or Pool blob. Used by the server and CLI; firmware only parses.
#[cfg(feature = "alloc")]
pub fn build(mode: KeyMode, interval_s: u32, status: u8, keys: &[AdvKey]) -> Result<alloc::vec::Vec<u8>, BlobError> {
    if keys.is_empty() || mode == KeyMode::Derived {
        return Err(BlobError::NoKeys);
    }
    let count = u16::try_from(keys.len()).unwrap_or(u16::MAX);
    if count > MAX_KEYS {
        return Err(BlobError::TooManyKeys(count));
    }
    let mut out = alloc::vec::Vec::with_capacity(blob_len(mode, count));
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(mode as u8);
    out.extend_from_slice(&interval_s.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.push(status);
    out.push(0);
    for k in keys {
        out.extend_from_slice(k);
    }
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// Build a Derived blob: a seed plus the window of `window` keys to cycle through.
#[cfg(feature = "alloc")]
pub fn build_derived(interval_s: u32, status: u8, seed: &AdvKey, window: u16) -> Result<alloc::vec::Vec<u8>, BlobError> {
    if window == 0 {
        return Err(BlobError::NoKeys);
    }
    if window > MAX_KEYS {
        return Err(BlobError::TooManyKeys(window));
    }
    let mut out = alloc::vec::Vec::with_capacity(blob_len(KeyMode::Derived, window));
    out.extend_from_slice(&MAGIC);
    out.push(VERSION);
    out.push(KeyMode::Derived as u8);
    out.extend_from_slice(&interval_s.to_le_bytes());
    out.extend_from_slice(&window.to_le_bytes());
    out.push(status);
    out.push(0);
    out.extend_from_slice(seed);
    let crc = crc32(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    Ok(out)
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;

    fn keys(n: usize) -> alloc::vec::Vec<AdvKey> {
        (0..n).map(|i| [i as u8; KEY_LEN]).collect()
    }

    #[test]
    fn round_trip() {
        let ks = keys(3);
        let bytes = build(KeyMode::Pool, 900, 0x40, &ks).unwrap();
        assert_eq!(bytes.len(), blob_len(KeyMode::Pool, 3));
        assert_eq!(Blob::peek_len(&bytes[..HEADER_LEN]).unwrap(), bytes.len());
        let blob = Blob::parse(&bytes).unwrap();
        assert_eq!(blob.mode, KeyMode::Pool);
        assert_eq!(blob.interval_s, 900);
        assert_eq!(blob.status, 0x40);
        assert_eq!(blob.key_count(), 3);
        assert_eq!(blob.keys().collect::<alloc::vec::Vec<_>>(), ks);
        assert_eq!(blob.key(4), Some(ks[1]), "index wraps");
    }

    #[cfg(feature = "crypto")]
    #[test]
    fn derived_round_trip() {
        let seed = [9u8; KEY_LEN];
        let bytes = build_derived(900, 0x40, &seed, 100).unwrap();
        assert_eq!(bytes.len(), blob_len(KeyMode::Derived, 100));
        assert_eq!(bytes.len(), HEADER_LEN + KEY_LEN + 4, "one seed, not 100 keys");
        assert_eq!(Blob::peek_len(&bytes[..HEADER_LEN]).unwrap(), bytes.len());
        let blob = Blob::parse(&bytes).unwrap();
        assert_eq!(blob.mode, KeyMode::Derived);
        assert_eq!(blob.key_count(), 100);
        assert_eq!(blob.seed(), Some(seed));
        assert_eq!(blob.key(0), None, "seed must never come back as a key");
        assert_eq!(blob.keys().count(), 0);
        let expect = crate::derive::derive_keypair(&seed, 5).adv_key();
        assert_eq!(blob.adv_key(5), expect);
        assert_eq!(blob.adv_key(105), expect, "index wraps at the window");
        assert!(build(KeyMode::Derived, 900, 0, &keys(1)).is_err());
    }

    #[test]
    fn rejects_corruption() {
        let good = build(KeyMode::Static, 0, 0, &keys(1)).unwrap();

        let mut b = good.clone();
        b[20] ^= 1;
        assert_eq!(Blob::parse(&b).unwrap_err(), BlobError::BadCrc);

        let mut b = good.clone();
        b[0] = b'X';
        assert_eq!(Blob::parse(&b).unwrap_err(), BlobError::BadMagic);

        let mut b = good.clone();
        b[4] = 9;
        assert_eq!(Blob::parse(&b).unwrap_err(), BlobError::UnsupportedVersion(9));

        let mut b = good.clone();
        b.push(0);
        assert!(matches!(Blob::parse(&b).unwrap_err(), BlobError::LengthMismatch { .. }));

        assert_eq!(Blob::parse(&good[..10]).unwrap_err(), BlobError::TooShort);
        assert_eq!(build(KeyMode::Static, 0, 0, &[]).unwrap_err(), BlobError::NoKeys);
    }
}
