use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

const IG_DOMAIN_SUFFIX: &str = "instagram.com";

// Must match the UA the IG WKWebView sends. We pin the value so the webview
// (configured via WebviewWindowBuilder::user_agent) and the reqwest client
// always agree — IG rejects follower/following calls whose UA differs from
// the browsing session that produced `sessionid`.
pub const IG_WEBVIEW_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Safari/605.1.15";

const REQUIRED_NAMES: [&str; 5] = ["sessionid", "csrftoken", "ds_user_id", "mid", "ig_did"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarvestedCookies {
    pub sessionid: String,
    pub csrftoken: String,
    pub ds_user_id: String,
    pub mid: Option<String>,
    pub ig_did: Option<String>,
}

impl HarvestedCookies {
    pub fn as_map(&self) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("sessionid".to_string(), self.sessionid.clone());
        m.insert("csrftoken".to_string(), self.csrftoken.clone());
        m.insert("ds_user_id".to_string(), self.ds_user_id.clone());
        if let Some(v) = &self.mid {
            m.insert("mid".to_string(), v.clone());
        }
        if let Some(v) = &self.ig_did {
            m.insert("ig_did".to_string(), v.clone());
        }
        m
    }
}

pub(crate) fn select(pairs: &[(String, String)]) -> Result<HarvestedCookies> {
    let mut picked: HashMap<&'static str, String> = HashMap::new();
    for (name, value) in pairs {
        if value.is_empty() {
            continue;
        }
        for required in REQUIRED_NAMES.iter() {
            if name == required {
                picked.insert(required, value.clone());
            }
        }
    }
    let sessionid = picked.remove("sessionid").ok_or(AppError::SessionExpired)?;
    let csrftoken = picked.remove("csrftoken").ok_or(AppError::SessionExpired)?;
    let ds_user_id = picked
        .remove("ds_user_id")
        .ok_or(AppError::SessionExpired)?;
    Ok(HarvestedCookies {
        sessionid,
        csrftoken,
        ds_user_id,
        mid: picked.remove("mid"),
        ig_did: picked.remove("ig_did"),
    })
}

/// Returns (name, value) pairs for cookies scoped to any `*.instagram.com`
/// domain in the webview's cookie jar.
///
/// We deliberately avoid `WebviewWindow::cookies_for_url` — in Tauri 2.10 on
/// macOS that API's URL matcher drops cookies whose domain attribute is
/// `instagram.com` (observed live: jar has 10+ IG cookies, filter returns 0).
/// Iterating the full jar and matching by domain suffix is reliable.
pub fn ig_cookie_pairs(window: &tauri::WebviewWindow) -> Result<Vec<(String, String)>> {
    let all = window.cookies().map_err(tauri_err)?;
    Ok(all
        .into_iter()
        .filter(|c| {
            c.domain()
                .map(|d| d.trim_start_matches('.').ends_with(IG_DOMAIN_SUFFIX))
                .unwrap_or(false)
        })
        .map(|c| (c.name().to_string(), c.value().to_string()))
        .collect())
}

pub fn harvest(window: &tauri::WebviewWindow) -> Result<HarvestedCookies> {
    let pairs = ig_cookie_pairs(window)?;
    select(&pairs)
}

// Tauri 2's `Webview::eval` does not return a JS value, so the IG webview's
// `navigator.userAgent` cannot be round-tripped cheaply from Rust. Instead
// we pin the UA the IG webview uses (`IG_WEBVIEW_USER_AGENT`, applied via
// WebviewWindowBuilder::user_agent when the `ig` webview is created) and
// return the same constant here so the reqwest client's UA is guaranteed to
// match what IG saw during the interactive login.
pub fn capture_user_agent(_window: &tauri::WebviewWindow) -> Result<String> {
    Ok(IG_WEBVIEW_USER_AGENT.to_string())
}

