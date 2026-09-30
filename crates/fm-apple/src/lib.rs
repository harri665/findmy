//! Retrieve encrypted reports with an authenticated Apple session and a
//! configured anisette provider. No private beacon keys leave the server.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const ENDPOINT: &str = "https://gateway.icloud.com/findmyservice/v2/fetch";
pub const BATCH_SIZE: usize = 256;

/// Deliberately does not implement Debug: it contains a bearer credential.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub dsid: String,
    #[serde(rename = "searchPartyToken")]
    pub search_party_token: String,
    pub uid: String,
    pub device_id: String,
}

impl Credentials {
    /// Accept our compact login export and a FindMy.py account export.
    /// Only the fields needed to retrieve reports are retained.
    pub fn from_export(value: &Value) -> Result<Self> {
        let account = value.get("type").and_then(Value::as_str) == Some("account");
        let data = if account {
            &value["login"]["data"]
        } else {
            value
        };
        let dsid = match &data["dsid"] {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            _ => String::new(),
        };
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let credentials = Self {
            dsid,
            search_party_token: text(if account {
                &data["mobileme_data"]["tokens"]["searchPartyToken"]
            } else {
                &data["searchPartyToken"]
            }),
            uid: text(if account {
                &value["ids"]["uid"]
            } else {
                &value["uid"]
            }),
            device_id: text(if account {
                &value["ids"]["devid"]
            } else {
                &value["device_id"]
            }),
        };
        credentials.validate()?;
        Ok(credentials)
    }

    pub fn validate(&self) -> Result<()> {
        if self.dsid.is_empty()
            || self.dsid.len() > 32
            || !self.dsid.bytes().all(|b| b.is_ascii_digit())
        {
            bail!("session must contain a numeric dsid");
        }
        for (name, value) in [
            ("searchPartyToken", &self.search_party_token),
            ("uid", &self.uid),
            ("device_id", &self.device_id),
        ] {
            if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
                bail!("session is missing a valid {name}");
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Report {
    #[serde(alias = "id")]
    pub hashed: String,
    pub payload: String,
}

pub struct Client {
    http: HttpClient,
    endpoint: String,
}

impl Client {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http: fm_anisette::http_client()?,
            endpoint: ENDPOINT.into(),
        })
    }

    /// Caller supplies batches so successfully fetched batches can be saved
    /// even if a later request fails. Dates are Unix seconds, encoded as ms.
    pub async fn fetch(
        &self,
        credentials: &Credentials,
        provider: &str,
        groups: &[Vec<String>],
        now: i64,
        time: &str,
    ) -> Result<Vec<Report>> {
        credentials.validate()?;
        if groups.iter().map(Vec::len).sum::<usize>() > BATCH_SIZE {
            bail!("too many keys in fetch batch");
        }
        let body = request_body(groups, now);
        for attempt in 0..3 {
            let headers = fm_anisette::headers(
                &self.http,
                provider,
                &credentials.uid,
                &credentials.device_id,
                time,
            )
            .await?;
            let response = self
                .http
                .post(&self.endpoint)
                .basic_auth(&credentials.dsid, Some(&credentials.search_party_token))
                .headers(headers)
                .json(&body)
                .send()
                .await
                .context("contacting Apple report service")?;
            match response.status().as_u16() {
                401 | 403 => bail!(
                    "Apple session expired or was rejected; sign in again and import a new session"
                ),
                200 => {}
                status => bail!("Apple report service returned HTTP {status}"),
            }
            let bytes = fm_anisette::read_limited(response, 8 * 1024 * 1024).await?;
            if bytes.iter().all(u8::is_ascii_whitespace) && attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_secs(2 * (attempt + 1))).await;
                continue;
            }
            return parse_response(&bytes);
        }
        unreachable!()
    }
}

fn request_body(groups: &[Vec<String>], now: i64) -> Value {
    let start = (now - 7 * 86400) * 1000;
    json!({
        "clientContext": {"clientBundleIdentifier":"com.apple.icloud.searchpartyuseragent", "policy":"foregroundClient"},
        "fetch": groups.iter().map(|keys| json!({
            "ownedDeviceIds":[], "keyType":1, "startDate":start, "startDateSecondary":start,
            "endDate":now * 1000, "primaryIds":keys, "secondaryIds":[]
        })).collect::<Vec<_>>()
    })
}

fn parse_response(bytes: &[u8]) -> Result<Vec<Report>> {
    let value: Value =
        serde_json::from_slice(bytes).context("Apple returned empty or invalid JSON")?;
    let locations = &value["acsnLocations"];
    if locations["statusCode"] != "200" && locations["statusCode"] != 200 {
        bail!("Apple report service did not return a successful location response");
    }
    let entries = locations["locationPayload"]
        .as_array()
        .context("Apple response is missing locationPayload")?;
    let mut reports = Vec::new();
    for entry in entries {
        let id = entry["id"]
            .as_str()
            .context("Apple response has a missing report ID")?;
        let hashed = B64.encode(B64.decode(id).context("Apple report ID is not base64")?);
        if B64.decode(&hashed)?.len() != 32 {
            bail!("Apple report ID has an invalid length");
        }
        for payload in entry["locationInfo"]
            .as_array()
            .context("Apple response is missing locationInfo")?
        {
            reports.push(Report {
                hashed: hashed.clone(),
                payload: payload
                    .as_str()
                    .context("Apple report payload is not a string")?
                    .to_owned(),
            });
        }
    }
    Ok(reports)
}

#[cfg(test)]
mod tests;
