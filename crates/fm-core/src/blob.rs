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
//!     14  28N  public keys
//! 14+28N    4  CRC-32 of everything before it (u32 LE)
//! ```
//!
//! Only public keys are in the blob. Private keys never leave the server.

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

/// Total blob size for `key_count` keys.
pub const fn blob_len(key_count: u16) -> usize {
    HEADER_LEN + key_count as usize * KEY_LEN + CRC_LEN
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
    keys: &'a [u8],
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
        let expected = blob_len(count);
        if bytes.len() != expected {
            return Err(BlobError::LengthMismatch { expected, actual: bytes.len() });
        }
        let (body, crc) = bytes.split_at(expected - CRC_LEN);
        if crc32(body) != u32::from_le_bytes([crc[0], crc[1], crc[2], crc[3]]) {
            return Err(BlobError::BadCrc);
        }
        Ok(Blob { mode, interval_s, status: bytes[12], keys: &body[HEADER_LEN..] })
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
        let count = u16::from_le_bytes([header[10], header[11]]);
        if count > MAX_KEYS {
            return Err(BlobError::TooManyKeys(count));
        }
        Ok(blob_len(count))
    }

    pub fn key_count(&self) -> u16 {
        (self.keys.len() / KEY_LEN) as u16
    }

    /// Key `index`, wrapping around so a stale index is always valid.
    pub fn key(&self, index: u16) -> AdvKey {
        let i = (index % self.key_count()) as usize * KEY_LEN;
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&self.keys[i..i + KEY_LEN]);
        k
    }

    pub fn keys(&self) -> impl Iterator<Item = AdvKey> + '_ {
        (0..self.key_count()).map(move |i| self.key(i))
    }
}

/// Build a blob. Used by the server and CLI; firmware only parses.
#[cfg(feature = "alloc")]
pub fn build(mode: KeyMode, interval_s: u32, status: u8, keys: &[AdvKey]) -> Result<alloc::vec::Vec<u8>, BlobError> {
    if keys.is_empty() {
        return Err(BlobError::NoKeys);
    }
    let count = u16::try_from(keys.len()).unwrap_or(u16::MAX);
    if count > MAX_KEYS {
        return Err(BlobError::TooManyKeys(count));
    }
    let mut out = alloc::vec::Vec::with_capacity(blob_len(count));
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
        assert_eq!(bytes.len(), blob_len(3));
        assert_eq!(Blob::peek_len(&bytes[..HEADER_LEN]).unwrap(), bytes.len());
        let blob = Blob::parse(&bytes).unwrap();
        assert_eq!(blob.mode, KeyMode::Pool);
        assert_eq!(blob.interval_s, 900);
        assert_eq!(blob.status, 0x40);
        assert_eq!(blob.key_count(), 3);
        assert_eq!(blob.keys().collect::<alloc::vec::Vec<_>>(), ks);
        assert_eq!(blob.key(4), ks[1], "index wraps");
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
