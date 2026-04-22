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
    let dir = resolve_cache_dir()?;
    let cache_path = dir.join(ig_user_id);
    if let Ok(bytes) = read_cached(&cache_path) {
        return Ok(bytes);
    }

    let http = build_client(user_agent, cookies)?;
    let response = http.get(url).send().await.map_err(AppError::Network)?;
    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("avatar fetch returned HTTP {}", status.as_u16()),
        )));
    }
    let bytes = response.bytes().await.map_err(AppError::Network)?.to_vec();
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
