use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::cookie::Jar;
use reqwest::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT, REFERER, USER_AGENT,
};
use reqwest::{Client, Url};
use serde_json::Value;

use crate::error::{AppError, Result};
use crate::models::{OwnProfile, UserRow};

const X_IG_APP_ID: &str = "936619743392459";
const X_ASBD_ID: &str = "198387";
const REFERER_URL: &str = "https://www.instagram.com/";
const ACCEPT_VALUE: &str = "*/*";
const BASE_URL: &str = "https://www.instagram.com";

const PAGE_SLEEP: Duration = Duration::from_millis(1_500);
const PAGE_SIZE: &str = "50";
pub const MAX_USERS: usize = 20_000;

const MAX_RETRIES: usize = 3;
const BACKOFFS_SECS: [u64; MAX_RETRIES] = [5, 15, 45];

#[derive(Debug)]
pub struct IgClient {
    http: Client,
    base_url: String,
}

impl IgClient {
    pub fn new(user_agent: String, cookies: HashMap<String, String>) -> Result<Self> {
        Self::with_base_url(user_agent, cookies, BASE_URL.to_string())
    }

    pub fn with_base_url(
        user_agent: String,
        cookies: HashMap<String, String>,
        base_url: String,
    ) -> Result<Self> {
        let csrftoken = cookies
            .get("csrftoken")
            .filter(|s| !s.is_empty())
            .cloned()
            .ok_or(AppError::SessionExpired)?;
        if cookies
            .get("sessionid")
            .map(|s| s.is_empty())
            .unwrap_or(true)
        {
            return Err(AppError::SessionExpired);
        }

        let jar = Arc::new(Jar::default());
        let seed_url: Url = BASE_URL.parse().map_err(|_| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid instagram base url",
            ))
        })?;
        for name in ["sessionid", "csrftoken", "ds_user_id", "mid", "ig_did"] {
            if let Some(value) = cookies.get(name) {
                let cookie_str = format!("{name}={value}; Domain=.instagram.com; Path=/");
                jar.add_cookie_str(&cookie_str, &seed_url);
            }
        }

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static(ACCEPT_VALUE));
        headers.insert(REFERER, HeaderValue::from_static(REFERER_URL));
        headers.insert(
            HeaderName::from_static("x-ig-app-id"),
            HeaderValue::from_static(X_IG_APP_ID),
        );
        headers.insert(
            HeaderName::from_static("x-asbd-id"),
            HeaderValue::from_static(X_ASBD_ID),
        );
        headers.insert(
            HeaderName::from_static("x-requested-with"),
            HeaderValue::from_static("XMLHttpRequest"),
        );
        headers.insert(
            HeaderName::from_static("x-csrftoken"),
            HeaderValue::from_str(&csrftoken).map_err(|_| {
                AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "csrftoken is not a valid header value",
                ))
            })?,
        );
        headers.insert(
            USER_AGENT,
            HeaderValue::from_str(&user_agent).map_err(|_| {
                AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "user agent is not a valid header value",
                ))
            })?,
        );

        let http = Client::builder()
            .cookie_provider(jar)
            .default_headers(headers)
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self { http, base_url })
    }

    async fn send(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{}", self.base_url, path);
        let mut attempt: usize = 0;
        loop {
            let response = self
                .http
                .get(&url)
                .query(query)
                .send()
                .await
                .map_err(AppError::Network)?;
            let status = response.status();
            let body = response.text().await.map_err(AppError::Network)?;

            if status.as_u16() == 401 || body.contains("\"login_required\"") {
                return Err(AppError::SessionExpired);
            }

            let rate_limited = status.as_u16() == 429
                || body.contains("\"feedback_required\"")
                || body.contains("\"checkpoint_required\"");

            if rate_limited {
                if attempt >= MAX_RETRIES {
                    return Err(AppError::RateLimited);
                }
                tokio::time::sleep(Duration::from_secs(BACKOFFS_SECS[attempt])).await;
                attempt += 1;
                continue;
            }

            if !status.is_success() {
                return Err(AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("instagram returned HTTP {}", status.as_u16()),
                )));
            }

            return serde_json::from_str::<Value>(&body).map_err(AppError::Decode);
        }
    }

    pub async fn resolve_profile_by_id(&self, user_id: &str) -> Result<OwnProfile> {
        let json = self
            .send(&format!("/api/v1/users/{}/info/", user_id), &[])
            .await?;
        parse_profile_by_id(&json, user_id)
    }

    pub async fn resolve_profile(&self, username: &str) -> Result<OwnProfile> {
        let json = self
            .send(
                "/api/v1/users/web_profile_info/",
                &[("username", username)],
            )
            .await?;
        let user = json
            .get("data")
            .and_then(|d| d.get("user"))
            .ok_or_else(|| {
                AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "profile response missing data.user",
                ))
            })?;

        let id = extract_id(user.get("id"))
            .or_else(|| extract_id(user.get("pk")))
            .ok_or_else(|| {
                AppError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "profile response missing user id",
                ))
            })?;
        let username = user
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or(username)
            .to_string();
        let full_name = user
            .get("full_name")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let followers_count = user
            .get("edge_followed_by")
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let following_count = user
            .get("edge_follow")
            .and_then(|v| v.get("count"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        Ok(OwnProfile {
            id,
            username,
            full_name,
            followers_count,
            following_count,
        })
    }

    pub async fn fetch_followers(&self, user_id: &str) -> Result<Vec<UserRow>> {
        self.fetch_paginated(&format!("/api/v1/friendships/{}/followers/", user_id))
            .await
    }

    pub async fn fetch_following(&self, user_id: &str) -> Result<Vec<UserRow>> {
        self.fetch_paginated(&format!("/api/v1/friendships/{}/following/", user_id))
            .await
    }

    async fn fetch_paginated(&self, path: &str) -> Result<Vec<UserRow>> {
        let mut out: Vec<UserRow> = Vec::new();
        let mut cursor: Option<String> = None;
        let mut first = true;
        let mut page_number: u32 = 0;

        loop {
            if out.len() >= MAX_USERS {
                break;
            }
            if !first {
                tokio::time::sleep(PAGE_SLEEP).await;
            }
            first = false;
            page_number += 1;

            let cursor_owned = cursor.clone().unwrap_or_default();
            let mut params: Vec<(&str, &str)> = vec![("count", PAGE_SIZE)];
            if cursor.is_some() {
                params.push(("max_id", cursor_owned.as_str()));
            }

            let page_start = Instant::now();
            let json = self.send(path, &params).await?;

            let mut users_in_page: usize = 0;
            if let Some(users) = json.get("users").and_then(|v| v.as_array()) {
                users_in_page = users.len();
                for user in users {
                    if let Some(row) = parse_user(user) {
                        out.push(row);
                        if out.len() >= MAX_USERS {
                            break;
                        }
                    }
                }
            }
            log::debug!(
                target: "sync",
                "fetch_paginated {} page {} users={} in {}ms",
                path,
                page_number,
                users_in_page,
                page_start.elapsed().as_millis()
            );

            cursor = json
                .get("next_max_id")
                .and_then(|v| {
                    if v.is_null() {
                        None
                    } else if let Some(s) = v.as_str() {
                        if s.is_empty() {
                            None
                        } else {
                            Some(s.to_string())
                        }
                    } else if let Some(n) = v.as_i64() {
                        Some(n.to_string())
                    } else {
                        None
                    }
                });

            if cursor.is_none() {
                break;
            }
        }

        Ok(out)
    }
}

