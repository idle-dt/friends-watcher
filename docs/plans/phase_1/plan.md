# followers-watcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS Tauri 2 desktop app that tracks a single Instagram user's followers/following via the logged-in web API and surfaces who unfollowed them since the last sync.

**Architecture:** One Tauri app with two WKWebViews — a main React UI webview and a second "ig" webview opened on demand for Instagram login. Rust backend harvests session cookies from the ig webview (`WebviewWindow::cookies_for_url`), makes authenticated calls to `instagram.com/api/v1/*` via `reqwest`, and writes append-only snapshots to a local SQLite DB. Diffs are computed on the fly by joining the two most recent snapshots. All external links open via `tauri-plugin-opener` (never hardcoded to Safari). Sync is strictly user-initiated (no background jobs, no notifications, no unfollow actions).

**Tech Stack:** Tauri 2.x, Rust (`reqwest` w/ cookie-store, `rusqlite` bundled, `serde`, `tokio`, `tauri-plugin-opener`, `url`, `thiserror`), Vite + TypeScript + React, SQLite stored at `~/Library/Application Support/com.followerswatcher.app/data.db`.

**Authoritative spec:** `/Users/denistaranenko/.claude/plans/playful-plotting-diffie.md` (read first). This plan refines that spec into concrete file paths, code sketches, and verification commands.

