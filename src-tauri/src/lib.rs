#![cfg_attr(tarpaulin, feature(coverage_attribute))]
mod audio;
mod commands;
mod db;
mod export;
mod metadata;
mod scanner;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::Manager;

pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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

            // Menu natif
            let about_item = MenuItem::with_id(
                app,
                "about",
                "About Le Resampler",
                true,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let buymecoffee = MenuItem::with_id(
                app,
                "buymecoffee",
                "☕ Support development",
                true,
                None::<&str>,
            )?;

            let help_menu = Submenu::with_items(
                app,
                "Help",
                true,
                &[&about_item, &separator, &buymecoffee],
            )?;

            let menu = Menu::with_items(app, &[&help_menu])?;
            app.set_menu(menu)?;

            app.on_menu_event(|app, event| match event.id().as_ref() {
                "about" => {
                    tauri::WebviewWindowBuilder::new(
                        app,
                        "about",
                        tauri::WebviewUrl::App("about.html".into()),
                    )
                    .title("About")
                    .inner_size(400.0, 300.0)
                    .resizable(false)
                    .center()
                    .build()
                    .ok();
                }
                "buymecoffee" => {
                    tauri_plugin_opener::open_url(
                        "https://buymeacoffee.com/flugv1t",
                        None::<&str>,
                    )
                    .ok();
                }
                _ => {}
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::scan_folder,
            commands::get_watched_folders,
            commands::list_samples,
            commands::update_sample_metadata,
            commands::add_tag_to_sample,
            commands::remove_tag_from_sample,
            commands::get_all_tags,
            commands::preview_sample,
            commands::stop_preview,
            commands::set_volume,
            commands::pick_folder,
            commands::open_url,
            commands::copy_samples_to,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Le Resampler");
}
