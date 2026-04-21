mod commands;
mod cookies;
mod db;
mod error;
mod instagram;
mod models;

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
      match db::open_db().and_then(|conn| {
        db::init_schema(&conn)?;
        Ok(conn)
      }) {
        Ok(_conn) => {
          log::info!("sqlite schema ready");
        }
        Err(e) => {
          log::error!("failed to initialize sqlite: {e}");
          return Err(Box::new(e) as Box<dyn std::error::Error>);
        }
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
