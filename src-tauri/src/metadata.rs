use lofty::{AudioFile, ItemKey, Probe, TaggedFileExt};
use regex::Regex;
use std::path::Path;

#[derive(Debug, PartialEq)]
pub enum KeySource {
    Metadata,
    Filename,
    AudioPitch,
}

pub struct ExtractedMeta {
    pub duration_ms: Option<i64>,
    pub bpm: Option<f64>,
    pub musical_key: Option<String>,
    pub key_source: Option<KeySource>,
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

    let mut key_source = if musical_key.is_some() {
        Some(KeySource::Metadata)
    } else {
        None
    };

    // Step 2: filename/path heuristics
    let filename = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let path_str = path.to_string_lossy();

    if bpm.is_none() {
        bpm = infer_bpm_from_filename(filename);
    }

    if musical_key.is_none() {
        musical_key = infer_key_from_filename(filename);
        if musical_key.is_some() {
            key_source = Some(KeySource::Filename);
        }
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
        key_source,
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
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);
    // Supports: _Am_, -Am-, (Am), [Am], key at start/end of stem.
    // Longer suffix alternatives must come first to avoid partial matches (e.g. "minor" before "min").
    let re = Regex::new(
        r"(?i)(?:^|[_\s\-(\[])([A-G][#b]?(?:min(?:or)?|maj(?:or)?|m)?)(?:[_\s\-)\].]|$)",
    )
    .unwrap();
    re.captures(stem).map(|cap| normalize_key(&cap[1]))
}

fn normalize_key(raw: &str) -> String {
    let lower = raw.to_lowercase();
    let note = lower
        .chars()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let rest = &lower[1..];
    if let Some(base) = rest
        .strip_suffix("minor")
        .or_else(|| rest.strip_suffix("min"))
    {
        format!("{note}{base}m")
    } else if let Some(base) = rest
        .strip_suffix("major")
        .or_else(|| rest.strip_suffix("maj"))
    {
        format!("{note}{base}maj")
    } else {
        format!("{note}{rest}")
    }
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

/// Writes a musical key to the primary tag of an audio file.
/// Silently skips files that have no existing tag rather than creating one.
pub fn write_key_to_file(path: &Path, key: &str) -> Result<(), String> {
    let probe = Probe::open(path).map_err(|e| e.to_string())?;
    let mut tagged_file = probe.read().map_err(|e| e.to_string())?;

    if tagged_file.primary_tag().is_none() {
        return Ok(());
    }

    {
        let tag = tagged_file.primary_tag_mut().unwrap();
        tag.insert_text(ItemKey::InitialKey, key.to_string());
    }

    tagged_file.save_to_path(path).map_err(|e| e.to_string())
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

    /// WAV with a minimal ID3v2.3 chunk embedded so lofty exposes a primary tag.
    fn make_tagged_wav(sample_rate: u32, num_samples: u32) -> Vec<u8> {
        // Minimal ID3v2.3 header: magic + version + flags + 4-byte synchsafe size (0)
        let id3: &[u8] = &[b'I', b'D', b'3', 3, 0, 0, 0, 0, 0, 0]; // 10 bytes, no frames
        let id3_size = id3.len() as u32;
        let pcm_size = num_samples * 2;
        // RIFF size = "WAVE"(4) + id3_chunk(8+10) + fmt(8+16) + data(8+pcm)
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
        v.extend_from_slice(&1u16.to_le_bytes()); // PCM
        v.extend_from_slice(&1u16.to_le_bytes()); // mono
        v.extend_from_slice(&sample_rate.to_le_bytes());
        v.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(b"data");
        v.extend_from_slice(&pcm_size.to_le_bytes());
        v.extend(std::iter::repeat_n(0u8, pcm_size as usize));
        v
    }

    // --- write_key_to_file ---

    #[test]
    fn write_key_to_file_nonexistent_returns_err() {
        assert!(write_key_to_file(Path::new("/nonexistent/sample.wav"), "Am").is_err());
    }

    #[test]
    fn write_key_to_file_invalid_format_returns_err() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("garbage.bin");
        std::fs::write(&path, b"not audio").unwrap();
        assert!(write_key_to_file(&path, "Am").is_err());
    }

    #[test]
    fn write_key_to_file_no_primary_tag_skips_silently() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("untagged.wav");
        std::fs::write(&path, make_wav(44100, 100)).unwrap();
        assert!(write_key_to_file(&path, "Am").is_ok());
    }

    #[test]
    fn write_key_to_file_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("tagged.wav");
        std::fs::write(&path, make_tagged_wav(44100, 100)).unwrap();
        write_key_to_file(&path, "Am").unwrap();
        let tagged = Probe::open(&path).unwrap().read().unwrap();
        let key = tagged
            .primary_tag()
            .and_then(|t| t.get_string(&ItemKey::InitialKey))
            .map(str::to_string);
        assert_eq!(key, Some("Am".to_string()));
    }

    // --- key_source tracking in extract() ---

    #[test]
    fn extract_key_source_from_filename() {
        let wav = make_wav(44100, 1);
        let dir = tempdir().unwrap();
        let path = dir.path().join("pad_Am_warm.wav");
        std::fs::write(&path, &wav).unwrap();
        let meta = extract(&path).unwrap();
        assert_eq!(meta.musical_key.as_deref(), Some("Am"));
        assert_eq!(meta.key_source, Some(KeySource::Filename));
    }

    #[test]
    fn extract_key_source_none_when_no_key() {
        let wav = make_wav(44100, 1);
        let dir = tempdir().unwrap();
        let path = dir.path().join("kick_128bpm.wav");
        std::fs::write(&path, &wav).unwrap();
        let meta = extract(&path).unwrap();
        assert!(meta.musical_key.is_none());
        assert!(meta.key_source.is_none());
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

    #[test]
    fn key_at_start_of_filename() {
        assert_eq!(
            infer_key_from_filename("Am_loop_140bpm.wav"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn key_in_parentheses() {
        assert_eq!(
            infer_key_from_filename("loop_(Am)_140bpm.wav"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn key_in_brackets() {
        assert_eq!(
            infer_key_from_filename("loop_[Dm]_120bpm.wav"),
            Some("Dm".to_string())
        );
    }

    #[test]
    fn key_minor_long_form() {
        assert_eq!(
            infer_key_from_filename("loop_Aminor_140bpm.wav"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn key_major_long_form() {
        assert_eq!(
            infer_key_from_filename("loop_Cmajor_120bpm.wav"),
            Some("Cmaj".to_string())
        );
    }

    #[test]
    fn key_min_short_form_normalized() {
        assert_eq!(
            infer_key_from_filename("loop_Fmin_120bpm.wav"),
            Some("Fm".to_string())
        );
    }

    #[test]
    fn key_maj_short_form_normalized() {
        assert_eq!(
            infer_key_from_filename("pad_Gmaj_warm.wav"),
            Some("Gmaj".to_string())
        );
    }

    #[test]
    fn key_lowercase_normalized_to_title_case() {
        assert_eq!(
            infer_key_from_filename("arp_cm_pad.wav"),
            Some("Cm".to_string())
        );
    }

    #[test]
    fn key_at_end_of_stem() {
        assert_eq!(
            infer_key_from_filename("pad_warm_Am.wav"),
            Some("Am".to_string())
        );
    }

    #[test]
    fn key_sharp_minor_normalized() {
        assert_eq!(
            infer_key_from_filename("chord_C#minor_vox.wav"),
            Some("C#m".to_string())
        );
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
