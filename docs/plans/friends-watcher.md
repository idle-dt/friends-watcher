# Plan: Friends Watcher — Instagram follower tracker (macOS, Tauri 2)

## Context

A single-user macOS desktop app that tracks one Instagram account's followers and
surfaces who unfollowed since the last sync. Not distributed via the App Store.
Authoritative spec is preserved in `~/.claude/plans/playful-plotting-diffie.md`;
the critical constraints are restated here so each ralphex subagent has them.

### Stack (non-negotiable)

- Tauri 2.x (Rust shell + WKWebView on macOS)
- Rust: `reqwest` (with `cookies` feature), `rusqlite` (with `bundled` feature),
  `serde` / `serde_json`, `tokio`, `tauri-plugin-opener`
- Frontend: Vite + TypeScript + React
- SQLite file path: `~/Library/Application Support/com.friendswatcher.app/data.db`
- Bundle identifier: `com.friendswatcher.app`

### File layout (target)

```
friends-watcher/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── build.rs
│   ├── icons/
│   └── src/
│       ├── main.rs
│       ├── commands.rs
│       ├── instagram.rs
│       ├── cookies.rs
│       ├── db.rs
│       ├── models.rs
│       └── error.rs
├── src/
│   ├── index.html
│   ├── main.tsx
│   ├── App.tsx
│   ├── views/
│   │   ├── LoginView.tsx
│   │   └── MainView.tsx
│   ├── components/
│   │   ├── RelationshipRow.tsx
│   │   ├── DiffBanner.tsx
│   │   ├── StatusEmpty.tsx
│   │   ├── SessionExpiredBanner.tsx
│   │   └── RateLimitedBanner.tsx
│   └── lib/
│       └── tauri.ts
├── package.json
├── vite.config.ts
└── tsconfig.json
```

### Instagram API

All calls go to `https://www.instagram.com/api/v1/` with cookies harvested from
the logged-in WKWebView.

**Endpoints:**

- `GET /users/web_profile_info/?username=<me>` — resolve own user ID from
  `ds_user_id` cookie.
- `GET /friendships/<user_id>/followers/?count=50&max_id=<cursor>`
- `GET /friendships/<user_id>/following/?count=50&max_id=<cursor>`

Pagination cursor is `next_max_id` in the response; null/absent means done.

**Required headers on every call:**

- `X-IG-App-ID: 936619743392459`
- `X-CSRFToken: <csrftoken cookie value>`
- `X-Requested-With: XMLHttpRequest`
- `X-ASBD-ID: 198387`
- `Referer: https://www.instagram.com/`
- `Accept: */*`
- `User-Agent:` must match the WKWebView UA **exactly** — capture at runtime.

**Cookies forwarded verbatim in `Cookie` header:** `sessionid`, `csrftoken`,
`ds_user_id`, `mid`, `ig_did`.

**Rate-limit rules:**

- 1.5 s sleep between pages (sequential, not parallel).
- On HTTP 429 or response body containing `"feedback_required"` /
  `"checkpoint_required"` → exponential backoff: 5 s, 15 s, 45 s. After 3
  failures, stop and surface a rate-limited banner. Return partial results if
  useful.
- Hard cap: 20,000 users per sync (defensive).

**Session-expired detection:** HTTP 401 or response body contains
`"login_required"` → return `AppError::SessionExpired`; UI flips back to
LoginView and reloads the webview to `instagram.com/accounts/login`.

### SQLite schema

```sql
CREATE TABLE snapshots (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  taken_at        TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  owner_user_id   TEXT NOT NULL,
  owner_username  TEXT NOT NULL,
  followers_count INTEGER NOT NULL,
  following_count INTEGER NOT NULL
);

CREATE TABLE followers (
  snapshot_id     INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  ig_user_id      TEXT NOT NULL,
  username        TEXT NOT NULL,
  full_name       TEXT,
  is_verified     INTEGER NOT NULL DEFAULT 0,
  profile_pic_url TEXT,
  PRIMARY KEY (snapshot_id, ig_user_id)
);

CREATE TABLE following (
  snapshot_id     INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
  ig_user_id      TEXT NOT NULL,
  username        TEXT NOT NULL,
  full_name       TEXT,
  is_verified     INTEGER NOT NULL DEFAULT 0,
  profile_pic_url TEXT,
  PRIMARY KEY (snapshot_id, ig_user_id)
);

CREATE INDEX idx_followers_snapshot  ON followers(snapshot_id);
CREATE INDEX idx_following_snapshot  ON following(snapshot_id);
CREATE INDEX idx_snapshots_taken_at  ON snapshots(taken_at DESC);
```

