//! HTTP client for a separately operated anisette provider.
//! Apple account credentials are never sent to the provider.

use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Response, Url};
use serde_json::Value;
use std::time::Duration;

pub fn validate_url(value: &str) -> Result<Url> {
    let url = Url::parse(value).context("provider URL is invalid")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        bail!("provider URL must be an HTTP(S) URL without credentials or a fragment");
    }
    Ok(url)
}

pub fn http_client() -> Result<Client> {
    Client::builder()
        .add_root_certificate(reqwest::Certificate::from_der(include_bytes!(
            "../certs/apple-root-ca.cer"
        ))?)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(45))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("creating HTTP client")
}

/// Bound untrusted responses even when the server omits Content-Length.
pub async fn read_limited(mut response: Response, limit: usize) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        bail!("response exceeds size limit");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            bail!("response exceeds size limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn headers(
    client: &Client,
    url: &str,
    uid: &str,
    device_id: &str,
    time: &str,
) -> Result<HeaderMap> {
    let response = client
        .get(validate_url(url)?)
        .send()
        .await
        .context("contacting anisette provider")?;
    if !response.status().is_success() {
        bail!(
            "anisette provider returned HTTP {}",
            response.status().as_u16()
        );
    }
    let data: Value = serde_json::from_slice(&read_limited(response, 64 * 1024).await?)
        .context("provider did not return JSON headers")?;
    build_headers(&data, uid, device_id, time)
}

fn build_headers(data: &Value, uid: &str, device_id: &str, time: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    for name in ["X-Apple-I-MD", "X-Apple-I-MD-M"] {
        let value = data
            .get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .with_context(|| format!("anisette provider is missing {name}"))?;
        insert(&mut headers, name, value)?;
    }
    let routing = match data.get("X-Apple-I-MD-RINFO") {
        Some(Value::String(v)) => v.clone(),
        Some(Value::Number(v)) => v.to_string(),
        _ => "17106176".into(),
    };
    for (name, value) in [
        ("X-Apple-I-MD-RINFO", routing),
        ("X-Apple-I-MD-LU", B64.encode(uid)),
        ("X-Mme-Device-Id", device_id.to_uppercase()),
        ("X-Apple-I-Client-Time", time.to_owned()),
        ("X-Apple-I-TimeZone", "UTC".into()),
        ("X-Apple-Locale", "en_US".into()),
        ("loc", "en_US".into()),
        ("X-Apple-I-SRL-NO", "0".into()),
    ] {
        insert(&mut headers, name, &value)?;
    }
    Ok(headers)
}

fn insert(headers: &mut HeaderMap, name: &str, value: &str) -> Result<()> {
    let mut value =
        HeaderValue::from_str(value).with_context(|| format!("invalid {name} header"))?;
    value.set_sensitive(true);
    headers.insert(HeaderName::from_bytes(name.as_bytes())?, value);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_provider_and_filters_headers() {
        for url in [
            "ftp://host",
            "http://user:password@host",
            "http://host/#fragment",
            "http://",
        ] {
            assert!(validate_url(url).is_err(), "{url}");
        }
        assert!(validate_url("http://anisette:6969/").is_ok());
        let data = json!({"X-Apple-I-MD":"otp", "X-Apple-I-MD-M":"machine", "X-Apple-I-MD-RINFO":17106176,
            "Authorization":"secret", "Host":"attacker"});
        let headers = build_headers(&data, "user", "device", "2026-10-05T00:00:00Z").unwrap();
        assert!(!headers.contains_key("authorization"));
        assert!(!headers.contains_key("host"));
        assert_eq!(headers["x-apple-i-md-lu"], "dXNlcg==");
        assert_eq!(headers["x-mme-device-id"], "DEVICE");
        assert!(headers["x-apple-i-md"].is_sensitive());
        assert!(build_headers(&json!({}), "u", "d", "t").is_err());
        assert!(build_headers(
            &json!({"X-Apple-I-MD":"bad\r\nheader", "X-Apple-I-MD-M":"m"}),
            "u",
            "d",
            "t"
        )
        .is_err());
    }
}
