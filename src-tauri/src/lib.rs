#![cfg_attr(tarpaulin, feature(coverage_attribute))]
mod audio;
mod commands;
mod db;
mod export;
mod metadata;
mod scanner;

use std::sync::Arc;
use tauri::Manager;

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Cannot find data dir: {}", e))?;
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join("samples.db");
            log::info!("Database path: {:?}", db_path);

            let db = db::DbPool::new(&db_path).map_err(|e| format!("DB init failed: {}", e))?;
            db.init_schema()
                .map_err(|e| format!("Schema init failed: {}", e))?;
            app.manage(Arc::new(db));

            let player =
                audio::AudioPlayer::new().map_err(|e| format!("Audio init failed: {}", e))?;
            app.manage(Arc::new(player));

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_folder,
            commands::list_samples,
            commands::update_sample_metadata,
            commands::add_tag_to_sample,
            commands::remove_tag_from_sample,
            commands::get_all_tags,
            commands::preview_sample,
            commands::stop_preview,
            commands::pick_folder,
            commands::copy_samples_to,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Sample Manager");
}
