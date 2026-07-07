use crate::db::{DbPool, SampleRecord};
use crate::metadata::{self, KeySource};
use crate::pitch;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use walkdir::WalkDir;

#[derive(Clone, Copy, PartialEq)]
pub enum KeyDetection {
    Off,
    /// YIN pitch detection — fast, root note only, one-shots only.
    Yin,
    /// Krumhansl-Schmuckler — harmonic analysis, major/minor, any sample type.
    Ks,
}

#[derive(Clone, Copy)]
pub struct ScanOptions {
    pub key_detection: KeyDetection,
    pub write_key_to_metadata: bool,
}

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

    let key_detection = db
        .get_setting("key_detection")
        .unwrap_or(None)
        .map(|v| match v.as_str() {
            "yin" => KeyDetection::Yin,
            "ks" => KeyDetection::Ks,
            _ => KeyDetection::Off,
        })
        .unwrap_or(KeyDetection::Off);
    let write_key_to_metadata = db
        .get_setting("write_key_to_metadata")
        .unwrap_or(None)
        .map(|v| v == "true")
        .unwrap_or(false);
    let opts = ScanOptions {
        key_detection,
        write_key_to_metadata,
    };

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
    let (added, skipped) = scan_entries(&paths, db, &now, opts, |cur, tot, fname| {
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
    opts: ScanOptions,
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

        match process_file(file_path, db, date_added, opts) {
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
    opts: ScanOptions,
) -> Result<bool, String> {
    let mut meta = metadata::extract(path)?;

    // Audio key detection when no key was found in filename or embedded metadata
    if meta.musical_key.is_none() {
        match opts.key_detection {
            KeyDetection::Off => {}
            KeyDetection::Yin => {
                // YIN: monophonic pitch detection, one-shots only
                if meta.sample_type.as_deref() == Some("one-shot") {
                    if let Some(key) = pitch::detect_key_yin(path) {
                        meta.musical_key = Some(key);
                        meta.key_source = Some(KeySource::AudioPitch);
                    }
                }
            }
            KeyDetection::Ks => {
                // K-S: harmonic analysis, works on any sample type
                if let Some(key) = pitch::detect_key_ks(path) {
                    meta.musical_key = Some(key);
                    meta.key_source = Some(KeySource::AudioPitch);
                }
            }
        }
    }

    // Write detected/inferred key back to the file's embedded metadata
    if opts.write_key_to_metadata {
        if let Some(key) = &meta.musical_key {
            if let Err(e) = metadata::write_key_to_file(path, key) {
                log::warn!("Failed to write key to {:?}: {}", path, e);
            }
        }
    }

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
    use std::f32::consts::PI;
    use std::sync::Arc;
    use tempfile::tempdir;

    fn make_db() -> Arc<DbPool> {
        let db = DbPool::new_in_memory().unwrap();
        db.init_schema().unwrap();
        Arc::new(db)
    }

    fn no_opts() -> ScanOptions {
        ScanOptions {
            key_detection: KeyDetection::Off,
            write_key_to_metadata: false,
        }
    }

    fn make_wav() -> Vec<u8> {
        let data_size = 2u32;
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36u32 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&44100u32.to_le_bytes());
        v.extend_from_slice(&88200u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&[0u8; 2]);
        v
    }

    fn make_sine_wav(freq: f32, sample_rate: u32, duration_ms: u32) -> Vec<u8> {
        let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as u32;
        let data_size = num_samples * 2;
        let mut pcm = Vec::new();
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let s = ((2.0 * PI * freq * t).sin() * 32767.0) as i16;
            pcm.extend_from_slice(&s.to_le_bytes());
        }
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&pcm);
        v
    }

    /// WAV with a chord (sum of sine waves), mono 16-bit PCM.
    fn make_chord_wav(freqs: &[f32], sample_rate: u32, duration_ms: u32) -> Vec<u8> {
        let num_samples = (sample_rate as f32 * duration_ms as f32 / 1000.0) as u32;
        let data_size = num_samples * 2;
        let mut pcm = Vec::new();
        for i in 0..num_samples {
            let t = i as f32 / sample_rate as f32;
            let s: f32 =
                freqs.iter().map(|&f| (2.0 * PI * f * t).sin()).sum::<f32>() / freqs.len() as f32;
            pcm.extend_from_slice(&((s * 32767.0) as i16).to_le_bytes());
        }
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend_from_slice(&pcm);
        v
    }

    fn make_tagged_wav() -> Vec<u8> {
        let id3: &[u8] = &[b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 0];
        let id3_size = id3.len() as u32;
        let pcm_size = 2u32;
        let riff_size = 4 + (8 + id3_size) + 24 + (8 + pcm_size);
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&riff_size.to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"id3 ");
        v.extend_from_slice(&id3_size.to_le_bytes());
        v.extend_from_slice(id3);
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&44100u32.to_le_bytes());
        v.extend_from_slice(&88200u32.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&pcm_size.to_le_bytes());
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
        assert!(process_file(&path, &db, &now, no_opts()).unwrap());
    }

    #[test]
    fn process_file_duplicate_returns_false() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        process_file(&path, &db, &now, no_opts()).unwrap();
        assert!(!process_file(&path, &db, &now, no_opts()).unwrap());
    }

    #[test]
    fn process_file_nonexistent_path_inserts_gracefully() {
        let db = make_db();
        let path = Path::new("/nonexistent/path/sample.wav");
        let now = chrono::Utc::now().to_rfc3339();
        assert!(process_file(path, &db, &now, no_opts()).is_ok());
    }

    #[test]
    fn process_file_yin_silent_one_shot_no_key() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Yin,
            write_key_to_metadata: false,
        };
        assert!(process_file(&path, &db, &now, opts).unwrap());
        let samples = db.list_samples(None, None, None).unwrap();
        assert!(samples[0].musical_key.is_none());
    }

    #[test]
    fn process_file_yin_tonal_one_shot_detects_key() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("a440.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500)).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Yin,
            write_key_to_metadata: false,
        };
        process_file(&path, &db, &now, opts).unwrap();
        let samples = db.list_samples(None, None, None).unwrap();
        assert_eq!(samples[0].musical_key.as_deref(), Some("A"));
    }

    #[test]
    fn process_file_yin_skipped_when_key_already_known() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("pad_Cm_warm.wav");
        std::fs::write(&path, make_sine_wav(440.0, 44100, 500)).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Yin,
            write_key_to_metadata: false,
        };
        process_file(&path, &db, &now, opts).unwrap();
        let samples = db.list_samples(None, None, None).unwrap();
        assert_eq!(samples[0].musical_key.as_deref(), Some("Cm"));
    }

    #[test]
    fn process_file_ks_tonal_detects_key() {
        let db = make_db();
        let dir = tempdir().unwrap();
        // C major chord (C4+E4+G4), 2 s — enough for K-S analysis
        let path = dir.path().join("c_major.wav");
        std::fs::write(&path, make_chord_wav(&[261.63, 329.63, 392.0], 44100, 2000)).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Ks,
            write_key_to_metadata: false,
        };
        process_file(&path, &db, &now, opts).unwrap();
        let samples = db.list_samples(None, None, None).unwrap();
        assert_eq!(samples[0].musical_key.as_deref(), Some("C"));
    }

    #[test]
    fn process_file_ks_silent_no_key() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("silent.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Ks,
            write_key_to_metadata: false,
        };
        process_file(&path, &db, &now, opts).unwrap();
        let samples = db.list_samples(None, None, None).unwrap();
        assert!(samples[0].musical_key.is_none());
    }

    #[test]
    fn process_file_write_key_to_metadata_no_key_skips() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("kick_128bpm.wav");
        std::fs::write(&path, make_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Off,
            write_key_to_metadata: true,
        };
        assert!(process_file(&path, &db, &now, opts).is_ok());
    }

    #[test]
    fn process_file_write_key_to_metadata_success() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("pad_Am_loop.wav");
        std::fs::write(&path, make_tagged_wav()).unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Off,
            write_key_to_metadata: true,
        };
        assert!(process_file(&path, &db, &now, opts).is_ok());
    }

    #[test]
    fn process_file_write_key_to_metadata_failure_warns_and_continues() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let path = dir.path().join("Am_pad.bin");
        std::fs::write(&path, b"garbage content").unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let opts = ScanOptions {
            key_detection: KeyDetection::Off,
            write_key_to_metadata: true,
        };
        assert!(process_file(&path, &db, &now, opts).is_ok());
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
        let (added, skipped) = scan_entries(&paths, &db, &now, no_opts(), |_, _, _| {});
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
        scan_entries(&paths, &db, &now, no_opts(), |_, _, _| {});
        let (added, skipped) = scan_entries(&paths, &db, &now, no_opts(), |_, _, _| {});
        assert_eq!(added, 0);
        assert_eq!(skipped, 2);
    }

    #[test]
    fn scan_entries_progress_callback_called_at_boundaries() {
        let db = make_db();
        let dir = tempdir().unwrap();
        let paths: Vec<PathBuf> = (0..3)
            .map(|i| {
                let p = dir.path().join(format!("file{}.wav", i));
                std::fs::write(&p, make_wav()).unwrap();
                p
            })
            .collect();
        let now = chrono::Utc::now().to_rfc3339();
        let mut calls: Vec<(usize, usize)> = Vec::new();
        scan_entries(&paths, &db, &now, no_opts(), |cur, tot, _| {
            calls.push((cur, tot));
        });
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], (1, 3));
        assert_eq!(calls[1], (3, 3));
    }
}
