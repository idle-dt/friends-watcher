use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{
    DiffResult, Relationship, RelationshipStatus, Snapshot, UserRow,
};

pub const APP_DIR_NAME: &str = "com.friendswatcher.app";
const DB_FILE_NAME: &str = "data.db";

pub fn db_path() -> std::io::Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve platform data directory",
        )
    })?;
    Ok(base.join(APP_DIR_NAME).join(DB_FILE_NAME))
}

pub fn open_db() -> rusqlite::Result<Connection> {
    let path = db_path().map_err(|e| {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
            Some(e.to_string()),
        )
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            rusqlite::Error::SqliteFailure(
                rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CANTOPEN),
                Some(e.to_string()),
            )
        })?;
    }
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
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
        "#,
    )
}

pub fn write_snapshot(
    conn: &mut Connection,
    owner_user_id: &str,
    owner_username: &str,
    followers: &[UserRow],
    following: &[UserRow],
) -> rusqlite::Result<i64> {
    let tx = conn.transaction()?;
    let taken_at = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    tx.execute(
        "INSERT INTO snapshots (taken_at, owner_user_id, owner_username, followers_count, following_count)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            taken_at,
            owner_user_id,
            owner_username,
            followers.len() as i64,
            following.len() as i64,
        ],
    )?;
    let snapshot_id = tx.last_insert_rowid();

    {
        let mut stmt = tx.prepare(
            "INSERT INTO followers (snapshot_id, ig_user_id, username, full_name, is_verified, profile_pic_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for u in followers {
            stmt.execute(params![
                snapshot_id,
                u.ig_user_id,
                u.username,
                u.full_name,
                u.is_verified as i64,
                u.profile_pic_url,
            ])?;
        }
    }

    {
        let mut stmt = tx.prepare(
            "INSERT INTO following (snapshot_id, ig_user_id, username, full_name, is_verified, profile_pic_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for u in following {
            stmt.execute(params![
                snapshot_id,
                u.ig_user_id,
                u.username,
                u.full_name,
                u.is_verified as i64,
                u.profile_pic_url,
            ])?;
        }
    }

    tx.commit()?;
    Ok(snapshot_id)
}

fn parse_taken_at(raw: &str) -> DateTime<Utc> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return dt.with_timezone(&Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return Utc.from_utc_datetime(&naive);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S") {
        return Utc.from_utc_datetime(&naive);
    }
    Utc::now()
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
    let raw_taken_at: String = row.get("taken_at")?;
    Ok(Snapshot {
        id: row.get("id")?,
        taken_at: parse_taken_at(&raw_taken_at),
        owner_user_id: row.get("owner_user_id")?,
        owner_username: row.get("owner_username")?,
        followers_count: row.get("followers_count")?,
        following_count: row.get("following_count")?,
    })
}

pub fn get_latest_snapshot(conn: &Connection) -> rusqlite::Result<Option<Snapshot>> {
    conn.query_row(
        "SELECT id, taken_at, owner_user_id, owner_username, followers_count, following_count
         FROM snapshots ORDER BY taken_at DESC, id DESC LIMIT 1",
        [],
        row_to_snapshot,
    )
    .optional()
}

pub fn get_previous_snapshot(conn: &Connection) -> rusqlite::Result<Option<Snapshot>> {
    conn.query_row(
        "SELECT id, taken_at, owner_user_id, owner_username, followers_count, following_count
         FROM snapshots ORDER BY taken_at DESC, id DESC LIMIT 1 OFFSET 1",
        [],
        row_to_snapshot,
    )
    .optional()
}

fn load_followers(conn: &Connection, snapshot_id: i64) -> rusqlite::Result<Vec<UserRow>> {
    let mut stmt = conn.prepare(
        "SELECT ig_user_id, username, full_name, is_verified, profile_pic_url
         FROM followers WHERE snapshot_id = ?1",
    )?;
    let rows = stmt.query_map(params![snapshot_id], |r| {
        Ok(UserRow {
            ig_user_id: r.get("ig_user_id")?,
            username: r.get("username")?,
            full_name: r.get("full_name")?,
            is_verified: {
                let v: i64 = r.get("is_verified")?;
                v != 0
            },
            profile_pic_url: r.get("profile_pic_url")?,
        })
    })?;
    rows.collect()
}