Diff is computed on demand by joining the two most recent snapshots.

### Tauri commands (Rust ↔ JS contract)

```rust
#[tauri::command] async fn get_session_state() -> SessionState;
#[tauri::command] async fn sync_now() -> Result<SyncResult, AppError>;
#[tauri::command] async fn get_latest_relationships() -> Vec<Relationship>;
#[tauri::command] async fn get_diff_since_previous() -> DiffResult;
#[tauri::command] fn open_profile(username: String);
```

`Relationship.status` is one of `"mutual" | "fan" | "ghost" | "new" | "lost"`.
`SyncResult` carries `new_followers`, `lost_followers`, `total_followers`,
`total_following`.

### Hard constraints (do not violate)

- v1 is strictly **read-only**: no unfollow actions, no auto-sync, no background
  jobs, no notifications. Sync is only ever user-initiated.
- **No hardcoded Safari.** All external links open via `tauri-plugin-opener`.
- **No credentials logged or persisted** outside the webview cookie jar and
  SQLite snapshots.
- If an IG endpoint/header behaves differently than documented above, **stop
  and surface the error** — do not guess.

### Out-of-band steps (do NOT let ralphex run these)

These must be done manually before/after ralphex, not inside a task:

- `git init` and the initial commit (handled before invoking ralphex)
- `gh repo create friends-watcher --private --source=. --remote=origin`
- `rustup target add aarch64-apple-darwin x86_64-apple-darwin` (for universal
  builds — only needed when task 9 runs)
- Final release build and manual QA against a real IG test account

## Validation Commands

- `cargo check --manifest-path src-tauri/Cargo.toml`

> Note: `npm run build` is deliberately excluded from global validation because
> partial UI commits may leave it temporarily red between tasks. Add it to the
> local dev loop during tasks 6–7.

---

### Task 1: Scaffold Tauri 2 project + baseline config

- [x] Initialize the frontend: `npm create vite@latest . -- --template react-ts` (use the working dir — answer prompts to overwrite nothing important)
- [x] Install Tauri CLI + API: `npm install --save-dev @tauri-apps/cli@^2` and `npm install @tauri-apps/api@^2`
- [x] Run `npx tauri init` with identifier `com.friendswatcher.app`, window title `Friends Watcher`, dev URL `http://localhost:5173`, dist dir `../dist`, frontend dev command `npm run dev`, frontend build command `npm run build`
- [x] Edit `src-tauri/tauri.conf.json`: set window min size to 900×600, productName `Friends Watcher`
- [x] Add Rust deps to `src-tauri/Cargo.toml`: `reqwest = { version = "0.12", features = ["cookies", "json", "rustls-tls"], default-features = false }`, `rusqlite = { version = "0.31", features = ["bundled"] }`, `serde = { version = "1", features = ["derive"] }`, `serde_json = "1"`, `tokio = { version = "1", features = ["full"] }`, `tauri-plugin-opener = "2"`, `thiserror = "1"`, `chrono = { version = "0.4", features = ["serde"] }`, `dirs = "5"`
- [x] Create stub module files with empty `pub` scaffolding: `src-tauri/src/{commands.rs,instagram.rs,cookies.rs,db.rs,models.rs,error.rs}`
- [x] Declare modules in `src-tauri/src/main.rs` (or `lib.rs` per Tauri 2 layout) and register the `tauri-plugin-opener` plugin in the builder
- [x] Confirm `cargo check --manifest-path src-tauri/Cargo.toml` is green

### Task 2: SQLite schema, snapshot writes, diff queries

