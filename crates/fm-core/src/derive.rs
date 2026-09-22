//! Derived key mode: one 28-byte seed yields an unbounded sequence of keys, so
//! a beacon stores a single secret instead of a pool of public keys.
//!
//! Key `i` is `SHA256("findmy/derive/v1" || seed || i (u32 BE) || ctr (u8))`
//! truncated to 28 bytes, used as the private scalar. `ctr` starts at 0 and is
//! bumped in the (≈2^-96) case the value is not a valid P-224 scalar.
//!
//! The seed is a secret: whoever holds it can compute every private key.

use sha2::{Digest, Sha256};

use crate::keys::KeyPair;
use crate::KEY_LEN;

pub const SEED_LEN: usize = KEY_LEN;
pub type Seed = [u8; SEED_LEN];

const DOMAIN: &[u8] = b"findmy/derive/v1";

/// The key pair for position `index` in the sequence of `seed`.
pub fn derive_keypair(seed: &Seed, index: u32) -> KeyPair {
    for ctr in 0..=u8::MAX {
        let mut h = Sha256::new();
        h.update(DOMAIN);
        h.update(seed);
        h.update(index.to_be_bytes());
        h.update([ctr]);
        let digest = h.finalize();
        let mut scalar = [0u8; KEY_LEN];
        scalar.copy_from_slice(&digest[..KEY_LEN]);
        if let Ok(kp) = KeyPair::from_private_bytes(&scalar) {
            return kp;
        }
    }
    unreachable!("256 consecutive invalid P-224 scalars")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_distinct() {
        let seed = [7u8; SEED_LEN];
        let a = derive_keypair(&seed, 0);
        assert_eq!(a.adv_key(), derive_keypair(&seed, 0).adv_key());
        assert_ne!(a.adv_key(), derive_keypair(&seed, 1).adv_key());
        assert_ne!(a.adv_key(), derive_keypair(&[8u8; SEED_LEN], 0).adv_key());
    }
}
