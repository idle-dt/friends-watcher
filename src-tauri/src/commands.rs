use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;
use tauri::{AppHandle, State, Url, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::avatars;
use crate::cookies::{capture_user_agent, harvest, ig_cookie_pairs};
use crate::db;
use crate::error::AppError;
use crate::instagram::IgClient;
use crate::models::{DiffResult, Relationship, SessionState, SyncResult};

const IG_LOGIN_URL: &str = "https://www.instagram.com/accounts/login/";
// Cap polling at ~10 minutes (300 ticks × 2s). Plenty for a real login,
// short enough that an abandoned attempt doesn't leave a task running forever.
const LOGIN_POLL_TICKS: u32 = 300;
const LOGIN_POLL_INTERVAL: Duration = Duration::from_secs(2);

// Guards against concurrent login flows — React StrictMode double-mounts in
// dev, and an occasional no-op re-render could otherwise spawn a second
// polling task while the first is still running.
static LOGIN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

fn tauri_io_err(e: tauri::Error) -> AppError {
    AppError::Io(std::io::Error::new(
        std::io::ErrorKind::Other,
        format!("tauri: {e}"),
    ))
}

pub struct DbState(pub Mutex<Connection>);

impl DbState {
    pub fn new(conn: Connection) -> Self {
        Self(Mutex::new(conn))
    }
}

fn lock_db<'a>(state: &'a State<'_, DbState>) -> Result<std::sync::MutexGuard<'a, Connection>, AppError> {
    state.0.lock().map_err(|_| {
        AppError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            "db mutex poisoned",
        ))
    })
}

#[tauri::command]
pub async fn get_session_state(
    window: WebviewWindow,
    state: State<'_, DbState>,
) -> Result<SessionState, AppError> {
    let logged_in = harvest(&window).is_ok();
    let guard = lock_db(&state)?;
    let latest = db::get_latest_snapshot(&*guard)?;
    Ok(SessionState {
        logged_in,
        username: latest.as_ref().map(|s| s.owner_username.clone()),
        last_sync_at: latest.map(|s| s.taken_at),
    })
}

#[tauri::command]
pub async fn sync_now(
    window: WebviewWindow,
    state: State<'_, DbState>,
) -> Result<SyncResult, AppError> {
    let cookies = harvest(&window)?;
    let user_agent = capture_user_agent(&window)?;
    let ds_user_id = cookies.ds_user_id.clone();
    let client = IgClient::new(user_agent, cookies.as_map())?;

    let profile = client.resolve_profile_by_id(&ds_user_id).await?;
    let followers = client.fetch_followers(&profile.id).await?;
    let following = client.fetch_following(&profile.id).await?;
    let total_followers = followers.len() as i64;
    let total_following = following.len() as i64;

    let mut guard = lock_db(&state)?;
    let snapshot_id = db::write_snapshot(
        &mut *guard,
        &profile.id,
        &profile.username,
        &followers,
        &following,
    )?;
    let previous = db::get_previous_snapshot(&*guard)?;
    let (new_followers, lost_followers) = match previous {
        Some(prev) => {
            let diff = db::get_diff(&*guard, snapshot_id, prev.id)?;
            (diff.new_followers, diff.lost_followers)
        }
        None => (Vec::new(), Vec::new()),
    };

    Ok(SyncResult {
        new_followers,
        lost_followers,
        total_followers,
        total_following,
    })
}

#[tauri::command]
pub async fn get_latest_relationships(
    state: State<'_, DbState>,
) -> Result<Vec<Relationship>, AppError> {
    let guard = lock_db(&state)?;
    match db::get_latest_snapshot(&*guard)? {
        Some(snapshot) => Ok(db::get_relationships(&*guard, snapshot.id)?),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
pub async fn get_diff_since_previous(
    state: State<'_, DbState>,
) -> Result<DiffResult, AppError> {
    let guard = lock_db(&state)?;
    match (
        db::get_latest_snapshot(&*guard)?,
        db::get_previous_snapshot(&*guard)?,
    ) {
        (Some(cur), Some(prev)) => Ok(db::get_diff(&*guard, cur.id, prev.id)?),
        _ => Ok(DiffResult {
            since: None,
            new_followers: Vec::new(),
            lost_followers: Vec::new(),
        }),
    }
}

#[tauri::command]
pub async fn start_ig_login(window: WebviewWindow) -> Result<(), AppError> {
    // If a login flow is already running (double mount, loop retry), no-op.
    if LOGIN_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        log::info!("ig-login: already in progress; skipping duplicate call");
        return Ok(());
    }

    let return_url = window.url().map_err(tauri_io_err)?;
    let ig_url: Url = IG_LOGIN_URL
        .parse()
        .expect("static IG_LOGIN_URL parses as a Url");

    log::info!("ig-login: starting; return_url={}", return_url);

    // Spawn the polling task before navigating: if navigate() fails, the
    // task simply expires after LOGIN_POLL_TICKS without doing damage.
    let watch_window = window.clone();
    tokio::spawn(async move {
        log::info!("ig-login: polling task spawned");
        for tick in 1..=LOGIN_POLL_TICKS {
            tokio::time::sleep(LOGIN_POLL_INTERVAL).await;

            let current_url = match watch_window.url() {
                Ok(u) => u,
                Err(e) => {
                    log::warn!("ig-login: window.url() failed: {e}; stopping");
                    break;
                }
            };

            // If the user navigated away from instagram.com (closed the flow,
            // or we already returned), stop polling.
            if current_url
                .host_str()
                .map_or(true, |h| !h.contains("instagram.com"))
            {
                log::info!(
                    "ig-login: window is off instagram.com (url={}); stopping",
                    current_url
                );
                break;
            }

            let pairs = match ig_cookie_pairs(&watch_window) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!("ig-login: ig_cookie_pairs failed on tick {tick}: {e:?}");
                    continue;
                }
            };
            let logged_in = pairs
                .iter()
                .any(|(n, v)| n == "sessionid" && !v.is_empty());
            if logged_in {
                match watch_window.navigate(return_url.clone()) {
                    Ok(()) => log::info!(
                        "ig-login: sessionid detected on tick {tick}; navigated back to {}",
                        return_url
                    ),
                    Err(e) => log::error!("ig-login: navigate back failed: {e}"),
                }
                break;
            }
        }
        LOGIN_IN_PROGRESS.store(false, Ordering::Release);
        log::info!("ig-login: polling task exiting");
    });

    log::info!("ig-login: navigating main webview to {}", ig_url);
    window.navigate(ig_url).map_err(tauri_io_err)?;
    Ok(())
}

