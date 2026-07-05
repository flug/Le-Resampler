use crate::db::{DbPool, SampleRecord};
use crate::metadata;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

const AUDIO_EXTENSIONS: &[&str] = &["wav", "aif", "aiff", "flac", "mp3", "ogg", "opus"];

#[derive(serde::Serialize, Clone)]
pub struct ScanProgress {
    pub current: usize,
    pub total: usize,
    pub filename: String,
}

#[derive(serde::Serialize, Clone)]
pub struct ScanComplete {
    pub added: usize,
    pub skipped: usize,
}

#[cfg_attr(tarpaulin, coverage(off))]
pub fn scan_folder_sync(path: &str, app: &AppHandle, db: &Arc<DbPool>) -> Result<usize, String> {
    log::info!("Starting scan of: {}", path);

    let paths: Vec<PathBuf> = WalkDir::new(path)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_audio_file(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect();

    let total = paths.len();
    log::info!("Found {} audio files", total);

    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            current: 0,
            total,
            filename: String::new(),
        },
    );

    let now = chrono::Utc::now().to_rfc3339();
    let (added, skipped) = scan_entries(&paths, db, &now, |cur, tot, fname| {
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                current: cur,
                total: tot,
                filename: fname.to_string(),
            },
        );
    });

    let _ = app.emit("scan-complete", ScanComplete { added, skipped });
    log::info!("Scan complete: {} added, {} skipped", added, skipped);
    Ok(added)
}

pub(crate) fn scan_entries(
    paths: &[PathBuf],
    db: &Arc<DbPool>,
    date_added: &str,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> (usize, usize) {
    let total = paths.len();
    let mut added = 0usize;
    let mut skipped = 0usize;

    for (i, file_path) in paths.iter().enumerate() {
        let filename = file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        if i % 20 == 0 || i == total.saturating_sub(1) {
            on_progress(i + 1, total, &filename);
        }

        match process_file(file_path, db, date_added) {
            Ok(true) => added += 1,
            Ok(false) => skipped += 1,
            Err(e) => {
                skipped += 1;
                log::warn!("Skipping {:?}: {}", file_path, e);
            }
        }
    }

    (added, skipped)
}

pub(crate) fn process_file(
    path: &Path,
    db: &Arc<DbPool>,
    date_added: &str,
) -> Result<bool, String> {
    let meta = metadata::extract(path)?;
    let path_str = path.to_string_lossy().to_string();
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&path_str)
        .to_string();

    let record = SampleRecord {
        id: 0,
        path: path_str,
        filename,
        duration_ms: meta.duration_ms,
        bpm: meta.bpm,
        musical_key: meta.musical_key,
        category: meta.category,
        sample_type: meta.sample_type,
        date_added: date_added.to_string(),
        file_size: meta.file_size,
        tags: vec![],
    };

    db.upsert_sample(&record).map_err(|e| e.to_string())
}

pub(crate) fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DbPool;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_db() -> Arc<DbPool> {
        let db = DbPool::new_in_memory().unwrap();
        db.init_schema().unwrap();
        Arc::new(db)
    }

    fn make_wav() -> Vec<u8> {
        let data_size = 2u32; // 1 sample, 16-bit
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36u32 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&44100u32.to_le_bytes());
        v.extend_from_slice(&88200u32.to_le_bytes()); // byte rate
        v.extend_from_slice(&2u16.to_le_bytes()); // block align
        v.extend_from_slice(&16u16.to_le_bytes()); // 16 bits
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&[0u8; 2]);
        v
    }

    // --- is_audio_file ---

    #[test]
    fn is_audio_wav() {
        assert!(is_audio_file(Path::new("sample.wav")));
    }

    #[test]
    fn is_audio_aif() {
        assert!(is_audio_file(Path::new("sample.aif")));
    }

    #[test]
    fn is_audio_aiff() {
        assert!(is_audio_file(Path::new("sample.aiff")));
    }

    #[test]
    fn is_audio_flac() {
        assert!(is_audio_file(Path::new("sample.flac")));
    }

    #[test]
    fn is_audio_mp3() {
        assert!(is_audio_file(Path::new("sample.mp3")));
    }

    #[test]
    fn is_audio_ogg() {
        assert!(is_audio_file(Path::new("sample.ogg")));
    }

    #[test]
    fn is_audio_opus() {
        assert!(is_audio_file(Path::new("sample.opus")));
    }

    #[test]
    fn is_audio_uppercase_extension() {
        assert!(is_audio_file(Path::new("sample.WAV")));
    }

    #[test]
    fn is_not_audio_txt() {
        assert!(!is_audio_file(Path::new("readme.txt")));
    }

    #[test]
    fn is_not_audio_no_extension() {
        assert!(!is_audio_file(Path::new("noext")));
    }

    // --- process_file ---

    #[test]
    fn process_file_new_wav_returns_true() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        assert!(process_file(&path, &db, &now).unwrap());
    }

    #[test]
    fn process_file_duplicate_returns_false() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        process_file(&path, &db, &now).unwrap();
        assert!(!process_file(&path, &db, &now).unwrap());
    }

    #[test]
    fn process_file_nonexistent_path_inserts_gracefully() {
        let db = make_db();
        let path = Path::new("/nonexistent/path/sample.wav");
        let now = chrono::Utc::now().to_rfc3339();
        // lofty fails silently; path is still upserted with null metadata
        assert!(process_file(path, &db, &now).is_ok());
    }

    // --- scan_entries ---

    #[test]
    fn scan_entries_new_files_returns_added_count() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..2)
            .map(|i| {
                let p = dir.path().join(format!("file{}.wav", i));
                std::fs::write(&p, make_wav()).unwrap();
                p
            })
            .collect();
        let now = chrono::Utc::now().to_rfc3339();
        let (added, skipped) = scan_entries(&paths, &db, &now, |_, _, _| {});
        assert_eq!(added, 2);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn scan_entries_second_pass_returns_skipped_count() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..2)
            .map(|i| {
                let p = dir.path().join(format!("file{}.wav", i));
                std::fs::write(&p, make_wav()).unwrap();
                p
            })
            .collect();
        let now = chrono::Utc::now().to_rfc3339();
        scan_entries(&paths, &db, &now, |_, _, _| {});
        let (added, skipped) = scan_entries(&paths, &db, &now, |_, _, _| {});
        assert_eq!(added, 0);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn scan_entries_progress_callback_called_at_boundaries() {
        let db = make_db();
        let dir = tempdir().unwrap();
        // 3 files: callback fires at i=0 and i=2 (first and last)
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = dir.path().join(format!("file{}.wav", i));
                std::fs::write(&p, make_wav()).unwrap();
                p
            })
            .collect();
        let now = chrono::Utc::now().to_rfc3339();
        let mut calls: Vec<(usize, usize)> = Vec::new();
        scan_entries(&paths, &db, &now, |cur, tot, _| {
            calls.push((cur, tot));
        });
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], (1, 3));
        assert_eq!(calls[1], (3, 3));
    }
}