**Working directory:** `/Users/denistaranenko/Work/friends-watcher/` (the on-disk directory name is unchanged; only the app's productName, bundle identifier, and GitHub repo are named `followers-watcher`). Currently contains only `prompt.md` and this `PLAN.md`.

---

## Prerequisites (run once, before Task 0)

- [ ] **Step P1: Verify toolchains**

Run: `cargo --version && node --version && npm --version && gh --version && gh auth status`
Expected: Rust ≥ 1.77, Node ≥ 20, npm ≥ 10, gh logged in. If `gh auth status` is not logged in, STOP and ask the user to run `gh auth login`.

- [ ] **Step P2: Install Tauri CLI (Rust) globally if absent**

Run: `cargo install tauri-cli --version "^2.0" --locked`
Expected: installs `cargo-tauri` binary. Verify with `cargo tauri --version` (should print a 2.x version).

---

## Task 0: Scaffold Tauri 2 project + git + GitHub repo

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/src/main.rs` (minimal)
- Create: `src-tauri/src/lib.rs` (minimal — Tauri 2 splits entry)
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/icons/` (placeholder icons — use Tauri's default generator)
- Create: `package.json`
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `index.html`
- Create: `src/main.tsx` (mounts `<div>OK</div>` for now — real UI comes in Task 7)
- Create: `src/styles.css`
- Create: `.gitignore`

- [ ] **Step 0.1: Scaffold via `create-tauri-app`**

Goal: produce a Tauri 2 scaffold with a React + TypeScript frontend at the project root. The CLI's exact flags vary between versions — use whichever form works. Example:

```bash
cd /tmp
npm create tauri-app@latest followers-watcher-scaffold -- --template react-ts --manager npm --identifier com.followerswatcher.app --yes
# If --yes is rejected by your CLI version, run it interactively and pick: React, TypeScript, npm.
rsync -a followers-watcher-scaffold/ /Users/denistaranenko/Work/friends-watcher/
rm -rf followers-watcher-scaffold
cd /Users/denistaranenko/Work/friends-watcher
```

Expected after the scaffold: directory contains `src/`, `src-tauri/`, `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, plus `prompt.md` and `PLAN.md` from before. If `rsync` refuses to overwrite a file, inspect manually — `prompt.md` and `PLAN.md` must not be touched.

- [ ] **Step 0.2: Set app identifier and window config in `src-tauri/tauri.conf.json`**

Edit `src-tauri/tauri.conf.json` so it contains (merge with scaffold, do not wipe):
```json
{
  "productName": "followers-watcher",
  "version": "0.1.0",
  "identifier": "com.followerswatcher.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "followers-watcher",
        "width": 1100,
        "height": 780,
        "resizable": true,
        "url": "index.html"
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "app",
    "icon": ["icons/icon.icns"],
    "macOS": { "minimumSystemVersion": "11.0" }
  }
}
```
Note: `csp: null` is acceptable for a personal, local-only tool. We don't want CSP fighting the IG webview.

- [ ] **Step 0.3: Add core Rust dependencies to `src-tauri/Cargo.toml`**

Edit `[dependencies]` section so it contains at least:
```toml
tauri = { version = "2", features = ["unstable"] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json", "cookies", "gzip"] }
rusqlite = { version = "0.32", features = ["bundled"] }
url = "2"
thiserror = "1"
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"
log = "0.4"
tauri-plugin-log = "2"
```
Run: `cd src-tauri && cargo check` — expected: compiles (the scaffold `main.rs`/`lib.rs` is still minimal). Fix any resolver complaints before moving on.

- [ ] **Step 0.4: Create `.gitignore` at project root**

```gitignore
# Rust
/src-tauri/target/
/src-tauri/Cargo.lock

# Node
node_modules/
dist/
.vite/

# macOS
.DS_Store

# Local DB / env / test artifacts
*.db
*.db-journal
.env
.env.*
!.env.example

# Editor
.idea/
.vscode/
*.swp
```
Note: we *do* commit `Cargo.lock` typically for binaries — so remove the `/src-tauri/Cargo.lock` line. Keep it out of the .gitignore.

Corrected `.gitignore` — remove the Cargo.lock line:
```gitignore
# Rust
/src-tauri/target/

# Node
node_modules/
dist/
.vite/

# macOS
.DS_Store

# Local DB / env / test artifacts
*.db
*.db-journal
.env
.env.*
!.env.example

# Editor
.idea/
.vscode/
*.swp

# Claude Code / AI tooling — exclude all Claude artifacts
.claude/
.claude-*/
CLAUDE.md
CLAUDE.local.md
claude*.md
.claude.json
.claude.local.json
```

- [ ] **Step 0.5: `cargo check` in src-tauri**

Run: `cd src-tauri && cargo check`
Expected: compiles with no errors (warnings OK). If it fails, read the error — likely a version mismatch with `tauri = "2"` features. Do NOT proceed until green.

- [ ] **Step 0.6: `npm install` + `npm run build` smoke test**

Run from project root:
```bash
npm install
npm run build
```
Expected: Vite produces `dist/index.html`. Errors here are almost always a missing tsconfig — fix before moving on.

- [ ] **Step 0.7: `git init` and first commit**

Run from project root:
```bash
git init
git checkout -b main 2>/dev/null || git branch -M main
git add .gitignore PLAN.md prompt.md package.json package-lock.json tsconfig*.json vite.config.ts index.html src/ src-tauri/
git status
```
Verify `git status` shows **no** files under `target/`, `node_modules/`, `dist/` as staged. If it does, stop and fix `.gitignore`.

Then commit:
```bash
git commit -m "chore: scaffold Tauri 2 project + baseline config"
```

- [ ] **Step 0.8: Create private GitHub repo and push**

Run: `gh repo create followers-watcher --private --source=. --remote=origin --push`
Expected: prints the new repo URL and pushes `main`. Verify: `git remote -v` shows `origin` pointing at `github.com/<user>/followers-watcher`.

---

## Task 1: Error types + shared models

**Files:**
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/models.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod error; mod models;`)

- [ ] **Step 1.1: Write `src-tauri/src/error.rs`**

```rust
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("session expired — please log in again")]
    SessionExpired,
    #[error("Instagram is rate-limiting — try again later")]
    RateLimited,
    #[error("instagram returned an unexpected response: {0}")]
    UnexpectedResponse(String),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("no session cookies found in webview")]
    NoSessionCookies,
    #[error("tauri error: {0}")]
    Tauri(String),
    #[error("{0}")]
    Other(String),
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self { AppError::Tauri(e.to_string()) }
}

// Serialize as a tagged error for the frontend.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let (kind, msg) = match self {
            AppError::SessionExpired       => ("session_expired",   self.to_string()),
            AppError::RateLimited          => ("rate_limited",      self.to_string()),
            AppError::NoSessionCookies     => ("no_session_cookies",self.to_string()),
            AppError::UnexpectedResponse(_)=> ("unexpected",        self.to_string()),
            _                              => ("internal",          self.to_string()),
        };
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("kind", kind)?;
        st.serialize_field("message", &msg)?;
        st.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
```

- [ ] **Step 1.2: Write `src-tauri/src/models.rs`**

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgUser {
    #[serde(rename = "pk")]
    pub ig_user_id: String,
    pub username: String,
    #[serde(default)]
    pub full_name: Option<String>,
    #[serde(default)]
    pub is_verified: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub profile_pic_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub id: i64,
    pub taken_at: DateTime<Utc>,
    pub owner_user_id: String,
    pub owner_username: String,
    pub followers_count: i64,
    pub following_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Relationship {
    pub ig_user_id: String,
    pub username: String,
    pub full_name: Option<String>,
    pub is_verified: bool,
    pub profile_pic_url: Option<String>,
    pub follows_you: bool,
    pub you_follow_them: bool,
    pub status: RelationshipStatus,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipStatus {
    Mutual, // follows you AND you follow them
    Fan,    // follows you, you don't follow back
    Ghost,  // you follow them, they don't follow back
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffEntry {
    pub ig_user_id: String,
    pub username: String,
    pub full_name: Option<String>,
    pub profile_pic_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffResult {
    pub new_followers: Vec<DiffEntry>,
    pub lost_followers: Vec<DiffEntry>,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub new_followers: Vec<DiffEntry>,
    pub lost_followers: Vec<DiffEntry>,
    pub total_followers: i64,
    pub total_following: i64,
    pub taken_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub logged_in: bool,
    pub username: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
}
```

- [ ] **Step 1.3: Register modules in `src-tauri/src/lib.rs`**

Edit the top of `src-tauri/src/lib.rs` (create it if the scaffold didn't) to include:
```rust
mod error;
mod models;

// keep existing run() / command registrations below
```

- [ ] **Step 1.4: `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: green. Any unused-import warnings are fine.

(No commit yet — this lands with Task 2.)

---

## Task 2: SQLite schema, snapshot write, diff queries

**Files:**
- Create: `src-tauri/src/db.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod db;`)
- Test: `src-tauri/src/db.rs` has `#[cfg(test)] mod tests` at the bottom.

- [ ] **Step 2.1: Write failing tests first in `src-tauri/src/db.rs`**

Create the file with tests at the bottom (implementation empty/stub above):
```rust
use crate::error::{AppError, AppResult};
use crate::models::*;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use std::path::Path;

pub struct Db { conn: Connection }

impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> AppResult<Self> { todo!() }
    pub fn open_in_memory() -> AppResult<Self> { todo!() }
    pub fn init_schema(&self) -> AppResult<()> { todo!() }
    pub fn write_snapshot(
        &mut self,
        owner_user_id: &str,
        owner_username: &str,
        followers: &[IgUser],
        following: &[IgUser],
    ) -> AppResult<Snapshot> { todo!() }
    pub fn latest_snapshot(&self) -> AppResult<Option<Snapshot>> { todo!() }
    pub fn get_latest_relationships(&self) -> AppResult<Vec<Relationship>> { todo!() }
    pub fn get_diff_since_previous(&self) -> AppResult<DiffResult> { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: &str, name: &str) -> IgUser {
        IgUser {
            ig_user_id: id.into(), username: name.into(),
            full_name: None, is_verified: false, is_private: false, profile_pic_url: None,
        }
    }

    #[test]
    fn schema_and_empty_queries() {
        let db = Db::open_in_memory().unwrap();
        db.init_schema().unwrap();
        assert!(db.latest_snapshot().unwrap().is_none());
        assert!(db.get_latest_relationships().unwrap().is_empty());
        let diff = db.get_diff_since_previous().unwrap();
        assert!(diff.new_followers.is_empty() && diff.lost_followers.is_empty());
    }

    #[test]
    fn first_snapshot_yields_mutual_fan_ghost_and_no_diff() {
        let mut db = Db::open_in_memory().unwrap();
        db.init_schema().unwrap();
        // alice: mutual, bob: fan (follows me, I don't), carol: ghost (I follow, doesn't follow back)
        let followers = vec![u("1","alice"), u("2","bob")];
        let following = vec![u("1","alice"), u("3","carol")];
        db.write_snapshot("me", "me_user", &followers, &following).unwrap();

        let rels = db.get_latest_relationships().unwrap();
        let by_user: std::collections::HashMap<_,_> =
            rels.iter().map(|r| (r.username.clone(), r)).collect();
        assert!(matches!(by_user["alice"].status, RelationshipStatus::Mutual));
        assert!(matches!(by_user["bob"].status,   RelationshipStatus::Fan));
        assert!(matches!(by_user["carol"].status, RelationshipStatus::Ghost));
        // First snapshot: no previous → diff empty
        let diff = db.get_diff_since_previous().unwrap();
        assert!(diff.new_followers.is_empty() && diff.lost_followers.is_empty());
    }

    #[test]
    fn second_snapshot_surfaces_new_and_lost_followers() {
        let mut db = Db::open_in_memory().unwrap();
        db.init_schema().unwrap();
        // T1: followers = [alice, bob]
        db.write_snapshot("me","me_user",
            &[u("1","alice"), u("2","bob")],
            &[u("1","alice")]).unwrap();
        // T2: followers = [alice, dave] — bob lost, dave new
        db.write_snapshot("me","me_user",
            &[u("1","alice"), u("4","dave")],
            &[u("1","alice")]).unwrap();
        let diff = db.get_diff_since_previous().unwrap();
        let lost: Vec<_> = diff.lost_followers.iter().map(|e| &e.username).collect();
        let new_:  Vec<_> = diff.new_followers.iter().map(|e| &e.username).collect();
        assert_eq!(lost, vec![&"bob".to_string()]);
        assert_eq!(new_, vec![&"dave".to_string()]);
    }
}
```

- [ ] **Step 2.2: Run tests — expect fail (todo!() panics)**

Run: `cd src-tauri && cargo test db::tests`
Expected: compile passes, tests panic with `not yet implemented`.

- [ ] **Step 2.3: Implement `Db` methods**

Replace the stub impls with:
```rust
impl Db {
    pub fn open<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        if let Some(parent) = path.as_ref().parent() { std::fs::create_dir_all(parent)?; }
        Ok(Self { conn: Connection::open(path)? })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Ok(Self { conn: Connection::open_in_memory()? })
    }

    pub fn init_schema(&self) -> AppResult<()> {
        self.conn.execute_batch(r#"
            CREATE TABLE IF NOT EXISTS snapshots (
              id              INTEGER PRIMARY KEY AUTOINCREMENT,
              taken_at        TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
              owner_user_id   TEXT NOT NULL,
              owner_username  TEXT NOT NULL,
              followers_count INTEGER NOT NULL,
              following_count INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS followers (
              snapshot_id     INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
              ig_user_id      TEXT NOT NULL,
              username        TEXT NOT NULL,
              full_name       TEXT,
              is_verified     INTEGER NOT NULL DEFAULT 0,
              profile_pic_url TEXT,
              PRIMARY KEY (snapshot_id, ig_user_id)
            );
            CREATE TABLE IF NOT EXISTS following (
              snapshot_id     INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
              ig_user_id      TEXT NOT NULL,
              username        TEXT NOT NULL,
              full_name       TEXT,
              is_verified     INTEGER NOT NULL DEFAULT 0,
              profile_pic_url TEXT,
              PRIMARY KEY (snapshot_id, ig_user_id)
            );
            CREATE INDEX IF NOT EXISTS idx_followers_snapshot ON followers(snapshot_id);
            CREATE INDEX IF NOT EXISTS idx_following_snapshot ON following(snapshot_id);
            CREATE INDEX IF NOT EXISTS idx_snapshots_taken_at ON snapshots(taken_at DESC);
        "#)?;
        Ok(())
    }

    pub fn write_snapshot(
        &mut self,
        owner_user_id: &str,
        owner_username: &str,
        followers: &[IgUser],
        following: &[IgUser],
    ) -> AppResult<Snapshot> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshots (owner_user_id, owner_username, followers_count, following_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![owner_user_id, owner_username, followers.len() as i64, following.len() as i64],
        )?;
        let snapshot_id = tx.last_insert_rowid();
        {
            let mut ins_f = tx.prepare(
                "INSERT INTO followers (snapshot_id, ig_user_id, username, full_name, is_verified, profile_pic_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for u in followers {
                ins_f.execute(params![snapshot_id, u.ig_user_id, u.username, u.full_name, u.is_verified as i64, u.profile_pic_url])?;
            }
            let mut ins_g = tx.prepare(
                "INSERT INTO following (snapshot_id, ig_user_id, username, full_name, is_verified, profile_pic_url)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for u in following {
                ins_g.execute(params![snapshot_id, u.ig_user_id, u.username, u.full_name, u.is_verified as i64, u.profile_pic_url])?;
            }
        }
        tx.commit()?;
        self.snapshot_by_id(snapshot_id).map(|o| o.unwrap())
    }

    fn snapshot_by_id(&self, id: i64) -> AppResult<Option<Snapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, taken_at, owner_user_id, owner_username, followers_count, following_count
             FROM snapshots WHERE id = ?1")?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(Snapshot {
                id: row.get(0)?,
                taken_at: row.get::<_, DateTime<Utc>>(1)?,
                owner_user_id: row.get(2)?,
                owner_username: row.get(3)?,
                followers_count: row.get(4)?,
                following_count: row.get(5)?,
            }))
        } else { Ok(None) }
    }

    pub fn latest_snapshot(&self) -> AppResult<Option<Snapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM snapshots ORDER BY id DESC LIMIT 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let id: i64 = row.get(0)?;
            self.snapshot_by_id(id)
        } else { Ok(None) }
    }

    fn previous_snapshot_id(&self) -> AppResult<Option<i64>> {
        let mut stmt = self.conn.prepare(
            "SELECT id FROM snapshots ORDER BY id DESC LIMIT 1 OFFSET 1")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? { Ok(Some(row.get(0)?)) } else { Ok(None) }
    }

    pub fn get_latest_relationships(&self) -> AppResult<Vec<Relationship>> {
        let Some(latest) = self.latest_snapshot()? else { return Ok(vec![]); };
        // Union followers + following for the latest snapshot; compute status per user.
        let mut stmt = self.conn.prepare(r#"
            WITH f AS (SELECT * FROM followers  WHERE snapshot_id = ?1),
                 g AS (SELECT * FROM following  WHERE snapshot_id = ?1)
            SELECT
              COALESCE(f.ig_user_id, g.ig_user_id) AS id,
              COALESCE(f.username,   g.username)   AS username,
              COALESCE(f.full_name,  g.full_name)  AS full_name,
              COALESCE(MAX(f.is_verified), MAX(g.is_verified), 0) AS is_verified,
              COALESCE(f.profile_pic_url, g.profile_pic_url) AS profile_pic_url,
              (f.ig_user_id IS NOT NULL) AS follows_you,
              (g.ig_user_id IS NOT NULL) AS you_follow_them
            FROM f
            FULL OUTER JOIN g ON f.ig_user_id = g.ig_user_id
            GROUP BY id
            ORDER BY username COLLATE NOCASE
        "#)?;
        // rusqlite pre-3.39 lacks FULL OUTER JOIN — use LEFT + anti-join union as a fallback if needed.
        // If the FULL OUTER JOIN fails at runtime, replace with:
        // SELECT ... FROM f LEFT JOIN g ... UNION ALL SELECT ... FROM g LEFT JOIN f ... WHERE f.ig_user_id IS NULL
        let rels = stmt.query_map(params![latest.id], |row| {
            let follows_you: bool    = row.get(5)?;
            let you_follow_them: bool = row.get(6)?;
            let status = match (follows_you, you_follow_them) {
                (true,  true)  => RelationshipStatus::Mutual,
                (true,  false) => RelationshipStatus::Fan,
                (false, true)  => RelationshipStatus::Ghost,
                (false, false) => RelationshipStatus::Mutual, // unreachable
            };
            Ok(Relationship {
                ig_user_id: row.get(0)?,
                username: row.get(1)?,
                full_name: row.get(2)?,
                is_verified: row.get::<_, i64>(3)? != 0,
                profile_pic_url: row.get(4)?,
                follows_you, you_follow_them, status,
            })
        })?.collect::<Result<Vec<_>, _>>()?;
        Ok(rels)
    }

    pub fn get_diff_since_previous(&self) -> AppResult<DiffResult> {
        let Some(latest) = self.latest_snapshot()? else {
            return Ok(DiffResult { new_followers: vec![], lost_followers: vec![], since: None });
        };
        let Some(prev_id) = self.previous_snapshot_id()? else {
            return Ok(DiffResult { new_followers: vec![], lost_followers: vec![], since: None });
        };
        let prev_taken = self.snapshot_by_id(prev_id)?.map(|s| s.taken_at);

        let new_followers  = self.followers_diff(latest.id, prev_id)?;
        let lost_followers = self.followers_diff(prev_id, latest.id)?;
        Ok(DiffResult { new_followers, lost_followers, since: prev_taken })
    }

    /// Followers present in snapshot `a` but NOT in snapshot `b`.
    fn followers_diff(&self, a: i64, b: i64) -> AppResult<Vec<DiffEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT ig_user_id, username, full_name, profile_pic_url
             FROM followers WHERE snapshot_id = ?1
             AND ig_user_id NOT IN (SELECT ig_user_id FROM followers WHERE snapshot_id = ?2)
             ORDER BY username COLLATE NOCASE"
        )?;
        let rows = stmt.query_map(params![a, b], |row| Ok(DiffEntry {
            ig_user_id: row.get(0)?,
            username: row.get(1)?,
            full_name: row.get(2)?,
            profile_pic_url: row.get(3)?,
        }))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}
```

Note: SQLite added FULL OUTER JOIN in 3.39 (rusqlite's bundled SQLite is much newer — 3.45+ — so this works). If `cargo test` fails at runtime on the FULL OUTER JOIN, replace `get_latest_relationships` with a UNION of two LEFT JOINs.

- [ ] **Step 2.4: Register `db` module and run tests**

Add to `src-tauri/src/lib.rs`: `mod db;`

Run: `cd src-tauri && cargo test db::tests -- --nocapture`
Expected: all 3 tests pass.

- [ ] **Step 2.5: Commit**

```bash
git add src-tauri/src/error.rs src-tauri/src/models.rs src-tauri/src/db.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(db): sqlite schema, snapshot write, diff queries"
git push origin main
```

---

## Task 3: Instagram API client with pagination + rate limiting

**Files:**
- Create: `src-tauri/src/instagram.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod instagram;`)
- Test: `src-tauri/src/instagram.rs` has `#[cfg(test)] mod tests`

- [ ] **Step 3.1: Add `wiremock` dev-dependency to `src-tauri/Cargo.toml`**

Append under `[dev-dependencies]` (create the section if absent):
```toml
[dev-dependencies]
wiremock = "0.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 3.2: Sketch `IgClient` + write failing tests**

Create `src-tauri/src/instagram.rs`:
```rust
use crate::error::{AppError, AppResult};
use crate::models::IgUser;
use reqwest::{Client, StatusCode, header};
use serde::Deserialize;
use std::time::Duration;

pub const BASE: &str = "https://www.instagram.com";
pub const X_IG_APP_ID: &str = "936619743392459";
pub const X_ASBD_ID:  &str = "198387";
pub const DEFAULT_PAGE_DELAY_MS: u64 = 1500;
pub const BACKOFF_STEPS_SECS: [u64; 3] = [5, 15, 45];
pub const MAX_USERS_PER_SYNC: usize = 20_000;
pub const PAGE_SIZE: usize = 50;

#[derive(Clone)]
pub struct SessionCookies {
    pub sessionid: String,
    pub csrftoken: String,
    pub ds_user_id: String,
    pub mid: Option<String>,
    pub ig_did: Option<String>,
}

impl SessionCookies {
    pub fn to_cookie_header(&self) -> String {
        let mut out = format!(
            "sessionid={}; csrftoken={}; ds_user_id={}",
            self.sessionid, self.csrftoken, self.ds_user_id
        );
        if let Some(m)  = &self.mid     { out.push_str(&format!("; mid={m}")); }
        if let Some(ig) = &self.ig_did  { out.push_str(&format!("; ig_did={ig}")); }
        out
    }
}

#[derive(Clone)]
pub struct IgClient {
    http: Client,
    base: String,
    user_agent: String,
    cookies: SessionCookies,
    page_delay: Duration,
}

impl IgClient {
    pub fn new(user_agent: String, cookies: SessionCookies) -> AppResult<Self> {
        let http = Client::builder()
            .gzip(true)
            .timeout(Duration::from_secs(30))
            .build()?;
        Ok(Self {
            http, base: BASE.into(), user_agent, cookies,
            page_delay: Duration::from_millis(DEFAULT_PAGE_DELAY_MS),
        })
    }

    #[cfg(test)]
    pub fn with_base(mut self, base: impl Into<String>) -> Self { self.base = base.into(); self }
    #[cfg(test)]
    pub fn with_page_delay(mut self, d: Duration) -> Self { self.page_delay = d; self }

    fn headers(&self) -> AppResult<header::HeaderMap> {
        use header::*;
        let mut h = HeaderMap::new();
        h.insert("X-IG-App-ID", X_IG_APP_ID.parse().unwrap());
        h.insert("X-CSRFToken", self.cookies.csrftoken.parse().map_err(|_| AppError::Other("bad csrf".into()))?);
        h.insert("X-Requested-With", "XMLHttpRequest".parse().unwrap());
        h.insert("X-ASBD-ID", X_ASBD_ID.parse().unwrap());
        h.insert(USER_AGENT, self.user_agent.parse().map_err(|_| AppError::Other("bad UA".into()))?);
        h.insert(REFERER, "https://www.instagram.com/".parse().unwrap());
        h.insert(ACCEPT, "*/*".parse().unwrap());
        h.insert(COOKIE, self.cookies.to_cookie_header().parse()
            .map_err(|_| AppError::Other("bad cookie header".into()))?);
        Ok(h)
    }

    /// GET with retry/backoff. Classifies 401/login_required → SessionExpired,
    /// 429/feedback_required/checkpoint_required → RateLimited (after backoff exhausted).
    async fn get_json(&self, url: &str) -> AppResult<serde_json::Value> {
        for attempt in 0..=BACKOFF_STEPS_SECS.len() {
            let resp = self.http.get(url).headers(self.headers()?).send().await?;
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED || body.contains("\"login_required\"") {
                return Err(AppError::SessionExpired);
            }
            let throttled = status == StatusCode::TOO_MANY_REQUESTS
                || body.contains("\"feedback_required\"")
                || body.contains("\"checkpoint_required\"");
            if throttled {
                if attempt < BACKOFF_STEPS_SECS.len() {
                    tokio::time::sleep(Duration::from_secs(BACKOFF_STEPS_SECS[attempt])).await;
                    continue;
                } else {
                    return Err(AppError::RateLimited);
                }
            }
            if !status.is_success() {
                return Err(AppError::UnexpectedResponse(format!("HTTP {status}: {}", body.chars().take(200).collect::<String>())));
            }
            return serde_json::from_str(&body).map_err(AppError::from);
        }
        Err(AppError::RateLimited)
    }

    pub async fn web_profile_info(&self, username: &str) -> AppResult<(String, String)> {
        let url = format!("{}/api/v1/users/web_profile_info/?username={username}", self.base);
        let v = self.get_json(&url).await?;
        let user = v.pointer("/data/user").ok_or_else(|| AppError::UnexpectedResponse("no /data/user".into()))?;
        let id = user.get("id").and_then(|x| x.as_str()).ok_or_else(|| AppError::UnexpectedResponse("no id".into()))?;
        let un = user.get("username").and_then(|x| x.as_str()).unwrap_or(username);
        Ok((id.to_string(), un.to_string()))
    }

    pub async fn followers(&self, user_id: &str) -> AppResult<Vec<IgUser>> {
        self.paginate(&format!("/api/v1/friendships/{user_id}/followers/")).await
    }

    pub async fn following(&self, user_id: &str) -> AppResult<Vec<IgUser>> {
        self.paginate(&format!("/api/v1/friendships/{user_id}/following/")).await
    }

    async fn paginate(&self, path: &str) -> AppResult<Vec<IgUser>> {
        #[derive(Deserialize)]
        struct Page { users: Vec<IgUser>, next_max_id: Option<String> }
        let mut all = Vec::<IgUser>::new();
        let mut cursor: Option<String> = None;
        loop {
            let url = match &cursor {
                Some(c) => format!("{}{path}?count={}&max_id={c}", self.base, PAGE_SIZE),
                None    => format!("{}{path}?count={}",             self.base, PAGE_SIZE),
            };
            let v = self.get_json(&url).await?;
            let page: Page = serde_json::from_value(v)?;
            all.extend(page.users);
            if all.len() >= MAX_USERS_PER_SYNC {
                all.truncate(MAX_USERS_PER_SYNC);
                break;
            }
            match page.next_max_id {
                Some(c) if !c.is_empty() => {
                    cursor = Some(c);
                    tokio::time::sleep(self.page_delay).await;
                }
                _ => break,
            }
        }
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path, query_param};

    fn cookies() -> SessionCookies {
        SessionCookies {
            sessionid: "sess".into(), csrftoken: "csrf".into(),
            ds_user_id: "42".into(), mid: None, ig_did: None,
        }
    }

    #[tokio::test]
    async fn web_profile_info_extracts_id_and_username() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/users/web_profile_info/"))
            .and(query_param("username", "tester"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": { "user": { "id": "123", "username": "tester",
                    "edge_followed_by": {"count":0}, "edge_follow": {"count":0} } },
                "status": "ok"
            })))
            .mount(&server).await;
        let c = IgClient::new("UA".into(), cookies()).unwrap().with_base(&server.uri());
        let (id, un) = c.web_profile_info("tester").await.unwrap();
        assert_eq!(id, "123"); assert_eq!(un, "tester");
    }

    #[tokio::test]
    async fn paginates_followers_and_stops_on_null_cursor() {
        let server = MockServer::start().await;
        // page 1
        Mock::given(method("GET")).and(path("/api/v1/friendships/42/followers/"))
            .and(query_param("count","50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "users": [{"pk":"1","username":"a","full_name":"A","is_verified":false,"is_private":false,"profile_pic_url":null}],
                "next_max_id": "cursor_2", "status":"ok"
            })))
            .mount(&server).await;
        // page 2 (final)
        Mock::given(method("GET")).and(path("/api/v1/friendships/42/followers/"))
            .and(query_param("max_id","cursor_2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "users": [{"pk":"2","username":"b","full_name":"B","is_verified":false,"is_private":false,"profile_pic_url":null}],
                "next_max_id": null, "status":"ok"
            })))
            .mount(&server).await;
        let c = IgClient::new("UA".into(), cookies()).unwrap()
            .with_base(&server.uri()).with_page_delay(Duration::from_millis(1));
        let got = c.followers("42").await.unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].username, "a");
        assert_eq!(got[1].username, "b");
    }

    #[tokio::test]
    async fn session_expired_on_login_required_body() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/v1/users/web_profile_info/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"message":"login_required","status":"fail"}"#))
            .mount(&server).await;
        let c = IgClient::new("UA".into(), cookies()).unwrap().with_base(&server.uri());
        match c.web_profile_info("x").await {
            Err(AppError::SessionExpired) => (),
            other => panic!("expected SessionExpired, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rate_limited_after_backoff_exhausted() {
        let server = MockServer::start().await;
        Mock::given(method("GET")).and(path("/api/v1/users/web_profile_info/"))
            .respond_with(ResponseTemplate::new(429).set_body_string("{}"))
            .mount(&server).await;
        let c = IgClient::new("UA".into(), cookies()).unwrap().with_base(&server.uri());
        // Patch backoff to near-zero for the test. Simpler: wrap client or shorten constants via a test-only
        // knob. Since BACKOFF_STEPS_SECS is a const, accept the ~65s test. Mark with #[ignore] if slow.
        // Alternative: make the steps configurable via a field — preferred. See Step 3.3.
        match c.web_profile_info("x").await {
            Err(AppError::RateLimited) => (),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }
}
```

- [ ] **Step 3.3: Make backoff configurable for tests**

Replace the const `BACKOFF_STEPS_SECS` usage with a field `backoff: Vec<Duration>` on `IgClient`, default `[5,15,45]` seconds. Add `.with_backoff(Vec<Duration>)` behind `#[cfg(test)]` so `rate_limited_after_backoff_exhausted` can set it to `[Duration::ZERO; 3]` and run in milliseconds.

- [ ] **Step 3.4: Run tests**

Run: `cd src-tauri && cargo test instagram::tests -- --nocapture`
Expected: 4 tests pass. If the pagination test hangs, the test-side `with_page_delay(1ms)` isn't being applied.

- [ ] **Step 3.5: Commit**

```bash
git add src-tauri/src/instagram.rs src-tauri/src/lib.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(ig): instagram api client with pagination + rate limiting"
git push origin main
```

---

## Task 4: Cookie harvesting from the IG webview

**Files:**
- Create: `src-tauri/src/cookies.rs`
- Modify: `src-tauri/src/lib.rs` (add `mod cookies;`)

- [ ] **Step 4.1: Write `src-tauri/src/cookies.rs`**

```rust
use crate::error::{AppError, AppResult};
use crate::instagram::SessionCookies;
use tauri::webview::WebviewWindow;
use url::Url;

const IG_URL: &str = "https://www.instagram.com";

/// Extracts the Instagram session cookies from a webview's cookie store.
/// Returns `NoSessionCookies` if `sessionid` is absent.
pub fn harvest(window: &WebviewWindow) -> AppResult<SessionCookies> {
    let url = Url::parse(IG_URL).expect("hardcoded URL is valid");
    let cookies = window.cookies_for_url(url)
        .map_err(|e| AppError::Tauri(e.to_string()))?;

    let mut sessionid = None;
    let mut csrftoken = None;
    let mut ds_user_id = None;
    let mut mid = None;
    let mut ig_did = None;

    for c in &cookies {
        match c.name().as_ref() {
            "sessionid"  => sessionid  = Some(c.value().to_string()),
            "csrftoken"  => csrftoken  = Some(c.value().to_string()),
            "ds_user_id" => ds_user_id = Some(c.value().to_string()),
            "mid"        => mid        = Some(c.value().to_string()),
            "ig_did"     => ig_did     = Some(c.value().to_string()),
            _ => {}
        }
    }

    let sessionid  = sessionid.ok_or(AppError::NoSessionCookies)?;
    let csrftoken  = csrftoken.ok_or(AppError::NoSessionCookies)?;
    let ds_user_id = ds_user_id.ok_or(AppError::NoSessionCookies)?;
    Ok(SessionCookies { sessionid, csrftoken, ds_user_id, mid, ig_did })
}

/// Reads the UA from the webview at runtime via JS eval bridge.
/// Impl note: stored to AppState by the frontend sending an event after
/// navigator.userAgent is available. We just provide a helper to read it
/// from whatever state holder we use.
pub fn ig_url() -> &'static str { IG_URL }
```

- [ ] **Step 4.2: Register module**

Add `mod cookies;` to `src-tauri/src/lib.rs`.

- [ ] **Step 4.3: `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: green.

- [ ] **Step 4.4: Commit**

```bash
git add src-tauri/src/cookies.rs src-tauri/src/lib.rs
git commit -m "feat(cookies): harvest session cookies from webview"
git push origin main
```

---

## Task 5: Tauri commands (sync_now, get_*, open_profile) + wire up plugins

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (full `run()` — register state, plugins, commands, spawn handler)
- Modify: `src-tauri/src/main.rs` (trivial — call `lib::run()`)
- Modify: `src-tauri/Cargo.toml` (add `tokio::sync::Mutex` — no new dep needed; tokio already includes it)
- Modify: `src-tauri/capabilities/default.json` (allow core + opener)

- [ ] **Step 5.1: Write `src-tauri/src/commands.rs`**

```rust
use crate::cookies;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::instagram::{IgClient, SessionCookies};
use crate::models::*;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager, Emitter};
use tokio::sync::Mutex;