#[tauri::command]
pub async fn get_avatar(
    window: WebviewWindow,
    ig_user_id: String,
    url: String,
) -> Result<Vec<u8>, AppError> {
    // fetch_avatar revalidates, but checking at the command boundary keeps
    // malformed callers from ever hitting cookie harvest or the filesystem.
    avatars::validate_ig_user_id(&ig_user_id)?;
    avatars::validate_avatar_url(&url)?;
    let cookies = harvest(&window)?;
    let user_agent = capture_user_agent(&window)?;
    avatars::fetch_avatar(&user_agent, &cookies.as_map(), &ig_user_id, &url).await
}

#[tauri::command]
pub fn open_profile(app: AppHandle, username: String) -> Result<(), AppError> {
    let url = format!("https://instagram.com/{}", username);
    app.opener()
        .open_url(url, None::<String>)
        .map_err(|e| {
            AppError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("opener: {e}"),
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RelationshipStatus, UserRow};

    fn u(id: &str, name: &str) -> UserRow {
        UserRow {
            ig_user_id: id.to_string(),
            username: name.to_string(),
            full_name: None,
            is_verified: false,
            profile_pic_url: None,
        }
    }

    fn fresh_state() -> DbState {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        db::init_schema(&conn).unwrap();
        DbState::new(conn)
    }

    #[test]
    fn latest_relationships_empty_when_no_snapshot() {
        let state = fresh_state();
        let guard = state.0.lock().unwrap();
        let latest = db::get_latest_snapshot(&*guard).unwrap();
        assert!(latest.is_none());
    }

    #[test]
    fn sync_then_latest_relationships_returns_merged_rows() {
        let state = fresh_state();
        {
            let mut guard = state.0.lock().unwrap();
            let followers = vec![u("1", "alice"), u("2", "bob")];
            let following = vec![u("1", "alice"), u("3", "carol")];
            db::write_snapshot(&mut *guard, "42", "me", &followers, &following).unwrap();
        }
        let guard = state.0.lock().unwrap();
        let snapshot = db::get_latest_snapshot(&*guard).unwrap().unwrap();
        let rels = db::get_relationships(&*guard, snapshot.id).unwrap();
        let statuses: std::collections::HashMap<_, _> = rels
            .iter()
            .map(|r| (r.username.clone(), r.status))
            .collect();
        assert_eq!(statuses["alice"], RelationshipStatus::Mutual);
        assert_eq!(statuses["bob"], RelationshipStatus::Fan);
        assert_eq!(statuses["carol"], RelationshipStatus::Ghost);
    }

    #[test]
    fn diff_since_previous_empty_with_single_snapshot() {
        let state = fresh_state();
        {
            let mut guard = state.0.lock().unwrap();
            db::write_snapshot(&mut *guard, "42", "me", &[u("1", "alice")], &[]).unwrap();
        }
        let guard = state.0.lock().unwrap();
        let latest = db::get_latest_snapshot(&*guard).unwrap();
        let previous = db::get_previous_snapshot(&*guard).unwrap();
        assert!(latest.is_some());
        assert!(previous.is_none());
    }

    #[test]
    fn diff_since_previous_reports_changes_across_two_snapshots() {
        let state = fresh_state();
        {
            let mut guard = state.0.lock().unwrap();
            db::write_snapshot(
                &mut *guard,
                "42",
                "me",
                &[u("1", "alice"), u("2", "bob")],
                &[],
            )
            .unwrap();
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        {
            let mut guard = state.0.lock().unwrap();
            db::write_snapshot(
                &mut *guard,
                "42",
                "me",
                &[u("1", "alice"), u("3", "carol")],
                &[],
            )
            .unwrap();
        }
        let guard = state.0.lock().unwrap();
        let cur = db::get_latest_snapshot(&*guard).unwrap().unwrap();
        let prev = db::get_previous_snapshot(&*guard).unwrap().unwrap();
        let diff = db::get_diff(&*guard, cur.id, prev.id).unwrap();
        let new_ids: Vec<_> = diff.new_followers.iter().map(|u| u.ig_user_id.as_str()).collect();
        let lost_ids: Vec<_> = diff.lost_followers.iter().map(|u| u.ig_user_id.as_str()).collect();
        assert_eq!(new_ids, vec!["3"]);
        assert_eq!(lost_ids, vec!["2"]);
        assert!(diff.since.is_some());
    }
}
