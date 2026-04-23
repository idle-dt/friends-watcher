mod avatars;
mod commands;
mod cookies;
mod db;
mod error;
mod instagram;
mod models;

use tauri::Manager;

use crate::avatars::AvatarHttp;
use crate::commands::DbState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(tauri_plugin_opener::init())
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      let conn = db::open_db()?;
      db::init_schema(&conn)?;
      log::info!("sqlite schema ready");
      app.manage(DbState::new(conn));
      app.manage(AvatarHttp::new()?);
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::get_session_state,
      commands::sync_now,
      commands::get_latest_relationships,
      commands::get_diff_since_previous,
      commands::open_profile,
      commands::start_ig_login,
      commands::get_avatar,
    ])
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
