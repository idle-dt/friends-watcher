use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, REFERER, USER_AGENT};
use reqwest::{Client, Url};

use crate::db::APP_DIR_NAME;
use crate::error::{AppError, Result};

const REFERER_URL: &str = "https://www.instagram.com/";
const ACCEPT_VALUE: &str = "image/*,*/*;q=0.8";
const COOKIE_SEED_URL: &str = "https://www.instagram.com";

// Hard cap on avatar response body. Profile pics are well under 1 MB; 5 MB
// leaves generous headroom while preventing an oversized or adversarial
// response from ballooning memory.
pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;

// Instagram user IDs are numeric strings. Validating keeps the value safe to
// use directly as a cache filename (no path traversal, no odd characters)
// and rejects malformed callers early.
pub fn validate_ig_user_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 64 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "ig_user_id must be a non-empty numeric string",
        )));
    }
    Ok(())
}

// Restrict avatar fetches to Instagram's image CDNs. Avoids turning this
// command into a generic outbound HTTP proxy (SSRF) if a compromised
// renderer passes an arbitrary URL.
pub fn validate_avatar_url(raw: &str) -> Result<Url> {
    let parsed: Url = raw.parse().map_err(|_| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "avatar url is not a valid URL",
        ))
    })?;
    if parsed.scheme() != "https" {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "avatar url must use https",
        )));
    }
    let host = parsed.host_str().unwrap_or("");
    let host_allowed = host == "cdninstagram.com"
        || host == "fbcdn.net"
        || host.ends_with(".cdninstagram.com")
        || host.ends_with(".fbcdn.net");
    if !host_allowed {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "avatar url host is not an Instagram CDN",
        )));
    }
    Ok(parsed)
}

fn resolve_cache_dir() -> Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve platform data directory",
        ))
    })?;
    let dir = base.join(APP_DIR_NAME).join("avatars");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// Factored out so tests can exercise the cache-hit path against a temp dir
// without adding a tempfile dev-dependency.
fn read_cached(path: &Path) -> std::io::Result<Vec<u8>> {
    std::fs::read(path)
}

fn write_cached(dir: &Path, ig_user_id: &str, bytes: &[u8]) -> std::io::Result<()> {
    let final_path = dir.join(ig_user_id);
    let tmp_path = dir.join(format!("{ig_user_id}.tmp"));
    std::fs::write(&tmp_path, bytes)?;
    std::fs::rename(&tmp_path, &final_path)?;
    Ok(())
}

fn build_client(user_agent: &str, cookies: &HashMap<String, String>) -> Result<Client> {
    let jar = Arc::new(Jar::default());
    let seed_url: Url = COOKIE_SEED_URL.parse().map_err(|_| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid instagram base url",
        ))
    })?;
    for name in ["sessionid", "csrftoken", "ds_user_id", "mid", "ig_did"] {
        if let Some(value) = cookies.get(name) {
            if !value.is_empty() {
                let cookie_str = format!("{name}={value}; Domain=.instagram.com; Path=/");
                jar.add_cookie_str(&cookie_str, &seed_url);
            }
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_VALUE));
    headers.insert(REFERER, HeaderValue::from_static(REFERER_URL));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(user_agent).map_err(|_| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "user agent is not a valid header value",
            ))
        })?,
    );

    Client::builder()
        .cookie_provider(jar)
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(AppError::Network)
}

pub async fn fetch_avatar(
    user_agent: &str,
    cookies: &HashMap<String, String>,
    ig_user_id: &str,
    url: &str,
) -> Result<Vec<u8>> {
    validate_ig_user_id(ig_user_id)?;
    let target = validate_avatar_url(url)?;

    let dir = resolve_cache_dir()?;
    let cache_path = dir.join(ig_user_id);
    if let Ok(bytes) = read_cached(&cache_path) {
        return Ok(bytes);
    }

    let http = build_client(user_agent, cookies)?;
    let mut response = http.get(target).send().await.map_err(AppError::Network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("avatar fetch returned HTTP {}", status.as_u16()),
        )));
    }
    // Early reject an oversized body via Content-Length when present.
    if let Some(declared) = response.content_length() {
        if declared > MAX_AVATAR_BYTES as u64 {
            return Err(AppError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("avatar response exceeds {MAX_AVATAR_BYTES}-byte cap"),
            )));
        }
    }
    // Stream chunks so a missing or lying Content-Length can't bypass the cap.
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(AppError::Network)? {
        if bytes.len() + chunk.len() > MAX_AVATAR_BYTES {
            return Err(AppError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("avatar response exceeds {MAX_AVATAR_BYTES}-byte cap"),
            )));
        }
        bytes.extend_from_slice(&chunk);
    }
    // Non-fatal: return the bytes even if the cache write fails.
    let _ = write_cached(&dir, ig_user_id, &bytes);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "friends-watcher-avatars-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn read_cached_returns_bytes_for_existing_file() {
        let dir = unique_tmp_dir("read-hit");
        let ig_user_id = "42";
        let payload = b"\xFF\xD8\xFFfake-jpeg-bytes";
        write_cached(&dir, ig_user_id, payload).unwrap();

        let got = read_cached(&dir.join(ig_user_id)).unwrap();
        assert_eq!(got, payload);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_cached_errors_when_file_absent() {
        let dir = unique_tmp_dir("read-miss");
        let err = read_cached(&dir.join("nope")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn validate_ig_user_id_accepts_numeric_strings() {
        assert!(validate_ig_user_id("42").is_ok());
        assert!(validate_ig_user_id("1234567890").is_ok());
    }

    #[test]
    fn validate_ig_user_id_rejects_bad_inputs() {
        assert!(validate_ig_user_id("").is_err());
        assert!(validate_ig_user_id("../../etc/passwd").is_err());
        assert!(validate_ig_user_id("42/43").is_err());
        assert!(validate_ig_user_id("42.tmp").is_err());
        assert!(validate_ig_user_id("abc").is_err());
        // Oversize input is rejected too.
        let long = "1".repeat(65);
        assert!(validate_ig_user_id(&long).is_err());
    }

    #[test]
    fn validate_avatar_url_accepts_ig_cdn_https() {
        assert!(validate_avatar_url(
            "https://scontent-sjc3-1.cdninstagram.com/v/t51.0-19/abc.jpg"
        )
        .is_ok());
        assert!(validate_avatar_url("https://scontent.fbcdn.net/v/t51/x.jpg").is_ok());
    }

    #[test]
    fn validate_avatar_url_rejects_non_ig_hosts_and_non_https() {
        assert!(validate_avatar_url("http://scontent.cdninstagram.com/x").is_err());
        assert!(validate_avatar_url("https://evil.example.com/x").is_err());
        assert!(validate_avatar_url("https://cdninstagram.com.evil.example/x").is_err());
        assert!(validate_avatar_url("file:///etc/passwd").is_err());
        assert!(validate_avatar_url("not a url").is_err());
        assert!(validate_avatar_url("").is_err());
    }

    #[test]
    fn write_cached_overwrites_existing_entry_atomically() {
        let dir = unique_tmp_dir("atomic-replace");
        let ig_user_id = "99";
        write_cached(&dir, ig_user_id, b"first").unwrap();
        write_cached(&dir, ig_user_id, b"second").unwrap();
        let got = read_cached(&dir.join(ig_user_id)).unwrap();
        assert_eq!(got, b"second");
        // The tmp file should be gone after rename.
        assert!(!dir.join(format!("{ig_user_id}.tmp")).exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