pub struct AppState {
    pub db: Arc<Mutex<Db>>,
    /// Captured via `navigator.userAgent` from the main webview on startup.
    pub user_agent: Arc<Mutex<Option<String>>>,
    pub owner: Arc<Mutex<Option<(String /*id*/, String /*username*/)>>>,
}

impl AppState {
    pub fn new(db_path: PathBuf) -> AppResult<Self> {
        let db = Db::open(&db_path)?;
        db.init_schema()?;
        Ok(Self {
            db: Arc::new(Mutex::new(db)),
            user_agent: Arc::new(Mutex::new(None)),
            owner: Arc::new(Mutex::new(None)),
        })
    }
}

pub fn default_db_path() -> PathBuf {
    let mut p = dirs::data_dir().expect("no $APPDATA");
    p.push("com.followerswatcher.app");
    p.push("data.db");
    p
}

#[tauri::command]
pub async fn set_user_agent(state: tauri::State<'_, AppState>, ua: String) -> AppResult<()> {
    *state.user_agent.lock().await = Some(ua);
    Ok(())
}

#[tauri::command]
pub async fn get_session_state(app: AppHandle, state: tauri::State<'_, AppState>) -> AppResult<SessionState> {
    let logged_in = app.get_webview_window("ig")
        .map(|w| cookies::harvest(&w).is_ok())
        .unwrap_or(false)
        || app.get_webview_window("main")
            .map(|w| cookies::harvest(&w).is_ok())
            .unwrap_or(false);

    let latest = state.db.lock().await.latest_snapshot()?;
    let (username, last_sync_at) = match latest {
        Some(s) => (Some(s.owner_username), Some(s.taken_at)),
        None => (None, None),
    };
    Ok(SessionState { logged_in, username, last_sync_at })
}

