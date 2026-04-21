use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRow {
    pub ig_user_id: String,
    pub username: String,
    pub full_name: Option<String>,
    pub is_verified: bool,
    pub profile_pic_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub taken_at: DateTime<Utc>,
    pub owner_user_id: String,
    pub owner_username: String,
    pub followers_count: i64,
    pub following_count: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RelationshipStatus {
    Mutual,
    Fan,
    Ghost,
    New,
    Lost,
}

impl RelationshipStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationshipStatus::Mutual => "mutual",
            RelationshipStatus::Fan => "fan",
            RelationshipStatus::Ghost => "ghost",
            RelationshipStatus::New => "new",
            RelationshipStatus::Lost => "lost",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub ig_user_id: String,
    pub username: String,
    pub full_name: Option<String>,
    pub is_verified: bool,
    pub profile_pic_url: Option<String>,
    pub follows_you: bool,
    pub you_follow: bool,
    pub status: RelationshipStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffResult {
    pub since: Option<DateTime<Utc>>,
    pub new_followers: Vec<UserRow>,
    pub lost_followers: Vec<UserRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub new_followers: Vec<UserRow>,
    pub lost_followers: Vec<UserRow>,
    pub total_followers: i64,
    pub total_following: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub logged_in: bool,
    pub username: Option<String>,
    pub last_sync_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnProfile {
    pub id: String,
    pub username: String,
    pub full_name: Option<String>,
    pub followers_count: i64,
    pub following_count: i64,
}