fn load_following(conn: &Connection, snapshot_id: i64) -> rusqlite::Result<Vec<UserRow>> {
    let mut stmt = conn.prepare(
        "SELECT ig_user_id, username, full_name, is_verified, profile_pic_url
         FROM following WHERE snapshot_id = ?1",
    )?;
    let rows = stmt.query_map(params![snapshot_id], |r| {
        Ok(UserRow {
            ig_user_id: r.get("ig_user_id")?,
            username: r.get("username")?,
            full_name: r.get("full_name")?,
            is_verified: {
                let v: i64 = r.get("is_verified")?;
                v != 0
            },
            profile_pic_url: r.get("profile_pic_url")?,
        })
    })?;
    rows.collect()
}

pub fn get_diff(
    conn: &Connection,
    current_id: i64,
    previous_id: i64,
) -> rusqlite::Result<DiffResult> {
    let current = load_followers(conn, current_id)?;
    let previous = load_followers(conn, previous_id)?;

    let current_ids: HashMap<&str, &UserRow> =
        current.iter().map(|u| (u.ig_user_id.as_str(), u)).collect();
    let previous_ids: HashMap<&str, &UserRow> =
        previous.iter().map(|u| (u.ig_user_id.as_str(), u)).collect();

    let new_followers: Vec<UserRow> = current
        .iter()
        .filter(|u| !previous_ids.contains_key(u.ig_user_id.as_str()))
        .cloned()
        .collect();

    let lost_followers: Vec<UserRow> = previous
        .iter()
        .filter(|u| !current_ids.contains_key(u.ig_user_id.as_str()))
        .cloned()
        .collect();

    let since: Option<DateTime<Utc>> = conn
        .query_row(
            "SELECT taken_at FROM snapshots WHERE id = ?1",
            params![previous_id],
            |r| {
                let raw: String = r.get(0)?;
                Ok(parse_taken_at(&raw))
            },
        )
        .optional()?;

    Ok(DiffResult {
        since,
        new_followers,
        lost_followers,
    })
}