- [x] Implement `db::open_db()` that resolves `~/Library/Application Support/com.friendswatcher.app/data.db` via `dirs::data_dir()`, creates the parent directory if missing, and returns a `rusqlite::Connection`
- [x] Implement `db::init_schema(&Connection)` that creates `snapshots`, `followers`, `following` tables and the three indexes exactly as specified in the Context
- [x] Call `init_schema` once on app startup; use `CREATE TABLE IF NOT EXISTS` so reruns are safe
- [x] Define `models::UserRow`, `models::Snapshot`, `models::Relationship` (with `status: &'static str` or a `RelationshipStatus` enum), `models::DiffResult`, `models::SyncResult`, `models::SessionState` — all with `serde::{Serialize, Deserialize}` where they cross the Tauri boundary
- [x] Implement `db::write_snapshot(&Connection, owner_user_id, owner_username, followers: &[UserRow], following: &[UserRow]) -> Result<i64>` in a single transaction; returns the new snapshot id
- [x] Implement `db::get_latest_snapshot(&Connection) -> Result<Option<Snapshot>>` ordered by `taken_at DESC`
- [x] Implement `db::get_previous_snapshot(&Connection) -> Result<Option<Snapshot>>` (second-most-recent)
- [x] Implement `db::get_diff(&Connection, current_id, previous_id) -> Result<DiffResult>` returning `new_followers` (in current, not in previous) and `lost_followers` (in previous, not in current)
- [x] Implement `db::get_relationships(&Connection, snapshot_id) -> Result<Vec<Relationship>>` joining `followers` and `following` at the given snapshot to compute `mutual | fan | ghost` per user
- [x] Confirm `cargo check` is green

### Task 3: Instagram API client — pagination, headers, rate limiting

- [x] Implement `error::AppError` as a `thiserror` enum with variants `SessionExpired`, `RateLimited`, `Network(reqwest::Error)`, `Decode(serde_json::Error)`, `Db(rusqlite::Error)`, `Io(std::io::Error)`; derive `serde::Serialize` via a manual impl so it crosses the Tauri boundary cleanly
- [x] Define header constants in `instagram.rs`: `X_IG_APP_ID = "936619743392459"`, `X_ASBD_ID = "198387"`, plus static strings for `Referer` and `Accept`
- [x] Implement `instagram::IgClient` holding a configured `reqwest::Client` with a cookie jar, the runtime-captured User-Agent, and the csrftoken value
- [x] Implement `IgClient::new(user_agent: String, cookies: HashMap<String, String>) -> Result<Self>` that seeds the cookie jar for `https://www.instagram.com/` with all required cookies
- [x] Implement private helper `send(url) -> Result<serde_json::Value>` that attaches every required header (including `X-CSRFToken` from the provided cookies) and maps HTTP 401 / body-contains-`"login_required"` → `SessionExpired`, HTTP 429 / body-contains-`"feedback_required"` or `"checkpoint_required"` → retry with backoff 5s, 15s, 45s (max 3 attempts) then `RateLimited`
- [x] Implement `IgClient::resolve_profile(username: &str) -> Result<OwnProfile>` hitting `/users/web_profile_info/`; extract `data.user.{id, username, full_name, edge_followed_by.count, edge_follow.count}`
- [x] Implement `IgClient::fetch_followers(user_id: &str) -> Result<Vec<UserRow>>` looping over `/friendships/<id>/followers/?count=50&max_id=<cursor>`, following `next_max_id`, sleeping 1.5 s between pages, stopping at 20,000 users
- [x] Implement `IgClient::fetch_following(user_id: &str) -> Result<Vec<UserRow>>` with the same shape
- [x] Parse `users[].{pk, username, full_name, is_verified, is_private, profile_pic_url}` into `UserRow`
- [x] Confirm `cargo check` is green

### Task 4: Harvest session cookies from the webview

- [x] Implement `cookies::harvest(window: &tauri::WebviewWindow) -> Result<HarvestedCookies>` that returns `sessionid`, `csrftoken`, `ds_user_id`, `mid`, `ig_did` by calling the Tauri 2 cookie API (`window.cookies()` / `window.cookies_for_url()`)
- [x] Return `AppError::SessionExpired` if `sessionid` is missing
- [x] Implement `cookies::capture_user_agent(window: &tauri::WebviewWindow) -> Result<String>` that reads `navigator.userAgent` from the webview (via `eval`-with-return or webview settings) so the IG client sends an exact UA match
- [x] Add an integration point: the `sync_now` command (implemented in Task 5) should call `harvest` + `capture_user_agent` and feed them into `IgClient::new` (deferred to Task 5 — `HarvestedCookies::as_map()` wired as the bridge)
- [x] Confirm `cargo check` is green

### Task 5: Tauri command handlers