#[tauri::command]
pub async fn open_ig_login(app: AppHandle) -> AppResult<()> {
    use tauri::{WebviewUrl, webview::WebviewWindowBuilder};
    if let Some(existing) = app.get_webview_window("ig") {
        existing.show().ok();
        existing.set_focus().ok();
        return Ok(());
    }
    WebviewWindowBuilder::new(&app, "ig",
        WebviewUrl::External("https://www.instagram.com/accounts/login/".parse().unwrap()))
        .title("Instagram — log in")
        .inner_size(900.0, 720.0)
        .build()
        .map_err(|e| AppError::Tauri(e.to_string()))?;
    Ok(())
}

#[tauri::command]
pub async fn close_ig_login(app: AppHandle) -> AppResult<()> {
    if let Some(w) = app.get_webview_window("ig") { w.close().ok(); }
    Ok(())
}

#[tauri::command]
pub async fn sync_now(app: AppHandle, state: tauri::State<'_, AppState>) -> AppResult<SyncResult> {
    // 1) Harvest cookies — prefer the "ig" window, fall back to "main" (in case IG was loaded there).
    let window = app.get_webview_window("ig")
        .or_else(|| app.get_webview_window("main"))
        .ok_or(AppError::Other("no webview available".into()))?;
    let cookies = cookies::harvest(&window)?;

    // 2) Captured UA, else fall back to a generic Safari macOS UA.
    let ua = state.user_agent.lock().await.clone()
        .unwrap_or_else(|| "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".to_string());

    let client = IgClient::new(ua, cookies.clone())?;

    // 3) Resolve owner id/username via ds_user_id cookie OR web_profile_info.
    //    We don't know the user's handle from ds_user_id alone; query info for the user's OWN profile:
    //    IG returns user data when querying /users/web_profile_info/?username=<own_handle>.
    //    The trick: we can reverse the id via the `users/<id>/info` endpoint, but simpler is to read the
    //    owner handle from the main webview's current URL after login (IG redirects to /<username>/).
    //    To stay simple and deterministic, read the owner's username that the frontend sent in via
    //    set_owner_username. If not set, error.
    let (owner_id, owner_username) = state.owner.lock().await.clone()
        .ok_or(AppError::Other("owner profile not resolved; log in first".into()))?;

    // Double-check id by hitting web_profile_info (also surfaces SessionExpired fast).
    let (id_check, _) = client.web_profile_info(&owner_username).await?;
    if id_check != owner_id {
        return Err(AppError::UnexpectedResponse(format!(
            "ds_user_id {} ≠ web_profile_info id {}", owner_id, id_check
        )));
    }

    // 4) Paginate both lists.
    let followers = client.followers(&owner_id).await?;
    let following = client.following(&owner_id).await?;

    // 5) Snapshot + diff.
    let mut db = state.db.lock().await;
    let taken_at = Utc::now();
    let _snap = db.write_snapshot(&owner_id, &owner_username, &followers, &following)?;
    let diff = db.get_diff_since_previous()?;

    // 6) Emit progress-complete event for UI (optional).
    app.emit("sync-complete", ()).ok();

    Ok(SyncResult {
        new_followers:  diff.new_followers,
        lost_followers: diff.lost_followers,
        total_followers: followers.len() as i64,
        total_following: following.len() as i64,
        taken_at,
    })
}