pub fn get_relationships(
    conn: &Connection,
    snapshot_id: i64,
) -> rusqlite::Result<Vec<Relationship>> {
    let followers = load_followers(conn, snapshot_id)?;
    let following = load_following(conn, snapshot_id)?;

    let follower_map: HashMap<String, UserRow> = followers
        .into_iter()
        .map(|u| (u.ig_user_id.clone(), u))
        .collect();
    let following_map: HashMap<String, UserRow> = following
        .into_iter()
        .map(|u| (u.ig_user_id.clone(), u))
        .collect();

    let mut all_ids: Vec<String> = follower_map
        .keys()
        .chain(following_map.keys())
        .cloned()
        .collect();
    all_ids.sort();
    all_ids.dedup();

    let mut out = Vec::with_capacity(all_ids.len());
    for id in all_ids {
        let follows_you = follower_map.contains_key(&id);
        let you_follow = following_map.contains_key(&id);
        let status = match (follows_you, you_follow) {
            (true, true) => RelationshipStatus::Mutual,
            (true, false) => RelationshipStatus::Fan,
            (false, true) => RelationshipStatus::Ghost,
            (false, false) => unreachable!(),
        };
        let source = follower_map
            .get(&id)
            .or_else(|| following_map.get(&id))
            .expect("id came from one of the two maps");
        out.push(Relationship {
            ig_user_id: source.ig_user_id.clone(),
            username: source.username.clone(),
            full_name: source.full_name.clone(),
            is_verified: source.is_verified,
            profile_pic_url: source.profile_pic_url.clone(),
            follows_you,
            you_follow,
            status,
        });
    }

    out.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(id: &str, name: &str) -> UserRow {
        UserRow {
            ig_user_id: id.to_string(),
            username: name.to_string(),
            full_name: Some(format!("Full {name}")),
            is_verified: false,
            profile_pic_url: Some(format!("https://ex/{name}.jpg")),
        }
    }

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn init_schema_is_idempotent() {
        let conn = fresh_db();
        init_schema(&conn).unwrap();
        init_schema(&conn).unwrap();
    }

    #[test]
    fn write_and_read_snapshot_roundtrip() {
        let mut conn = fresh_db();
        let followers = vec![u("1", "alice"), u("2", "bob")];
        let following = vec![u("2", "bob"), u("3", "carol")];

        let id = write_snapshot(&mut conn, "owner-1", "me", &followers, &following).unwrap();
        assert!(id > 0);

        let latest = get_latest_snapshot(&conn).unwrap().expect("has latest");
        assert_eq!(latest.id, id);
        assert_eq!(latest.owner_user_id, "owner-1");
        assert_eq!(latest.owner_username, "me");
        assert_eq!(latest.followers_count, 2);
        assert_eq!(latest.following_count, 2);
    }

    #[test]
    fn diff_reports_new_and_lost_followers() {
        let mut conn = fresh_db();
        let prev_followers = vec![u("1", "alice"), u("2", "bob")];
        let prev_following = vec![u("1", "alice")];
        let prev_id =
            write_snapshot(&mut conn, "owner-1", "me", &prev_followers, &prev_following).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let cur_followers = vec![u("1", "alice"), u("3", "carol")];
        let cur_following = vec![u("1", "alice")];
        let cur_id =
            write_snapshot(&mut conn, "owner-1", "me", &cur_followers, &cur_following).unwrap();

        let diff = get_diff(&conn, cur_id, prev_id).unwrap();
        let new_ids: Vec<_> = diff.new_followers.iter().map(|u| u.ig_user_id.as_str()).collect();
        let lost_ids: Vec<_> = diff
            .lost_followers
            .iter()
            .map(|u| u.ig_user_id.as_str())
            .collect();
        assert_eq!(new_ids, vec!["3"]);
        assert_eq!(lost_ids, vec!["2"]);
        assert!(diff.since.is_some());
    }

    #[test]
    fn relationships_classify_mutual_fan_ghost() {
        let mut conn = fresh_db();
        let followers = vec![u("1", "mutual"), u("2", "fan_only")];
        let following = vec![u("1", "mutual"), u("3", "ghost_only")];
        let id = write_snapshot(&mut conn, "owner-1", "me", &followers, &following).unwrap();

        let rels = get_relationships(&conn, id).unwrap();
        let by_id: HashMap<_, _> = rels.iter().map(|r| (r.ig_user_id.clone(), r)).collect();
        assert_eq!(by_id["1"].status, RelationshipStatus::Mutual);
        assert_eq!(by_id["2"].status, RelationshipStatus::Fan);
        assert_eq!(by_id["3"].status, RelationshipStatus::Ghost);
        assert!(by_id["1"].follows_you && by_id["1"].you_follow);
        assert!(by_id["2"].follows_you && !by_id["2"].you_follow);
        assert!(!by_id["3"].follows_you && by_id["3"].you_follow);
    }

    #[test]
    fn previous_snapshot_returns_second_most_recent() {
        let mut conn = fresh_db();
        let _id1 = write_snapshot(&mut conn, "o", "me", &[], &[]).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let id2 = write_snapshot(&mut conn, "o", "me", &[u("1", "a")], &[]).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let id3 = write_snapshot(&mut conn, "o", "me", &[u("1", "a"), u("2", "b")], &[]).unwrap();

        let latest = get_latest_snapshot(&conn).unwrap().unwrap();
        let previous = get_previous_snapshot(&conn).unwrap().unwrap();
        assert_eq!(latest.id, id3);
        assert_eq!(previous.id, id2);
    }

    #[test]
    fn no_snapshots_returns_none() {
        let conn = fresh_db();
        assert!(get_latest_snapshot(&conn).unwrap().is_none());
        assert!(get_previous_snapshot(&conn).unwrap().is_none());
    }
}
