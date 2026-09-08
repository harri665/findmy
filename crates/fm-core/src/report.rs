//! Location reports uploaded by finder devices.
//!
//! ```text
//! offset size  field
//!      0    4  timestamp, seconds since 2001-01-01 (u32 BE)
//!      4    1  confidence
//!      5   57  finder's ephemeral P-224 public key (SEC1 uncompressed, 04||x||y)
//!     62   10  ciphertext
//!     72   16  AES-GCM tag
//! ```
//!
//! Newer reports are 89 bytes with one extra byte at offset 4; it is dropped
//! before parsing. Decryption:
//!
//! ```text
//! shared = ECDH(our private key, ephemeral key)              // 28-byte x
//! k      = SHA256(shared || 00 00 00 01 || ephemeral key)    // ANSI X9.63 KDF
//! plain  = AES-128-GCM(key = k[0..16], iv = k[16..32]).open(ciphertext || tag)
//! plain  = lat (i32 BE, 1e-7 deg) | lon (i32 BE) | accuracy (m) | status
//! ```

use aes_gcm::aead::consts::U16;
use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::aes::Aes128;
use aes_gcm::{AesGcm, Nonce};
use p224::ecdh::diffie_hellman;
use p224::PublicKey;
use sha2::{Digest, Sha256};

use crate::keys::KeyPair;
use crate::APPLE_EPOCH_OFFSET;

/// AES-128-GCM with Apple's 16-byte IV.
type Cipher = AesGcm<Aes128, U16>;

pub const REPORT_LEN: usize = 88;
const EPH_KEY_LEN: usize = 57;
const CIPHERTEXT_LEN: usize = 10;
const TAG_LEN: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportError {
    BadLength(usize),
    BadEphemeralKey,
    /// Authentication failed: wrong key, or a corrupted report.
    DecryptFailed,
}

impl core::fmt::Display for ReportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ReportError::BadLength(n) => write!(f, "report is {n} bytes, expected 88 or 89"),
            ReportError::BadEphemeralKey => f.write_str("report has an invalid ephemeral key"),
            ReportError::DecryptFailed => f.write_str("report decryption failed (wrong key or corrupt data)"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ReportError {}

/// A decrypted location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    /// When the finder saw the beacon, Unix seconds.
    pub timestamp: i64,
    pub confidence: u8,
    pub latitude: f64,
    pub longitude: f64,
    /// Horizontal accuracy, metres.
    pub accuracy: u8,
    /// The status byte the beacon was advertising.
    pub status: u8,
}

/// Decrypt one report with the key pair whose hashed key it was fetched under.
pub fn decrypt(key: &KeyPair, report: &[u8]) -> Result<Location, ReportError> {
    let mut buf = [0u8; REPORT_LEN];
    match report.len() {
        REPORT_LEN => buf.copy_from_slice(report),
        n if n == REPORT_LEN + 1 => {
            buf[..4].copy_from_slice(&report[..4]);
            buf[4..].copy_from_slice(&report[5..]);
        }
        n => return Err(ReportError::BadLength(n)),
    }

    let apple_secs = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let confidence = buf[4];
    let eph_bytes = &buf[5..5 + EPH_KEY_LEN];
    let sealed = &buf[5 + EPH_KEY_LEN..];
    debug_assert_eq!(sealed.len(), CIPHERTEXT_LEN + TAG_LEN);

    let eph = PublicKey::from_sec1_bytes(eph_bytes).map_err(|_| ReportError::BadEphemeralKey)?;
    let shared = diffie_hellman(key.secret().to_nonzero_scalar(), eph.as_affine());
    let plain = open(shared.raw_secret_bytes(), eph_bytes, sealed)?;

    Ok(Location {
        timestamp: apple_secs as i64 + APPLE_EPOCH_OFFSET,
        confidence,
        latitude: i32::from_be_bytes([plain[0], plain[1], plain[2], plain[3]]) as f64 / 1e7,
        longitude: i32::from_be_bytes([plain[4], plain[5], plain[6], plain[7]]) as f64 / 1e7,
        accuracy: plain[8],
        status: plain[9],
    })
}