fn tauri_err(e: tauri::Error) -> AppError {
    AppError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("tauri: {e}"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(name: &str, value: &str) -> (String, String) {
        (name.to_string(), value.to_string())
    }

    fn full_set() -> Vec<(String, String)> {
        vec![
            pair("sessionid", "SID"),
            pair("csrftoken", "CSRF"),
            pair("ds_user_id", "42"),
            pair("mid", "MID"),
            pair("ig_did", "DID"),
        ]
    }

    #[test]
    fn select_returns_all_five_when_present() {
        let got = select(&full_set()).unwrap();
        assert_eq!(got.sessionid, "SID");
        assert_eq!(got.csrftoken, "CSRF");
        assert_eq!(got.ds_user_id, "42");
        assert_eq!(got.mid.as_deref(), Some("MID"));
        assert_eq!(got.ig_did.as_deref(), Some("DID"));
    }

    #[test]
    fn select_ignores_unrelated_cookies() {
        let mut pairs = full_set();
        pairs.push(pair("rur", "IAD"));
        pairs.push(pair("datr", "abc"));
        let got = select(&pairs).unwrap();
        assert_eq!(got.sessionid, "SID");
    }

    #[test]
    fn select_missing_sessionid_is_session_expired() {
        let pairs: Vec<_> = full_set()
            .into_iter()
            .filter(|(n, _)| n != "sessionid")
            .collect();
        let err = select(&pairs).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn select_missing_csrftoken_is_session_expired() {
        let pairs: Vec<_> = full_set()
            .into_iter()
            .filter(|(n, _)| n != "csrftoken")
            .collect();
        let err = select(&pairs).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn select_missing_ds_user_id_is_session_expired() {
        let pairs: Vec<_> = full_set()
            .into_iter()
            .filter(|(n, _)| n != "ds_user_id")
            .collect();
        let err = select(&pairs).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn select_empty_sessionid_is_session_expired() {
        let mut pairs = full_set();
        for p in pairs.iter_mut() {
            if p.0 == "sessionid" {
                p.1.clear();
            }
        }
        let err = select(&pairs).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn select_missing_optional_mid_and_ig_did() {
        let pairs = vec![
            pair("sessionid", "SID"),
            pair("csrftoken", "CSRF"),
            pair("ds_user_id", "42"),
        ];
        let got = select(&pairs).unwrap();
        assert!(got.mid.is_none());
        assert!(got.ig_did.is_none());
    }

    #[test]
    fn as_map_includes_optional_fields_only_when_set() {
        let with_opt = HarvestedCookies {
            sessionid: "s".into(),
            csrftoken: "c".into(),
            ds_user_id: "d".into(),
            mid: Some("m".into()),
            ig_did: Some("i".into()),
        };
        let m = with_opt.as_map();
        assert_eq!(m.len(), 5);
        assert_eq!(m.get("mid").map(String::as_str), Some("m"));

        let without_opt = HarvestedCookies {
            sessionid: "s".into(),
            csrftoken: "c".into(),
            ds_user_id: "d".into(),
            mid: None,
            ig_did: None,
        };
        let m = without_opt.as_map();
        assert_eq!(m.len(), 3);
        assert!(!m.contains_key("mid"));
        assert!(!m.contains_key("ig_did"));
    }

    #[test]
    fn harvested_cookies_map_is_accepted_by_ig_client() {
        use crate::instagram::IgClient;
        let harvested = HarvestedCookies {
            sessionid: "SID".into(),
            csrftoken: "CSRF".into(),
            ds_user_id: "42".into(),
            mid: Some("MID".into()),
            ig_did: Some("DID".into()),
        };
        let client = IgClient::new(IG_WEBVIEW_USER_AGENT.to_string(), harvested.as_map());
        assert!(client.is_ok());
    }

    #[test]
    fn ig_webview_user_agent_is_a_safari_ua() {
        assert!(IG_WEBVIEW_USER_AGENT.contains("Safari"));
        assert!(IG_WEBVIEW_USER_AGENT.contains("Mac OS X"));
    }
}
