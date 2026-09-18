//! Configuration from environment variables.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `FM_BIND` | `0.0.0.0:3001` | listen address (the website's nginx proxies `/api` here) |
//! | `FM_DATA_DIR` | `./data` | SQLite database and generated master key |
//! | `FM_ADMIN_PASSWORD` | *required* | password for the dashboard |
//! | `FM_MASTER_KEY` | generated | base64 32-byte key that encrypts private keys at rest |
//! | `RUST_LOG` | `info` | log filter |

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

pub struct Config {
    pub bind: String,
    pub data_dir: PathBuf,
    pub admin_password: String,
    pub master_key: [u8; 32],
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = std::env::var("FM_BIND").unwrap_or_else(|_| "0.0.0.0:3001".into());
        let data_dir = PathBuf::from(std::env::var("FM_DATA_DIR").unwrap_or_else(|_| "data".into()));
        std::fs::create_dir_all(&data_dir).with_context(|| format!("creating {}", data_dir.display()))?;

        let admin_password = match std::env::var("FM_ADMIN_PASSWORD") {
            Ok(p) if p.len() >= 8 => p,
            Ok(_) => bail!("FM_ADMIN_PASSWORD must be at least 8 characters"),
            Err(_) => bail!("FM_ADMIN_PASSWORD is not set; the dashboard needs a password"),
        };

        let master_key = match std::env::var("FM_MASTER_KEY") {
            Ok(v) => decode_key(&v).context("FM_MASTER_KEY")?,
            Err(_) => load_or_create_key(&data_dir.join("master.key"))?,
        };

        Ok(Config { bind, data_dir, admin_password, master_key })
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("findmy.db")
    }
}

fn decode_key(b64: &str) -> Result<[u8; 32]> {
    B64.decode(b64.trim())
        .context("not valid base64")?
        .try_into()
        .map_err(|_| anyhow::anyhow!("must decode to exactly 32 bytes"))
}

/// Without `FM_MASTER_KEY`, keep a generated key next to the database. That
/// still protects database copies and backups that don't include it; set the
/// env var to keep the key out of the data volume entirely.
fn load_or_create_key(path: &std::path::Path) -> Result<[u8; 32]> {
    if path.exists() {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        return decode_key(&text).with_context(|| format!("parsing {}", path.display()));
    }
    let mut key = [0u8; 32];
    getrandom::getrandom(&mut key).map_err(|e| anyhow::anyhow!("OS randomness: {e}"))?;
    std::fs::write(path, B64.encode(key) + "\n").with_context(|| format!("writing {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    tracing::warn!("generated a new master key at {} (set FM_MASTER_KEY to manage it yourself)", path.display());
    Ok(key)
}
