//! Encryption at rest for private keys, plus small hashing helpers.

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use sha2::{Digest, Sha256};

/// Seals secrets as `nonce (12) || ciphertext+tag` with ChaCha20-Poly1305.
pub struct Sealer {
    cipher: ChaCha20Poly1305,
}

impl Sealer {
    pub fn new(key: &[u8; 32]) -> Self {
        Sealer { cipher: ChaCha20Poly1305::new(&Key::from(*key)) }
    }

    pub fn seal(&self, plaintext: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 12];
        getrandom::getrandom(&mut nonce).expect("OS randomness");
        let ct = self.cipher.encrypt(&Nonce::from(nonce), plaintext).expect("encryption cannot fail");
        let mut out = Vec::with_capacity(12 + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        out
    }

    /// `None` if the data was tampered with or sealed under another key.
    pub fn open(&self, sealed: &[u8]) -> Option<Vec<u8>> {
        if sealed.len() < 12 {
            return None;
        }
        let (nonce, ct) = sealed.split_at(12);
        let nonce: [u8; 12] = nonce.try_into().ok()?;
        self.cipher.decrypt(&Nonce::from(nonce), ct).ok()
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Compare two digests without an early exit.
pub fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// A random token for sessions and device agents.
pub fn random_token() -> String {
    let mut b = [0u8; 32];
    getrandom::getrandom(&mut b).expect("OS randomness");
    hex(&b)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open() {
        let s = Sealer::new(&[7; 32]);
        let sealed = s.seal(b"secret");
        assert_eq!(s.open(&sealed).unwrap(), b"secret");
        assert!(Sealer::new(&[8; 32]).open(&sealed).is_none(), "wrong key");
        let mut bad = sealed.clone();
        bad[13] ^= 1;
        assert!(s.open(&bad).is_none(), "tampered");
        assert_ne!(s.seal(b"secret"), sealed, "fresh nonce each time");
    }

    #[test]
    fn helpers() {
        assert!(ct_eq(&sha256(b"a"), &sha256(b"a")));
        assert!(!ct_eq(&sha256(b"a"), &sha256(b"b")));
        assert_eq!(random_token().len(), 64);
        assert_eq!(sha256_hex(b"").len(), 64);
    }
}
