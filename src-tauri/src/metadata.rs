use lofty::{AudioFile, ItemKey, Probe, TaggedFileExt};
use regex::Regex;
use std::path::Path;

pub struct ExtractedMeta {
    pub duration_ms: Option<i64>,
    pub bpm: Option<f64>,
    pub musical_key: Option<String>,
    pub category: Option<String>,
    pub sample_type: Option<String>,
    pub file_size: Option<i64>,
}

pub fn extract(path: &Path) -> Result<ExtractedMeta, String> {
    let file_size = path.metadata().ok().map(|m| m.len() as i64);

    // Step 1: embedded metadata via lofty
    let (duration_ms, mut bpm, mut musical_key) = match Probe::open(path)
        .map_err(|e| e.to_string())
        .and_then(|p| p.read().map_err(|e| e.to_string()))
    {
        Ok(tagged_file) => {
            let props = tagged_file.properties();
            let dur = props.duration().as_millis() as i64;
            let duration_ms = if dur > 0 { Some(dur) } else { None };

            let tag = tagged_file
                .primary_tag()
                .or_else(|| tagged_file.first_tag());

            let bpm = tag
                .and_then(|t| t.get_string(&ItemKey::Bpm))
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|&v| v > 0.0);

            let key = tag
                .and_then(|t| t.get_string(&ItemKey::InitialKey))
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty());

            (duration_ms, bpm, key)
        }
        Err(e) => {
            log::debug!("lofty failed for {:?}: {}", path, e);
            (None, None, None)
        }
    };

    // Step 2: filename/path heuristics
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let path_str = path.to_string_lossy();

    if bpm.is_none() {
        bpm = infer_bpm_from_filename(filename);
    }

    if musical_key.is_none() {
        musical_key = infer_key_from_filename(filename);
    }

    let category = infer_category_from_path(&path_str);

    let sample_type = duration_ms.map(|d| {
        if d < 2000 {
            "one-shot".to_string()
        } else {
            "loop".to_string()
        }
    });

    Ok(ExtractedMeta {
        duration_ms,
        bpm,
        musical_key,
        category,
        sample_type,
        file_size,
    })
}

pub(crate) fn infer_bpm_from_filename(filename: &str) -> Option<f64> {
    let re = Regex::new(r"(?i)\b(\d{2,3})[_\s-]?bpm\b").unwrap();
    re.captures(filename)
        .and_then(|cap| cap[1].parse::<f64>().ok())
        .filter(|&v| (40.0..=300.0).contains(&v))
}

pub(crate) fn infer_key_from_filename(filename: &str) -> Option<String> {
    let re = Regex::new(r"(?i)[_\s-]([A-G][#b]?(?:min|maj|m)?)[_\s\-.]").unwrap();
    re.captures(filename).map(|cap| cap[1].to_string())
}