#[tauri::command]
pub async fn set_owner(state: tauri::State<'_, AppState>, id: String, username: String) -> AppResult<()> {
    *state.owner.lock().await = Some((id, username));
    Ok(())
}

#[tauri::command]
pub async fn resolve_owner_from_session(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    username_hint: String,
) -> AppResult<(String, String)> {
    let window = app.get_webview_window("ig")
        .or_else(|| app.get_webview_window("main"))
        .ok_or(AppError::Other("no webview".into()))?;
    let cookies = cookies::harvest(&window)?;
    let ua = state.user_agent.lock().await.clone()
        .unwrap_or_else(|| "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15".to_string());
    let client = IgClient::new(ua, cookies.clone())?;
    let (id, username) = client.web_profile_info(&username_hint).await?;
    // Verify id matches ds_user_id cookie.
    if id != cookies.ds_user_id {
        return Err(AppError::UnexpectedResponse(format!(
            "username {username_hint} does not match the logged-in account"
        )));
    }
    *state.owner.lock().await = Some((id.clone(), username.clone()));
    Ok((id, username))
}

#[tauri::command]
pub async fn get_latest_relationships(state: tauri::State<'_, AppState>) -> AppResult<Vec<Relationship>> {
    state.db.lock().await.get_latest_relationships()
}