/// ANSI X9.63 KDF with SHA-256, one block: `SHA256(z || counter=1 || shared_info)`,
/// split into the AES key and IV.
fn kdf(shared_x: &[u8], eph_key: &[u8]) -> (Cipher, Nonce<U16>) {
    let mut h = Sha256::new();
    h.update(shared_x);
    h.update(1u32.to_be_bytes());
    h.update(eph_key);
    let k: [u8; 32] = h.finalize().into();
    let (key, iv) = k.split_at(16);
    let cipher = Cipher::new_from_slice(key).expect("16-byte key");
    let iv: [u8; 16] = iv.try_into().expect("16-byte iv");
    (cipher, Nonce::<U16>::from(iv))
}

fn open(shared_x: &[u8], eph_key: &[u8], sealed: &[u8]) -> Result<[u8; CIPHERTEXT_LEN], ReportError> {
    let (cipher, iv) = kdf(shared_x, eph_key);
    let plain = cipher.decrypt(&iv, sealed).map_err(|_| ReportError::DecryptFailed)?;
    let mut out = [0u8; CIPHERTEXT_LEN];
    if plain.len() != CIPHERTEXT_LEN {
        return Err(ReportError::DecryptFailed);
    }
    out.copy_from_slice(&plain);
    Ok(out)
}

/// Produce a report the way a finder device does. For tests and the server's
/// test suite only. Real finders are iPhones.
#[cfg(any(test, feature = "finder-sim"))]
pub fn seal(
    adv_key: &crate::AdvKey,
    ephemeral: &p224::SecretKey,
    location: &Location,
) -> Result<alloc::vec::Vec<u8>, crate::keys::KeyError> {
    use p224::elliptic_curve::sec1::ToEncodedPoint;

    let target = crate::keys::public_from_adv_key(adv_key)?;
    let eph_point = ephemeral.public_key().to_encoded_point(false);
    let eph_bytes = eph_point.as_bytes();
    let shared = diffie_hellman(ephemeral.to_nonzero_scalar(), target.as_affine());
    let (cipher, iv) = kdf(shared.raw_secret_bytes(), eph_bytes);

    let mut plain = [0u8; CIPHERTEXT_LEN];
    plain[0..4].copy_from_slice(&((location.latitude * 1e7).round() as i32).to_be_bytes());
    plain[4..8].copy_from_slice(&((location.longitude * 1e7).round() as i32).to_be_bytes());
    plain[8] = location.accuracy;
    plain[9] = location.status;

    let sealed = cipher
        .encrypt(&iv, plain.as_slice())
        .expect("AES-GCM encryption is infallible for small inputs");

    let mut out = alloc::vec::Vec::with_capacity(REPORT_LEN);
    let apple_secs = (location.timestamp - APPLE_EPOCH_OFFSET) as u32;
    out.extend_from_slice(&apple_secs.to_be_bytes());
    out.push(location.confidence);
    out.extend_from_slice(eph_bytes);
    out.extend_from_slice(&sealed);
    debug_assert_eq!(out.len(), REPORT_LEN);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    fn boulder() -> Location {
        Location {
            timestamp: 1_790_000_000,
            confidence: 2,
            latitude: 40.0149856,
            longitude: -105.2705456,
            accuracy: 35,
            status: 0x40,
        }
    }

    #[test]
    fn seal_then_decrypt() {
        let owner = KeyPair::generate(&mut OsRng);
        let eph = p224::SecretKey::random(&mut OsRng);
        let report = seal(&owner.adv_key(), &eph, &boulder()).unwrap();
        assert_eq!(report.len(), REPORT_LEN);
        assert_eq!(decrypt(&owner, &report).unwrap(), boulder());
    }

    #[test]
    fn accepts_89_byte_format() {
        let owner = KeyPair::generate(&mut OsRng);
        let eph = p224::SecretKey::random(&mut OsRng);
        let mut report = seal(&owner.adv_key(), &eph, &boulder()).unwrap();
        report.insert(4, 0xAA);
        assert_eq!(decrypt(&owner, &report).unwrap(), boulder());
    }

    #[test]
    fn wrong_key_or_tamper_fails() {
        let owner = KeyPair::generate(&mut OsRng);
        let other = KeyPair::generate(&mut OsRng);
        let eph = p224::SecretKey::random(&mut OsRng);
        let report = seal(&owner.adv_key(), &eph, &boulder()).unwrap();
        assert_eq!(decrypt(&other, &report).unwrap_err(), ReportError::DecryptFailed);

        let mut bad = report.clone();
        bad[70] ^= 1;
        assert_eq!(decrypt(&owner, &bad).unwrap_err(), ReportError::DecryptFailed);

        assert_eq!(decrypt(&owner, &report[..50]).unwrap_err(), ReportError::BadLength(50));
    }
}
