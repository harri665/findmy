//! Device key file: everything the CLI knows about one device, as JSON.
//!
//! It holds **private keys in plain text**. It's meant for development and for
//! importing into fm-server, which stores keys encrypted. Keep it out of git
//! (`*.keys.json` is ignored).

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use fm_core::keys::KeyPair;
use fm_core::schedule::KeyMode;
use fm_core::{AdvKey, KEY_LEN};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct KeyFile {
    pub name: String,
    /// "static" or "pool".
    pub mode: String,
    pub interval_s: u32,
    pub status: u8,
    pub keys: Vec<KeyEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct KeyEntry {
    /// Private key, hex. Never sent to a beacon.
    pub private: String,
    /// Advertisement key, base64 (the format Apple-facing tools use).
    pub adv: String,
    /// SHA256(adv key), base64: the ID reports are fetched under.
    pub hashed: String,
}

impl KeyEntry {
    pub fn from_pair(kp: &KeyPair) -> Self {
        KeyEntry {
            private: hex(&kp.private_bytes()),
            adv: B64.encode(kp.adv_key()),
            hashed: B64.encode(kp.hashed_adv_key()),
        }
    }

    pub fn pair(&self) -> Result<KeyPair> {
        let bytes: [u8; KEY_LEN] = unhex(&self.private)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("private key must be {KEY_LEN} bytes"))?;
        let kp = KeyPair::from_private_bytes(&bytes)?;
        // Catch hand-edited files where the columns no longer match.
        if B64.encode(kp.adv_key()) != self.adv {
            bail!("adv key does not match private key {}", self.private);
        }
        Ok(kp)
    }

    pub fn adv_key(&self) -> Result<AdvKey> {
        B64.decode(&self.adv)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("adv key must be {KEY_LEN} bytes"))
    }
}

impl KeyFile {
    pub fn key_mode(&self) -> Result<KeyMode> {
        parse_mode(&self.mode)
    }

    pub fn load(path: &str) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let kf: KeyFile = serde_json::from_str(&text).with_context(|| format!("parsing {path}"))?;
        if kf.keys.is_empty() {
            bail!("{path} has no keys");
        }
        kf.key_mode()?;
        Ok(kf)
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text + "\n").with_context(|| format!("writing {path}"))
    }

    /// Find the key whose hashed ID is `hashed` (base64).
    pub fn by_hashed(&self, hashed: &str) -> Option<&KeyEntry> {
        self.keys.iter().find(|k| k.hashed == hashed)
    }
}

pub fn parse_mode(s: &str) -> Result<KeyMode> {
    match s {
        "static" => Ok(KeyMode::Static),
        "pool" => Ok(KeyMode::Pool),
        other => bail!("unknown key mode {other:?} (expected \"static\" or \"pool\")"),
    }
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(s: &str) -> Result<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        bail!("odd-length hex string");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).with_context(|| format!("bad hex at {i}")))
        .collect()
}
