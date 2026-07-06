use crate::audio::AudioPlayer;
use crate::db::{DbPool, SampleRecord};
use crate::scanner;
use std::sync::Arc;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    use tokio::sync::oneshot;

    let (tx, rx) = oneshot::channel();

    app.dialog().file().pick_folder(move |folder_path| {
        let path = folder_path.map(|p| p.to_string());
        let _ = tx.send(path);
    });

    rx.await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn scan_folder(
    path: String,
    app: AppHandle,
    db: State<'_, Arc<DbPool>>,
) -> Result<usize, String> {
    let db = Arc::clone(&*db);
    db.add_folder(&path).map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move || scanner::scan_folder_sync(&path, &app, &db))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_watched_folders(db: State<'_, Arc<DbPool>>) -> Result<Vec<String>, String> {
    db.get_folders().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_samples(
    filter_category: Option<String>,
    filter_tag: Option<String>,
    search: Option<String>,
    db: State<'_, Arc<DbPool>>,
) -> Result<Vec<SampleRecord>, String> {
    db.list_samples(
        filter_category.as_deref(),
        filter_tag.as_deref(),
        search.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn update_sample_metadata(
    id: i64,
    category: Option<String>,
    bpm: Option<f64>,
    musical_key: Option<String>,
    db: State<'_, Arc<DbPool>>,
) -> Result<(), String> {
    db.update_sample(id, category.as_deref(), bpm, musical_key.as_deref())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_tag_to_sample(
    sample_id: i64,
    tag_name: String,
    db: State<'_, Arc<DbPool>>,
) -> Result<(), String> {
    db.add_tag(sample_id, &tag_name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_tag_from_sample(
    sample_id: i64,
    tag_name: String,
    db: State<'_, Arc<DbPool>>,
) -> Result<(), String> {
    db.remove_tag(sample_id, &tag_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_all_tags(db: State<'_, Arc<DbPool>>) -> Result<Vec<String>, String> {
    db.get_all_tags().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_sample(
    path: String,
    player: State<'_, Arc<AudioPlayer>>,
) -> Result<(), String> {
    player.play(path)
}

#[tauri::command]
pub async fn stop_preview(player: State<'_, Arc<AudioPlayer>>) -> Result<(), String> {
    player.stop()
}

#[tauri::command]
pub async fn set_volume(volume: f32, player: State<'_, Arc<AudioPlayer>>) -> Result<(), String> {
    player.set_volume(volume)
}

#[tauri::command]
pub async fn open_url(url: String, app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn copy_samples_to(
    dest: String,
    samples: Vec<crate::export::ExportEntry>,
    template: String,
) -> Result<crate::export::ExportResult, String> {
    tokio::task::spawn_blocking(move || crate::export::copy_samples(&dest, &samples, &template))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_setting(
    key: String,
    db: State<'_, Arc<DbPool>>,
) -> Result<Option<String>, String> {
    db.get_setting(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_setting(
    key: String,
    value: String,
    db: State<'_, Arc<DbPool>>,
) -> Result<(), String> {
    db.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_settings_window(app: AppHandle) -> Result<(), String> {
    tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Settings")
    .inner_size(460.0, 460.0)
    .resizable(false)
    .center()
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}
