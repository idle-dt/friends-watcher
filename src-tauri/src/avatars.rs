use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, REFERER, USER_AGENT};
use reqwest::{Client, Url};

use crate::cookies::IG_WEBVIEW_USER_AGENT;
use crate::db::APP_DIR_NAME;
use crate::error::{AppError, Result};

const REFERER_URL: &str = "https://www.instagram.com/";
const ACCEPT_VALUE: &str = "image/*,*/*;q=0.8";

// Hard cap on avatar response body. Profile pics are well under 1 MB; 5 MB
// leaves generous headroom while preventing an oversized or adversarial
// response from ballooning memory.
pub const MAX_AVATAR_BYTES: usize = 5 * 1024 * 1024;

// Target size for cached avatars. The UI renders at 32×32 logical pixels
// (64×64 physical on Retina), so 64×64 is the largest useful preview size.
const AVATAR_THUMB_MAX: u32 = 64;
// JPEG quality for re-encoded thumbnails. 85 is the standard perceptual
// sweet spot — visually lossless for photos, ~10× smaller than the full CDN
// payload for typical profile pics.
const AVATAR_THUMB_JPEG_QUALITY: u8 = 85;

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
    if let Err(err) = std::fs::rename(&tmp_path, &final_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(err);
    }
    Ok(())
}

// Decode the network body, resize to AVATAR_THUMB_MAX on the longest edge,
// and re-encode as JPEG. Returns `None` on any decode/resize/encode error so
// the caller can fall through to the original bytes — this path must never
// make the app worse than today.
fn downscale_to_thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let decoded = reader.decode().ok()?;
    let thumb = decoded.thumbnail(AVATAR_THUMB_MAX, AVATAR_THUMB_MAX);
    let rgb = thumb.to_rgb8();
    let mut out = Vec::with_capacity(8 * 1024);
    let encoder = JpegEncoder::new_with_quality(&mut out, AVATAR_THUMB_JPEG_QUALITY);
    rgb.write_with_encoder(encoder).ok()?;
    Some(out)
}

// Shared HTTP client for avatar downloads. CDN URLs are signed, so we omit
// the cookie jar (session cookies would only cause needless revalidation
// churn). The UA is pinned to IG_WEBVIEW_USER_AGENT so CDN heuristics
// continue to accept the request.
pub struct AvatarHttp {
    client: Client,
}

impl AvatarHttp {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_VALUE));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_URL));
        headers.insert(USER_AGENT, HeaderValue::from_static(IG_WEBVIEW_USER_AGENT));

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(AppError::Network)?;
        Ok(Self { client })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

pub async fn fetch_avatar(
    client: &Client,
    ig_user_id: &str,
    url: &str,
) -> Result<Vec<u8>> {
    validate_ig_user_id(ig_user_id)?;
    let target = validate_avatar_url(url)?;

    let started = Instant::now();
    let dir = resolve_cache_dir()?;
    let cache_path = dir.join(ig_user_id);
    if let Ok(bytes) = read_cached(&cache_path) {
        log::debug!(
            target: "avatars",
            "fetch_avatar cache=hit bytes={} elapsed={}ms",
            bytes.len(),
            started.elapsed().as_millis()
        );
        return Ok(bytes);
    }

    let mut response = client.get(target).send().await.map_err(AppError::Network)?;
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
    let network_bytes = bytes.len();
    let payload = match downscale_to_thumbnail(&bytes) {
        Some(thumb) => thumb,
        None => {
            log::warn!(
                target: "avatars",
                "downscale failed for ig_user_id={} bytes={}; caching original",
                ig_user_id,
                network_bytes
            );
            bytes
        }
    };
    // Non-fatal: return the bytes even if the cache write fails.
    let _ = write_cached(&dir, ig_user_id, &payload);
    log::debug!(
        target: "avatars",
        "fetch_avatar cache=miss network_bytes={} stored_bytes={} elapsed={}ms",
        network_bytes,
        payload.len(),
        started.elapsed().as_millis()
    );
    Ok(payload)
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

    #[test]
    fn avatar_http_builds_with_pinned_ua() {
        let http = AvatarHttp::new().expect("client construction should succeed");
        // Smoke: the client is usable; can't hit the network in tests, but we
        // can confirm construction returned Ok and the handle is returnable.
        let _ = http.client();
    }

    #[test]
    fn thumbnail_roundtrip_smaller_than_source() {
        use image::{ExtendedColorType, ImageEncoder, RgbImage};

        // Build a 512x512 RGB image with a gradient so JPEG can't just
        // collapse it into a trivial solid block; the encoded size should
        // be comfortably larger than a 64x64 re-encode.
        let mut img = RgbImage::new(512, 512);
        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let r = (x & 0xFF) as u8;
            let g = (y & 0xFF) as u8;
            let b = ((x ^ y) & 0xFF) as u8;
            *pixel = image::Rgb([r, g, b]);
        }

        let mut source = Vec::new();
        let encoder = JpegEncoder::new_with_quality(&mut source, 90);
        encoder
            .write_image(img.as_raw(), 512, 512, ExtendedColorType::Rgb8)
            .expect("source JPEG should encode");

        let thumb = downscale_to_thumbnail(&source).expect("downscale should succeed on valid JPEG");
        assert!(
            thumb.len() < source.len(),
            "thumbnail bytes ({}) must be strictly smaller than source ({})",
            thumb.len(),
            source.len()
        );

        let decoded = ImageReader::new(Cursor::new(&thumb))
            .with_guessed_format()
            .expect("thumb format should be guessable")
            .decode()
            .expect("thumb should decode");
        let max_dim = decoded.width().max(decoded.height());
        assert!(
            max_dim <= AVATAR_THUMB_MAX,
            "max thumbnail dimension {} exceeds target {}",
            max_dim,
            AVATAR_THUMB_MAX
        );
    }

    #[test]
    fn downscale_returns_none_on_garbage_input() {
        // Non-fatal fallback path: malformed bytes must not panic and must
        // return None so fetch_avatar can cache the original payload.
        assert!(downscale_to_thumbnail(b"not an image at all").is_none());
    }

    #[test]
    fn write_cached_removes_tmp_when_rename_fails() {
        let dir = unique_tmp_dir("rename-fail");
        let ig_user_id = "77";
        // Pre-create the final path as a non-empty directory. On POSIX,
        // rename(tmp_file -> non_empty_dir) fails with ENOTEMPTY / EISDIR,
        // so write_cached must clean up the .tmp sidecar and surface the error.
        let final_path = dir.join(ig_user_id);
        std::fs::create_dir(&final_path).unwrap();
        std::fs::write(final_path.join("blocker"), b"x").unwrap();

        let result = write_cached(&dir, ig_user_id, b"payload");
        let tmp_path = dir.join(format!("{ig_user_id}.tmp"));

        if result.is_err() {
            assert!(
                !tmp_path.exists(),
                ".tmp sidecar must be removed after rename failure"
            );
        } else {
            // Some platforms may allow this rename; in that case the test is
            // inapplicable and we skip without failing.
            eprintln!("skipping rename-failure assertion: rename unexpectedly succeeded on this platform");
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
