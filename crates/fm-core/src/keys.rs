//! P-224 key pairs.
//!
//! Only the x-coordinate of the public point is advertised (28 bytes). A
//! finder rebuilds a point from it with an arbitrary y; that is fine because
//! the ECDH shared secret is the x-coordinate of `k·P`, which is the same for
//! `P` and `-P`.

use p224::elliptic_curve::sec1::ToEncodedPoint;
use p224::{FieldBytes, PublicKey, SecretKey};
use rand_core::CryptoRngCore;
use sha2::{Digest, Sha256};

use crate::{AdvKey, KEY_LEN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyError {
    /// Private key bytes are zero or not below the curve order.
    InvalidPrivateKey,
    /// Bytes are not the x-coordinate of a point on P-224.
    NotOnCurve,
}

impl core::fmt::Display for KeyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            KeyError::InvalidPrivateKey => "invalid P-224 private key",
            KeyError::NotOnCurve => "not a valid P-224 x-coordinate",
        })
    }
}

#[cfg(feature = "std")]
impl std::error::Error for KeyError {}

/// A device key: the private half stays on the server, the advertisement key
/// goes to the beacon.
#[derive(Clone)]
pub struct KeyPair {
    secret: SecretKey,
}

impl KeyPair {
    pub fn generate(rng: &mut impl CryptoRngCore) -> Self {
        KeyPair { secret: SecretKey::random(rng) }
    }

    pub fn from_private_bytes(bytes: &[u8; KEY_LEN]) -> Result<Self, KeyError> {
        let secret = SecretKey::from_bytes(&FieldBytes::from(*bytes))
            .map_err(|_| KeyError::InvalidPrivateKey)?;
        Ok(KeyPair { secret })
    }

    /// The 28-byte private scalar, big-endian. Handle with care.
    pub fn private_bytes(&self) -> [u8; KEY_LEN] {
        let mut out = [0u8; KEY_LEN];
        out.copy_from_slice(&self.secret.to_bytes());
        out
    }

    /// The key to advertise: x-coordinate of the public point.
    pub fn adv_key(&self) -> AdvKey {
        let point = self.secret.public_key().to_encoded_point(false);
        let mut out = [0u8; KEY_LEN];
        // An uncompressed point from a valid secret always has an x-coordinate.
        out.copy_from_slice(point.x().expect("non-identity point"));
        out
    }

    /// The ID Apple indexes reports under: `SHA256(adv_key)`.
    pub fn hashed_adv_key(&self) -> [u8; 32] {
        hashed_adv_key(&self.adv_key())
    }

    pub(crate) fn secret(&self) -> &SecretKey {
        &self.secret
    }
}

impl core::fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never print the private key.
        f.debug_struct("KeyPair").field("adv_key", &self.adv_key()).finish_non_exhaustive()
    }
}

/// `SHA256(adv_key)`, the report ID used when fetching from Apple.
pub fn hashed_adv_key(adv_key: &AdvKey) -> [u8; 32] {
    Sha256::digest(adv_key).into()
}

/// Rebuild a public point from an advertised x-coordinate (even y), as a
/// finder device does.
pub fn public_from_adv_key(adv_key: &AdvKey) -> Result<PublicKey, KeyError> {
    let mut sec1 = [0u8; 1 + KEY_LEN];
    sec1[0] = 0x02;
    sec1[1..].copy_from_slice(adv_key);
    PublicKey::from_sec1_bytes(&sec1).map_err(|_| KeyError::NotOnCurve)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand_core::OsRng;

    #[test]
    fn private_bytes_round_trip() {
        let kp = KeyPair::generate(&mut OsRng);
        let again = KeyPair::from_private_bytes(&kp.private_bytes()).unwrap();
        assert_eq!(kp.adv_key(), again.adv_key());
    }

    #[test]
    fn adv_key_is_on_curve() {
        for _ in 0..16 {
            let kp = KeyPair::generate(&mut OsRng);
            public_from_adv_key(&kp.adv_key()).unwrap();
        }
    }

    #[test]
    fn rejects_bad_private_keys() {
        assert_eq!(KeyPair::from_private_bytes(&[0; KEY_LEN]).unwrap_err(), KeyError::InvalidPrivateKey);
        assert_eq!(KeyPair::from_private_bytes(&[0xFF; KEY_LEN]).unwrap_err(), KeyError::InvalidPrivateKey);
    }

    #[test]
    fn hashed_key_is_sha256() {
        // SHA-256 of 28 zero bytes.
        let h = hashed_adv_key(&[0; KEY_LEN]);
        assert_eq!(h[..4], [0x3a, 0xdd, 0xfb, 0x14]);
    }
}