pub(crate) fn infer_category_from_path(path: &str) -> Option<String> {
    let lower = path.to_lowercase();
    let checks: &[(&str, &str)] = &[
        ("kick", "kick"),
        ("snare", "snare"),
        ("clap", "snare"),
        ("/hat", "hat"),
        ("\\hat", "hat"),
        ("hihat", "hat"),
        ("hi-hat", "hat"),
        ("loop", "loop"),
        ("vocal", "vocal"),
        ("voice", "vocal"),
        ("/fx/", "fx"),
        ("\\fx\\", "fx"),
        ("effect", "fx"),
        ("/bass/", "bass"),
        ("\\bass\\", "bass"),
        ("basslne", "bass"),
        ("perc", "perc"),
        ("tom", "perc"),
    ];
    for (pattern, category) in checks {
        if lower.contains(pattern) {
            return Some((*category).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as IoWrite;
    use tempfile::tempdir;

    fn make_wav(sample_rate: u32, num_samples: u32) -> Vec<u8> {
        let data_size = num_samples * 2; // 16-bit mono = 2 bytes/sample
        let mut v = Vec::new();
        v.extend_from_slice(b"RIFF");
        v.extend_from_slice(&(36u32 + data_size).to_le_bytes());
        v.extend_from_slice(b"WAVE");
        v.extend_from_slice(b"fmt ");
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
        v.extend_from_slice(&2u16.to_le_bytes()); // block align
        v.extend_from_slice(&16u16.to_le_bytes()); // 16 bits per sample
        v.extend_from_slice(b"data");
        v.extend_from_slice(&data_size.to_le_bytes());
        v.extend(std::iter::repeat_n(0u8, data_size as usize));
        v
    }

    // --- infer_bpm_from_filename ---

    #[test]
    fn bpm_dash_separator() {
        assert_eq!(infer_bpm_from_filename("groove-128bpm.wav"), Some(128.0));
    }

    #[test]
    fn bpm_uppercase() {
        assert_eq!(infer_bpm_from_filename("groove-128BPM.wav"), Some(128.0));
    }

    #[test]
    fn bpm_space_separator() {
        assert_eq!(infer_bpm_from_filename("hit 90 bpm.wav"), Some(90.0));
    }

    #[test]
    fn bpm_start_of_string() {
        assert_eq!(infer_bpm_from_filename("128bpm.wav"), Some(128.0));
    }

    #[test]
    fn bpm_boundary_low() {
        assert_eq!(infer_bpm_from_filename("40bpm.wav"), Some(40.0));
    }

    #[test]
    fn bpm_boundary_high() {
        assert_eq!(infer_bpm_from_filename("300bpm.wav"), Some(300.0));
    }

    #[test]
    fn bpm_below_min_filtered_out() {
        assert_eq!(infer_bpm_from_filename("kick-30bpm.wav"), None);
    }

    #[test]
    fn bpm_above_max_filtered_out() {
        assert_eq!(infer_bpm_from_filename("kick-350bpm.wav"), None);
    }

    #[test]
    fn bpm_no_bpm_in_name() {
        assert_eq!(infer_bpm_from_filename("kick.wav"), None);
    }

    // --- infer_key_from_filename ---

    #[test]
    fn key_major() {
        assert_eq!(
            infer_key_from_filename("arp_Cmaj_pad.wav"),
            Some("Cmaj".to_string())
        );
    }

    #[test]
    fn key_minor_short() {
        assert_eq!(
            infer_key_from_filename("sample_Am_loop.wav"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn key_sharp_minor() {
        assert_eq!(
            infer_key_from_filename("chord_F#m-vox.wav"),
            Some("F#m".to_string())
        );
    }

    #[test]
    fn key_flat() {
        assert_eq!(
            infer_key_from_filename("pad_Bb_warm.wav"),
            Some("Bb".to_string())
        );
    }

    #[test]
    fn key_no_key_in_name() {
        assert_eq!(infer_key_from_filename("kick_128bpm.wav"), None);
    }

    // --- infer_category_from_path ---

    #[test]
    fn category_kick() {
        assert_eq!(
            infer_category_from_path("/samples/kick/hit.wav"),
            Some("kick".to_string())
        );
    }

    #[test]
    fn category_snare() {
        assert_eq!(
            infer_category_from_path("/samples/snare/hit.wav"),
            Some("snare".to_string())
        );
    }

    #[test]
    fn category_clap_maps_to_snare() {
        assert_eq!(
            infer_category_from_path("/samples/clap/hit.wav"),
            Some("snare".to_string())
        );
    }

    #[test]
    fn category_hat_slash_prefix() {
        assert_eq!(
            infer_category_from_path("/samples/hat/closed.wav"),
            Some("hat".to_string())
        );
    }

    #[test]
    fn category_hihat() {
        assert_eq!(
            infer_category_from_path("/samples/hihat/open.wav"),
            Some("hat".to_string())
        );
    }

    #[test]
    fn category_hi_hat_dash() {
        assert_eq!(
            infer_category_from_path("/samples/hi-hat/pedal.wav"),
            Some("hat".to_string())
        );
    }

    #[test]
    fn category_loop() {
        assert_eq!(
            infer_category_from_path("/samples/loop/beat.wav"),
            Some("loop".to_string())
        );
    }

    #[test]
    fn category_vocal() {
        assert_eq!(
            infer_category_from_path("/samples/vocal/chorus.wav"),
            Some("vocal".to_string())
        );
    }

    #[test]
    fn category_voice_maps_to_vocal() {
        assert_eq!(
            infer_category_from_path("/samples/voice/whisper.wav"),
            Some("vocal".to_string())
        );
    }

    #[test]
    fn category_fx() {
        assert_eq!(
            infer_category_from_path("/samples/fx/sweep.wav"),
            Some("fx".to_string())
        );
    }

    #[test]
    fn category_bass() {
        assert_eq!(
            infer_category_from_path("/samples/bass/line.wav"),
            Some("bass".to_string())
        );
    }

    #[test]
    fn category_perc() {
        assert_eq!(
            infer_category_from_path("/samples/perc/conga.wav"),
            Some("perc".to_string())
        );
    }

    #[test]
    fn category_none() {
        assert_eq!(infer_category_from_path("/samples/misc/sound.wav"), None);
    }

    // --- extract() integration ---

    #[test]
    fn extract_nonexistent_path_ok() {
        let result = extract(Path::new("/nonexistent/path/sound.wav"));
        assert!(result.is_ok());
        let meta = result.unwrap();
        assert!(meta.duration_ms.is_none());
        assert!(meta.file_size.is_none());
        assert!(meta.sample_type.is_none());
    }

    #[test]
    fn extract_wav_one_shot_sample_type() {
        // 1 sample at 1000 Hz = 1 ms → one-shot
        let wav = make_wav(1000, 1);
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        std::fs::write(&path, &wav).unwrap();
        let meta = extract(&path).unwrap();
        assert_eq!(meta.sample_type.as_deref(), Some("one-shot"));
        assert!(meta.file_size.is_some());
    }

    #[test]
    fn extract_wav_loop_sample_type() {
        // 2000 samples at 1000 Hz = 2000 ms → loop
        let wav = make_wav(1000, 2000);
        let dir = tempdir().unwrap();
        let path = dir.path().join("sample.wav");
        std::fs::write(&path, &wav).unwrap();
        let meta = extract(&path).unwrap();
        assert_eq!(meta.sample_type.as_deref(), Some("loop"));
    }

    #[test]
    fn extract_infers_bpm_and_category_from_path() {
        let wav = make_wav(44100, 1);
        let dir = tempdir().unwrap();
        let kick_dir = dir.path().join("kick");
        std::fs::create_dir_all(&kick_dir).unwrap();
        let path = kick_dir.join("hit-128bpm.wav");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(&wav).unwrap();
        let meta = extract(&path).unwrap();
        assert_eq!(meta.bpm, Some(128.0));
        assert_eq!(meta.category.as_deref(), Some("kick"));
    }
}