fn extract_id(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if let Some(s) = v.as_str() {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    if let Some(n) = v.as_u64() {
        return Some(n.to_string());
    }
    None
}

fn parse_profile_by_id(json: &Value, fallback_id: &str) -> Result<OwnProfile> {
    let user = json.get("user").ok_or_else(|| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "by-id profile response missing user",
        ))
    })?;
    let id = extract_id(user.get("pk"))
        .or_else(|| extract_id(user.get("id")))
        .unwrap_or_else(|| fallback_id.to_string());
    let username = user
        .get("username")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "by-id profile response missing username",
            ))
        })?;
    let full_name = user
        .get("full_name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let followers_count = user
        .get("follower_count")
        .or_else(|| user.get("edge_followed_by").and_then(|v| v.get("count")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let following_count = user
        .get("following_count")
        .or_else(|| user.get("edge_follow").and_then(|v| v.get("count")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    Ok(OwnProfile {
        id,
        username,
        full_name,
        followers_count,
        following_count,
    })
}

fn parse_user(v: &Value) -> Option<UserRow> {
    let ig_user_id = extract_id(v.get("pk")).or_else(|| extract_id(v.get("id")))?;
    let username = v.get("username").and_then(|u| u.as_str())?.to_string();
    let full_name = v
        .get("full_name")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let is_verified = v.get("is_verified").and_then(|u| u.as_bool()).unwrap_or(false);
    let profile_pic_url = v
        .get("profile_pic_url")
        .and_then(|u| u.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    Some(UserRow {
        ig_user_id,
        username,
        full_name,
        is_verified,
        profile_pic_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn good_cookies() -> HashMap<String, String> {
        let mut c = HashMap::new();
        c.insert("csrftoken".into(), "tok".into());
        c.insert("sessionid".into(), "sess".into());
        c.insert("ds_user_id".into(), "123".into());
        c.insert("mid".into(), "m".into());
        c.insert("ig_did".into(), "d".into());
        c
    }

    #[test]
    fn parse_user_handles_string_pk() {
        let v = json!({
            "pk": "12345",
            "username": "alice",
            "full_name": "Alice A.",
            "is_verified": false,
            "is_private": true,
            "profile_pic_url": "https://example.test/alice.jpg"
        });
        let row = parse_user(&v).unwrap();
        assert_eq!(row.ig_user_id, "12345");
        assert_eq!(row.username, "alice");
        assert_eq!(row.full_name.as_deref(), Some("Alice A."));
        assert_eq!(
            row.profile_pic_url.as_deref(),
            Some("https://example.test/alice.jpg")
        );
        assert!(!row.is_verified);
    }

    #[test]
    fn parse_user_handles_integer_pk() {
        let v = json!({
            "pk": 98765,
            "username": "bob",
        });
        let row = parse_user(&v).unwrap();
        assert_eq!(row.ig_user_id, "98765");
        assert_eq!(row.username, "bob");
        assert!(row.full_name.is_none());
        assert!(row.profile_pic_url.is_none());
    }

    #[test]
    fn parse_user_skips_missing_username() {
        let v = json!({ "pk": "1" });
        assert!(parse_user(&v).is_none());
    }

    #[test]
    fn parse_user_skips_missing_pk() {
        let v = json!({ "username": "x" });
        assert!(parse_user(&v).is_none());
    }

    #[test]
    fn parse_user_treats_empty_full_name_as_none() {
        let v = json!({ "pk": "1", "username": "x", "full_name": "" });
        let row = parse_user(&v).unwrap();
        assert!(row.full_name.is_none());
    }

    #[test]
    fn ig_client_rejects_missing_sessionid() {
        let mut cookies = HashMap::new();
        cookies.insert("csrftoken".to_string(), "tok".to_string());
        let err = IgClient::new("Mozilla/5.0".to_string(), cookies).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn ig_client_rejects_empty_sessionid() {
        let mut cookies = good_cookies();
        cookies.insert("sessionid".to_string(), String::new());
        let err = IgClient::new("Mozilla/5.0".to_string(), cookies).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn ig_client_rejects_missing_csrftoken() {
        let mut cookies = HashMap::new();
        cookies.insert("sessionid".to_string(), "sess".to_string());
        let err = IgClient::new("Mozilla/5.0".to_string(), cookies).unwrap_err();
        assert!(matches!(err, AppError::SessionExpired));
    }

    #[test]
    fn ig_client_builds_with_valid_cookies() {
        let client = IgClient::new("Mozilla/5.0".to_string(), good_cookies());
        assert!(client.is_ok());
    }

    #[test]
    fn parse_profile_by_id_extracts_username_and_counts() {
        let v = json!({
            "user": {
                "pk": "42",
                "username": "me",
                "full_name": "Me Myself",
                "follower_count": 123,
                "following_count": 45,
            }
        });
        let p = parse_profile_by_id(&v, "42").unwrap();
        assert_eq!(p.id, "42");
        assert_eq!(p.username, "me");
        assert_eq!(p.full_name.as_deref(), Some("Me Myself"));
        assert_eq!(p.followers_count, 123);
        assert_eq!(p.following_count, 45);
    }

    #[test]
    fn parse_profile_by_id_falls_back_to_edge_counts() {
        let v = json!({
            "user": {
                "pk": 42,
                "username": "me",
                "edge_followed_by": { "count": 9 },
                "edge_follow": { "count": 3 },
            }
        });
        let p = parse_profile_by_id(&v, "42").unwrap();
        assert_eq!(p.followers_count, 9);
        assert_eq!(p.following_count, 3);
        assert!(p.full_name.is_none());
    }

    #[test]
    fn parse_profile_by_id_uses_fallback_id_when_pk_missing() {
        let v = json!({ "user": { "username": "me" } });
        let p = parse_profile_by_id(&v, "99").unwrap();
        assert_eq!(p.id, "99");
    }

    #[test]
    fn parse_profile_by_id_errors_when_username_missing() {
        let v = json!({ "user": { "pk": "42" } });
        let err = parse_profile_by_id(&v, "42").unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
    }

    #[test]
    fn parse_profile_by_id_errors_when_user_missing() {
        let v = json!({ "something": {} });
        let err = parse_profile_by_id(&v, "42").unwrap_err();
        assert!(matches!(err, AppError::Io(_)));
    }

    #[test]
    fn extract_id_prefers_string_then_numeric() {
        assert_eq!(extract_id(Some(&json!("abc"))), Some("abc".to_string()));
        assert_eq!(extract_id(Some(&json!(42))), Some("42".to_string()));
        assert_eq!(extract_id(Some(&json!(null))), None);
        assert_eq!(extract_id(Some(&json!(""))), None);
        assert_eq!(extract_id(None), None);
    }
}