- [x] Implement `commands::get_session_state(window) -> SessionState` that returns `{ logged_in: bool, username: Option<String>, last_sync_at: Option<chrono::DateTime<Utc>> }` — `logged_in` is true iff `sessionid` is present, `username` comes from the most recent snapshot, `last_sync_at` from `snapshots.taken_at`
- [x] Implement `commands::sync_now(window) -> Result<SyncResult, AppError>`: harvest cookies → capture UA → `IgClient::new` → resolve own profile (using `ds_user_id` cookie to look up the username) → fetch followers → fetch following → `write_snapshot` → compute diff against previous snapshot → return `SyncResult`
- [x] Implement `commands::get_latest_relationships() -> Result<Vec<Relationship>, AppError>` delegating to `db::get_relationships` at the latest snapshot id
- [x] Implement `commands::get_diff_since_previous() -> Result<DiffResult, AppError>` using the latest two snapshots; if fewer than two snapshots exist, return an empty diff with `since = null`
- [x] Implement `commands::open_profile(app: AppHandle, username: String)` calling `tauri_plugin_opener::OpenerExt::opener(&app).open_url(format!("https://instagram.com/{username}"), None::<String>)`
- [x] Register all commands in the Tauri `invoke_handler!` macro in `main.rs`
- [x] Confirm `cargo check` is green

### Task 6: Login view + main view with diff banner and table

- [x] Create `src/lib/tauri.ts` with typed wrappers around `invoke` for each command — mirror the Rust types as TS interfaces (`SessionState`, `SyncResult`, `Relationship`, `DiffResult`)
- [x] Implement `src/views/LoginView.tsx`: render an embedded Tauri webview labeled `ig` pointed at `https://www.instagram.com/accounts/login/`; poll `get_session_state` every 2 s and call `onLogin()` when `logged_in` flips true
- [x] Implement `src/views/MainView.tsx`: a Sync button, a loading state with a "Checking followers — X of Y" progress hint, and a table of relationships
- [x] Implement `src/components/DiffBanner.tsx` that renders `new_followers` and `lost_followers` counts plus `since` date; hidden when both are zero
- [x] Implement `src/components/RelationshipRow.tsx`: avatar (`profile_pic_url`), username (links via `invoke("open_profile", { username })`), full name, "Follows you" badge, "You follow them" badge, a colored `status` tag
- [x] Implement `src/components/StatusEmpty.tsx` for the no-snapshot-yet state
- [x] Wire `src/App.tsx`: call `get_session_state` on mount; route to `LoginView` when not logged in, `MainView` otherwise
- [x] Smoke-test locally with `cargo tauri dev` (manual, skipped - not automatable)
- [x] Confirm `cargo check` is green

### Task 7: Session-expired and rate-limited banners

- [x] Implement `src/components/SessionExpiredBanner.tsx` ("Please log in again") with a button that flips the app back to `LoginView` and reloads the IG webview to the login URL
- [x] Implement `src/components/RateLimitedBanner.tsx` ("Instagram is rate-limiting — try again later")
- [x] Extend `src/lib/tauri.ts` to surface `AppError` discriminants from the Rust side to the UI as a typed union
- [x] Update `MainView.tsx` to render the appropriate banner when `sync_now` rejects with `SessionExpired` or `RateLimited`
- [x] Confirm `cargo check` is green

### Task 8: README — dev, build, first-launch

- [x] Create `README.md` with sections: Overview, Privacy (no password, no server), Prerequisites (Rust stable, Node 20+, Xcode CLI tools), Dev loop (`npm install` + `cargo tauri dev`), Release build (`cargo tauri build --target universal-apple-darwin`), First-launch instructions (right-click the `.app` → Open → Open again on the Gatekeeper prompt), Troubleshooting (rate-limit banner, session-expired flow), Known limits (20k users, v1 read-only)
- [x] Include a screenshot placeholder (`docs/screenshot.png`) — don't commit the image in this task
- [x] Confirm `cargo check` is green

### Task 9: Universal macOS bundle config

- [x] In `src-tauri/tauri.conf.json`, ensure `bundle.active = true`, `bundle.targets` includes `"dmg"` and `"app"`, and `bundle.macOS.minimumSystemVersion` is set to `"10.15"`
- [x] Add a project-local script or README note: `rustup target add aarch64-apple-darwin x86_64-apple-darwin` (do not run inside the task; the validation command already covers `cargo check`)
- [x] Verify the Tauri CLI accepts `--target universal-apple-darwin` by running `npx tauri build --target universal-apple-darwin --bundles app --verbose` manually (skipped - not automatable; requires extra toolchain targets and long build time)
- [x] Confirm `cargo check` is green