#[tauri::command]
pub async fn get_diff_since_previous(state: tauri::State<'_, AppState>) -> AppResult<DiffResult> {
    state.db.lock().await.get_diff_since_previous()
}

#[tauri::command]
pub fn open_profile(app: AppHandle, username: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let url = format!("https://instagram.com/{username}");
    app.opener().open_url(url, None::<&str>)
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(())
}
```

**Design note on owner resolution:** we require the frontend to prompt the user for their own handle once (or read it from the URL the IG webview redirects to — future improvement). For v1 simplicity, the LoginView captures the handle from the IG webview URL after login (`instagram.com/<username>/`) and calls `resolve_owner_from_session`.

- [ ] **Step 5.2: Rewrite `src-tauri/src/lib.rs`**

```rust
mod cookies;
mod commands;
mod db;
mod error;
mod instagram;
mod models;

use commands::{AppState, default_db_path};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .setup(|app| {
            let state = AppState::new(default_db_path())
                .expect("failed to open app DB");
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::set_user_agent,
            commands::get_session_state,
            commands::open_ig_login,
            commands::close_ig_login,
            commands::sync_now,
            commands::set_owner,
            commands::resolve_owner_from_session,
            commands::get_latest_relationships,
            commands::get_diff_since_previous,
            commands::open_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5.3: Ensure `src-tauri/src/main.rs` just calls the lib**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    followers_watcher_lib::run();
}
```
(Adjust the lib name to whatever the scaffold produced — check `Cargo.toml` `[lib] name`. Default scaffold uses `{crate_name}_lib`.)

- [ ] **Step 5.4: Update `src-tauri/capabilities/default.json`**

Ensure it grants opener + core permissions to the main window:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Default capabilities for followers-watcher",
  "windows": ["main", "ig"],
  "permissions": [
    "core:default",
    "opener:default",
    "opener:allow-open-url"
  ]
}
```

- [ ] **Step 5.5: `cargo check`**

Run: `cd src-tauri && cargo check`
Expected: green. Fix compile errors before committing — typical issues: mismatched `WebviewUrl::External` import, missing feature flags on `tauri = { features = ["unstable"] }`.

- [ ] **Step 5.6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/main.rs src-tauri/capabilities/default.json
git commit -m "feat(commands): tauri command handlers (sync, diff, open_profile)"
git push origin main
```

---

## Task 6: Frontend — invoke wrappers, LoginView, MainView, table, diff banner

**Files:**
- Create: `src/lib/tauri.ts`
- Create: `src/views/LoginView.tsx`
- Create: `src/views/MainView.tsx`
- Create: `src/components/RelationshipRow.tsx`
- Create: `src/components/DiffBanner.tsx`
- Create: `src/components/StatusEmpty.tsx`
- Modify: `src/App.tsx` (routing between views + status polling)
- Modify: `src/main.tsx` (send UA to backend on boot)
- Modify: `src/styles.css` (minimal layout — table, banner, buttons)

- [ ] **Step 6.1: Install frontend deps**

Run from project root:
```bash
npm install @tauri-apps/api@^2 @tauri-apps/plugin-opener@^2
```
Verify `package.json` lists both.

- [ ] **Step 6.2: `src/lib/tauri.ts` — typed invoke wrappers**

```ts
import { invoke } from "@tauri-apps/api/core";

export type DiffEntry = {
  igUserId: string; username: string;
  fullName: string | null; profilePicUrl: string | null;
};
export type DiffResult = {
  newFollowers: DiffEntry[]; lostFollowers: DiffEntry[];
  since: string | null;
};
export type SyncResult = {
  newFollowers: DiffEntry[]; lostFollowers: DiffEntry[];
  totalFollowers: number; totalFollowing: number;
  takenAt: string;
};
export type RelationshipStatus = "mutual" | "fan" | "ghost";
export type Relationship = {
  igUserId: string; username: string;
  fullName: string | null; isVerified: boolean;
  profilePicUrl: string | null;
  followsYou: boolean; youFollowThem: boolean;
  status: RelationshipStatus;
};
export type SessionState = {
  loggedIn: boolean; username: string | null; lastSyncAt: string | null;
};

// Backend serializes errors as { kind, message }.
export type AppErrorShape = { kind: string; message: string };

export const api = {
  setUserAgent: (ua: string)       => invoke<void>("set_user_agent", { ua }),
  getSessionState: ()              => invoke<SessionState>("get_session_state"),
  openIgLogin: ()                  => invoke<void>("open_ig_login"),
  closeIgLogin: ()                 => invoke<void>("close_ig_login"),
  resolveOwner: (usernameHint: string) =>
    invoke<[string, string]>("resolve_owner_from_session", { usernameHint }),
  syncNow: ()                      => invoke<SyncResult>("sync_now"),
  getLatestRelationships: ()       => invoke<Relationship[]>("get_latest_relationships"),
  getDiffSincePrevious: ()         => invoke<DiffResult>("get_diff_since_previous"),
  openProfile: (username: string)  => invoke<void>("open_profile", { username }),
};
```

- [ ] **Step 6.3: `src/main.tsx`**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";
import { api } from "./lib/tauri";

// Report the actual WKWebView UA to the backend so HTTP calls match.
api.setUserAgent(navigator.userAgent).catch(console.error);

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode><App /></React.StrictMode>
);
```

- [ ] **Step 6.4: `src/App.tsx`**

```tsx
import { useEffect, useState } from "react";
import LoginView from "./views/LoginView";
import MainView from "./views/MainView";
import { api, type SessionState } from "./lib/tauri";

export default function App() {
  const [session, setSession] = useState<SessionState | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = async () => {
    try {
      const s = await api.getSessionState();
      setSession(s);
    } finally { setLoading(false); }
  };

  useEffect(() => {
    refresh();
    // Poll every 2s — cheap and simple; Instagram login window emits no events to us directly.
    const t = setInterval(refresh, 2000);
    return () => clearInterval(t);
  }, []);

  if (loading) return <div className="center">Loading…</div>;
  if (!session?.loggedIn) return <LoginView onLogin={refresh} />;
  return <MainView session={session} />;
}
```

- [ ] **Step 6.5: `src/views/LoginView.tsx`**

```tsx
import { useState } from "react";
import { api } from "../lib/tauri";

export default function LoginView({ onLogin }: { onLogin: () => void }) {
  const [handle, setHandle] = useState("");
  const [status, setStatus] = useState<"idle"|"waiting"|"resolving"|"error">("idle");
  const [err, setErr] = useState<string | null>(null);

  const startLogin = async () => {
    setStatus("waiting"); setErr(null);
    try {
      await api.openIgLogin();
      // The session poll in App.tsx will pick up the cookie. Once sessionid is present,
      // session.loggedIn flips true and we leave this view automatically.
    } catch (e: any) {
      setErr(e?.message ?? String(e)); setStatus("error");
    }
  };

  const confirmHandle = async () => {
    setStatus("resolving"); setErr(null);
    try {
      await api.resolveOwner(handle.trim());
      await api.closeIgLogin();
      onLogin();
    } catch (e: any) {
      setErr(e?.message ?? String(e)); setStatus("error");
    }
  };

  return (
    <div className="login">
      <h1>followers-watcher</h1>
      <p>Log in to your Instagram account to start tracking followers.</p>
      <button onClick={startLogin}>Open Instagram login</button>
      <p className="hint">After you finish logging in, come back here and enter your handle:</p>
      <input
        value={handle}
        onChange={(e) => setHandle(e.target.value)}
        placeholder="yourhandle"
        autoCapitalize="none" autoCorrect="off" spellCheck={false}
      />
      <button disabled={!handle.trim()} onClick={confirmHandle}>Confirm</button>
      {err && <p className="error">{err}</p>}
    </div>
  );
}
```

- [ ] **Step 6.6: `src/components/DiffBanner.tsx`**

```tsx
import type { DiffResult } from "../lib/tauri";

export default function DiffBanner({ diff }: { diff: DiffResult | null }) {
  if (!diff || !diff.since) return null;
  const since = new Date(diff.since).toLocaleDateString();
  const lost = diff.lostFollowers.length;
  const newN = diff.newFollowers.length;
  if (!lost && !newN) return <div className="banner muted">No changes since {since}.</div>;
  return (
    <div className="banner">
      <strong>{lost}</strong> unfollowed you since {since}
      {newN > 0 && <> · <strong>{newN}</strong> new followers</>}
    </div>
  );
}
```

- [ ] **Step 6.7: `src/components/RelationshipRow.tsx`**

```tsx
import type { Relationship } from "../lib/tauri";
import { api } from "../lib/tauri";

export default function RelationshipRow({ r }: { r: Relationship }) {
  return (
    <tr onClick={() => api.openProfile(r.username)} style={{cursor:"pointer"}}>
      <td>{r.profilePicUrl
        ? <img src={r.profilePicUrl} alt="" width={32} height={32} />
        : <span className="avatar-placeholder" />}</td>
      <td>@{r.username}{r.isVerified && " ✓"}</td>
      <td>{r.fullName ?? ""}</td>
      <td>{r.followsYou ? "yes" : "—"}</td>
      <td>{r.youFollowThem ? "yes" : "—"}</td>
      <td><span className={`tag tag-${r.status}`}>{r.status}</span></td>
    </tr>
  );
}
```

- [ ] **Step 6.8: `src/components/StatusEmpty.tsx`**

```tsx
export default function StatusEmpty() {
  return <div className="empty">Tap <strong>Sync</strong> to see your followers.</div>;
}
```

- [ ] **Step 6.9: `src/views/MainView.tsx`**

```tsx
import { useEffect, useState } from "react";
import { api, type DiffResult, type Relationship, type SessionState } from "../lib/tauri";
import DiffBanner from "../components/DiffBanner";
import RelationshipRow from "../components/RelationshipRow";
import StatusEmpty from "../components/StatusEmpty";

export default function MainView({ session }: { session: SessionState }) {
  const [rels, setRels] = useState<Relationship[]>([]);
  const [diff, setDiff] = useState<DiffResult | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<{ kind: string; message: string } | null>(null);

  const refresh = async () => {
    setRels(await api.getLatestRelationships());
    setDiff(await api.getDiffSincePrevious());
  };

  useEffect(() => { refresh(); }, []);

  const doSync = async () => {
    setSyncing(true); setError(null);
    try {
      await api.syncNow();
      await refresh();
    } catch (e: any) {
      // e from Tauri invoke is already the serialized AppError { kind, message }.
      setError(typeof e === "object" && e && "kind" in e ? e : { kind: "internal", message: String(e) });
    } finally {
      setSyncing(false);
    }
  };

  const filtered = filter.trim()
    ? rels.filter(r =>
        r.username.toLowerCase().includes(filter.toLowerCase()) ||
        (r.fullName ?? "").toLowerCase().includes(filter.toLowerCase()))
    : rels;

  return (
    <div className="main">
      <header>
        <div>
          <h2>@{session.username}</h2>
          {session.lastSyncAt && <small>Last synced {new Date(session.lastSyncAt).toLocaleString()}</small>}
        </div>
        <button onClick={doSync} disabled={syncing}>{syncing ? "Syncing…" : "Sync"}</button>
      </header>
      <DiffBanner diff={diff} />
      {error && (
        <div className={`banner error ${error.kind}`}>
          {error.kind === "session_expired" && "Please log in again."}
          {error.kind === "rate_limited"    && "Instagram is rate-limiting — try again later."}
          {error.kind !== "session_expired" && error.kind !== "rate_limited" && error.message}
        </div>
      )}
      <input className="filter" placeholder="Filter by username or name"
             value={filter} onChange={(e) => setFilter(e.target.value)} />
      {rels.length === 0 ? <StatusEmpty /> : (
        <table>
          <thead>
            <tr><th></th><th>Username</th><th>Name</th><th>Follows you</th><th>You follow them</th><th>Status</th></tr>
          </thead>
          <tbody>
            {filtered.map(r => <RelationshipRow key={r.igUserId} r={r} />)}
          </tbody>
        </table>
      )}
    </div>
  );
}
```

- [ ] **Step 6.10: `src/styles.css` — minimal layout**

```css
* { box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", sans-serif;
       margin: 0; background: #fafafa; color: #111; }
.center { display: grid; place-items: center; height: 100vh; }
.login { max-width: 480px; margin: 6rem auto; padding: 2rem;
         background: #fff; border-radius: 12px; box-shadow: 0 2px 12px rgba(0,0,0,0.06); }
.login h1 { margin-top: 0; }
.login input { display: block; width: 100%; padding: 0.5rem 0.75rem; margin: 0.5rem 0 1rem;
               border: 1px solid #ccc; border-radius: 6px; }
.login .hint { color: #555; font-size: 0.9rem; }
.error { color: #b00; }
button { padding: 0.5rem 1rem; border: 1px solid #ccc; background: #fff; border-radius: 6px; cursor: pointer; }
button:disabled { opacity: 0.5; cursor: not-allowed; }
.main { max-width: 1000px; margin: 1.5rem auto; padding: 0 1rem; }
.main header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 1rem; }
.banner { padding: 0.75rem 1rem; background: #fff5e6; border-radius: 6px; margin-bottom: 1rem; }
.banner.muted { background: #f0f0f0; color: #555; }
.banner.error { background: #fde2e2; color: #8a1f1f; }
.empty { text-align: center; padding: 3rem; color: #666; }
.filter { width: 100%; padding: 0.5rem; margin-bottom: 0.75rem; border: 1px solid #ccc; border-radius: 6px; }
table { width: 100%; border-collapse: collapse; background: #fff; border-radius: 8px; overflow: hidden; }
th, td { text-align: left; padding: 0.5rem 0.75rem; border-bottom: 1px solid #eee; }
tr:hover { background: #f7f7f7; }
.avatar-placeholder { display: inline-block; width: 32px; height: 32px; border-radius: 50%; background: #ddd; }
.tag { display: inline-block; padding: 2px 8px; border-radius: 10px; font-size: 0.8rem; }
.tag-mutual { background: #e0f5e9; color: #1c7a3c; }
.tag-fan    { background: #e4ecff; color: #1f3d8a; }
.tag-ghost  { background: #fde2e2; color: #8a1f1f; }
```

- [ ] **Step 6.11: `npm run build` + `cargo check`**

Run:
```bash
npm run build
cd src-tauri && cargo check
```
Expected: both green.

- [ ] **Step 6.12: Commit**

```bash
git add src/ package.json package-lock.json
git commit -m "feat(ui): login view + main view with diff banner and table"
git push origin main
```

---

## Task 7: Session-expired + rate-limited UX polish

The MainView already shows both banners based on `error.kind` (Step 6.9). This task adds the auto-flip back to LoginView on `session_expired` and guards against stale state.

**Files:**
- Modify: `src/App.tsx` (expose a `logout()` callback; `MainView` calls it on session_expired)
- Modify: `src/views/MainView.tsx` (accept `onSessionExpired` prop)
- Modify: `src-tauri/src/commands.rs` (reload the ig webview on session expiry via a dedicated command)

- [ ] **Step 7.1: Add `reload_ig_login` command**

Append to `src-tauri/src/commands.rs`:
```rust
#[tauri::command]
pub async fn reload_ig_login(app: AppHandle) -> AppResult<()> {
    // Close any stale ig window, then open a fresh one at the login page.
    if let Some(w) = app.get_webview_window("ig") { w.close().ok(); }
    open_ig_login(app).await
}
```
Register in `tauri::generate_handler![... reload_ig_login ...]` in `lib.rs`.

- [ ] **Step 7.2: Add wrapper in `src/lib/tauri.ts`**

```ts
reloadIgLogin: () => invoke<void>("reload_ig_login"),
```

- [ ] **Step 7.3: Wire auto-flip in `src/App.tsx`**

```tsx
const handleSessionExpired = async () => {
  await api.reloadIgLogin();
  setSession({ loggedIn: false, username: null, lastSyncAt: null });
};
// ...
return <MainView session={session} onSessionExpired={handleSessionExpired} />;
```

And in `MainView.tsx`:
```tsx
export default function MainView({ session, onSessionExpired }:
  { session: SessionState; onSessionExpired: () => void }) {
  // ... in catch branch:
  if (error?.kind === "session_expired") onSessionExpired();
}
```
Place the `onSessionExpired()` call inside `useEffect(() => { if (error?.kind === "session_expired") onSessionExpired(); }, [error])` so it runs once after render.

- [ ] **Step 7.4: Build + cargo check**

```bash
npm run build
cd src-tauri && cargo check
```
Expected: green.

- [ ] **Step 7.5: Commit**

```bash
git add src/App.tsx src/views/MainView.tsx src/lib/tauri.ts src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(ui): session-expired and rate-limited banners"
git push origin main
```

---

## Task 8: README

**Files:**
- Create: `README.md`

- [ ] **Step 8.1: Write `README.md`**

Include these sections (keep it concise):
- What it is (personal IG follower tracker, macOS only, read-only)
- Prereqs (Rust ≥ 1.77, Node ≥ 20, Xcode CLT)
- Dev: `npm install`, `cargo tauri dev`
- Build: `cargo tauri build --target universal-apple-darwin`
- First-launch gesture: **right-click the `.app` → Open → Open** (because it's unsigned). Screenshot-describable in words.
- Data location: `~/Library/Application Support/com.followerswatcher.app/data.db`
- What is NOT included: no unfollow, no notifications, no auto-sync, no cloud

- [ ] **Step 8.2: Commit**

```bash
git add README.md
git commit -m "docs: readme with dev, build, and first-launch instructions"
git push origin main
```

---

## Task 9: Universal macOS bundle config

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `README.md` (already has the universal command from Task 8; just confirm)

- [ ] **Step 9.1: Add `macOS.targets` in `tauri.conf.json`**

Under `bundle`:
```json
"bundle": {
  "active": true,
  "targets": "app",
  "icon": ["icons/icon.icns"],
  "macOS": {
    "minimumSystemVersion": "11.0",
    "dmg": { "background": null }
  }
}
```
The universal target is selected via the CLI flag, not the config. Confirm the README shows:
```
rustup target add x86_64-apple-darwin aarch64-apple-darwin
cargo tauri build --target universal-apple-darwin
```

- [ ] **Step 9.2: Install cross targets (no build yet — that's Task 10's manual verification)**

Run: `rustup target add x86_64-apple-darwin aarch64-apple-darwin`
Expected: both installed (or already up-to-date).

- [ ] **Step 9.3: Commit**

```bash
git add src-tauri/tauri.conf.json README.md
git commit -m "build: universal macos bundle config"
git push origin main
```

---

## Task 10: Manual verification (the gate before declaring done)

No commits in this task unless a bug is found.

- [ ] **Step 10.1: Start the dev app**

Run: `cd /Users/denistaranenko/Work/friends-watcher && cargo tauri dev`
Expected: window opens showing the LoginView.

- [ ] **Step 10.2: Log in to a TEST Instagram account**

NOT the girlfriend's or any high-value account.
Click "Open Instagram login" → the ig webview window opens → log in on Instagram's actual page → IG redirects you to your profile.

- [ ] **Step 10.3: Confirm handle**

Back in the main window, type the test account's handle → Confirm.
Expected: the view flips to MainView. `SessionState.loggedIn = true`, `username` is set.

- [ ] **Step 10.4: Click Sync**

Watch the Rust console (`cargo tauri dev` terminal):
- Paginated GETs to `/api/v1/friendships/<id>/followers/` and `.../following/`.
- No 429s. No `feedback_required`. Each page ~1.5s apart.

Expected on completion: the table fills; the DiffBanner shows nothing since this is the first snapshot.

- [ ] **Step 10.5: Unfollow one user manually from the IG test account**

In a real browser, log into the test account and unfollow one user. (Do not use the app.)
Back in the app, click Sync again.
Expected: DiffBanner shows "1 unfollowed you since <date>". That user is in `diff.lost_followers`.

- [ ] **Step 10.6: Click a row → OS default browser opens**

Before clicking, change the default browser in System Settings → Desktop & Dock → Default web browser to **Chrome** or **Arc**. Then click any row.
Expected: the profile opens in the non-Safari default browser. If it opens Safari, `tauri-plugin-opener` config is wrong — do NOT ship.

- [ ] **Step 10.7: Simulate session expiry**

In the ig login window, open devtools (Cmd+Opt+I), go to Application → Cookies → instagram.com → delete `sessionid`. Close devtools.
In the main window, click Sync.
Expected: the error banner reads "Please log in again" and the main window switches back to LoginView; the ig webview reloads to the login page.

- [ ] **Step 10.8: Verify SQLite file**

Run: `ls -la "$HOME/Library/Application Support/com.followerswatcher.app/"`
Expected: `data.db` present. Inspect:
```bash
sqlite3 "$HOME/Library/Application Support/com.followerswatcher.app/data.db" "SELECT id, taken_at, followers_count, following_count FROM snapshots;"
```
Expected: at least 2 rows (one before and one after the manual unfollow).

- [ ] **Step 10.9: Universal release build**

Run: `cargo tauri build --target universal-apple-darwin`
Expected: `src-tauri/target/universal-apple-darwin/release/bundle/macos/followers-watcher.app` exists.

- [ ] **Step 10.10: First-launch gesture**

`open src-tauri/target/universal-apple-darwin/release/bundle/macos/followers-watcher.app` — Gatekeeper blocks.
Right-click the `.app` in Finder → **Open** → confirm.
Expected: app launches. Subsequent double-clicks work.

- [ ] **Step 10.11: Push final state and confirm clean tree**

```bash
git status          # should be clean
git log --oneline   # should show 9 commits matching the prompt's sequence
git push origin main
```

---

## Git failure fallback

If any git operation in this plan fails (e.g., `gh repo create` hits a network/auth error, `git push` is rejected, a commit is blocked by a hook, or the working tree is unexpectedly dirty), **do not stop progress on the code**. Skip the failing git step, note it in a `GIT-TODO.md` scratchpad at the project root with the exact command and error, and continue to the next coding task. The commits and pushes can be reconciled in a single batch at the end of the session or deferred for the user to resolve manually. Never use destructive git commands (`--force`, `reset --hard`, `--no-verify`) to work around a failure.

## Risk register (read before starting)

- **Cookie availability timing:** `cookies_for_url` on the ig window may return empty immediately after the window opens — poll for `sessionid` rather than assuming it's there on first read. The `App.tsx` 2s polling in Task 6 handles this.
- **WKWebView cookie sharing:** Tauri 2 on macOS uses a shared cookie store across webviews in the same process by default. If the `ig` webview's cookies don't appear on `main` (or vice versa), harvest from whichever window actually loaded `instagram.com`. The `sync_now` command tries `ig` first, then `main` — good enough.
- **IG responses that drift from spec:** if `pk` comes back as an integer instead of a string, change `ig_user_id: String` to use `#[serde(with = "serde_with::DisplayFromStr")]` or a `StringOrNumber` helper. Surface this as a stop-and-report condition (per the prompt).
- **Backoff test runtime:** if the rate-limited test takes too long, the configurable-backoff change in Step 3.3 is essential. Don't mark it `#[ignore]` — fix it.
- **macOS minimum version:** `11.0` matches Tauri 2's floor. Older Macs won't run this.
- **gh auth scope:** `gh repo create --private --source=. --remote=origin --push` requires `repo` scope. The verified auth shows `repo` — good.

---

## Self-review checklist (run after completing all tasks)

- [ ] Every commit in the prompt's 9-commit sequence exists on `origin/main`.
- [ ] `cargo check` succeeds on `HEAD`; each intermediate commit is buildable.
- [ ] `cargo test` runs the db (3) + instagram (4) tests green.
- [ ] No `target/`, `dist/`, `node_modules/`, `.DS_Store` tracked.
- [ ] No session cookies, sessionid, csrftoken, or personal IG data anywhere in the repo (grep: `git grep -E "sessionid|csrftoken|ds_user_id" -- . ':!PLAN.md' ':!prompt.md'` → expect only code references, no real values).
- [ ] `open_profile` opens the OS default browser (verified by changing it to Chrome/Arc).
- [ ] First-launch right-click → Open gesture is documented in README.
- [ ] Universal `.app` bundle exists and runs from a fresh Finder launch.
