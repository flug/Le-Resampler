use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct ExportEntry {
    pub path: String,
    pub filename: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub bpm: Option<f64>,
    pub musical_key: Option<String>,
    pub sample_type: Option<String>,
}

#[derive(Serialize)]
pub struct ExportResult {
    pub copied: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub fn resolve_path(template: &str, entry: &ExportEntry) -> PathBuf {
    let category = entry
        .category
        .as_deref()
        .or_else(|| entry.tags.first().map(|t| t.as_str()))
        .unwrap_or("misc");
    let bpm = entry
        .bpm
        .map(|b| format!("{:.0}", b))
        .unwrap_or_else(|| "unknown".to_string());
    let s = template
        .replace("%filename%", &entry.filename)
        .replace("%category%", category)
        .replace("%key%", entry.musical_key.as_deref().unwrap_or("unknown"))
        .replace("%bpm%", &bpm)
        .replace(
            "%tags%",
            entry.tags.first().map(|t| t.as_str()).unwrap_or("misc"),
        )
        .replace("%type%", entry.sample_type.as_deref().unwrap_or("misc"));
    PathBuf::from(s)
}

pub fn copy_samples(dest: &str, samples: &[ExportEntry], template: &str) -> ExportResult {
    let dest_path = Path::new(dest);
    let mut copied = 0;
    let mut skipped = 0;
    let mut errors: Vec<String> = Vec::new();

    for entry in samples {
        let rel = resolve_path(template, entry);
        let dest_file = dest_path.join(&rel);

        if let Some(parent) = dest_file.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                errors.push(format!("Cannot create folder {}: {}", parent.display(), e));
                skipped += 1;
                continue;
            }
        }

        if dest_file.exists() {
            skipped += 1;
            continue;
        }

        match std::fs::copy(&entry.path, &dest_file) {
            Ok(_) => copied += 1,
            Err(e) => {
                errors.push(format!("Cannot copy {}: {}", entry.path, e));
                skipped += 1;
            }
        }
    }

    ExportResult {
        copied,
        skipped,
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(path: &std::path::Path, content: &[u8]) {
        std::fs::write(path, content).unwrap();
    }

    fn entry(path: &str, filename: &str, category: Option<&str>, tags: Vec<&str>) -> ExportEntry {
        ExportEntry {
            path: path.to_string(),
            filename: filename.to_string(),
            category: category.map(|s| s.to_string()),
            tags: tags.into_iter().map(|s| s.to_string()).collect(),
            bpm: None,
            musical_key: None,
            sample_type: None,
        }
    }

    const DEFAULT: &str = "%category%/%filename%";

    // ── resolve_path tests ───────────────────────────────────────────────────

    #[test]
    fn resolve_path_category_filename() {
        let e = entry("/src/kick_001.wav", "kick_001.wav", Some("kick"), vec![]);
        assert_eq!(
            resolve_path(DEFAULT, &e),
            PathBuf::from("kick/kick_001.wav")
        );
    }

    #[test]
    fn resolve_path_key_nested() {
        let mut e = entry("/src/kick.wav", "kick.wav", Some("kick"), vec![]);
        e.musical_key = Some("Am".to_string());
        assert_eq!(
            resolve_path("%category%/%key%/%filename%", &e),
            PathBuf::from("kick/Am/kick.wav")
        );
    }

    #[test]
    fn resolve_path_bpm_fallback() {
        let e = entry("/src/x.wav", "x.wav", Some("loop"), vec![]);
        assert_eq!(
            resolve_path("%bpm%/%filename%", &e),
            PathBuf::from("unknown/x.wav")
        );
    }

    #[test]
    fn resolve_path_tags_first() {
        let e = entry("/src/loop.wav", "loop.wav", None, vec!["groovy"]);
        assert_eq!(
            resolve_path("%tags%/%filename%", &e),
            PathBuf::from("groovy/loop.wav")
        );
    }

    #[test]
    fn resolve_path_all_none_falls_back() {
        let e = entry("/src/x.wav", "x.wav", None, vec![]);
        assert_eq!(resolve_path(DEFAULT, &e), PathBuf::from("misc/x.wav"));
    }

    // ── copy_samples tests ───────────────────────────────────────────────────

    #[test]
    fn copy_single_sample_creates_folder_and_file() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("kick_001.wav");
        write_file(&src, b"data");

        let entries = vec![entry(
            src.to_str().unwrap(),
            "kick_001.wav",
            Some("kick"),
            vec![],
        )];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 1);
        assert_eq!(r.skipped, 0);
        assert!(r.errors.is_empty());
        assert!(dst_dir.path().join("kick").join("kick_001.wav").exists());
    }

    #[test]
    fn copy_uses_first_tag_when_no_category() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("loop.wav");
        write_file(&src, b"data");

        let entries = vec![entry(
            src.to_str().unwrap(),
            "loop.wav",
            None,
            vec!["groovy"],
        )];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 1);
        assert!(dst_dir.path().join("groovy").join("loop.wav").exists());
    }

    #[test]
    fn copy_falls_back_to_misc() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("unknown.wav");
        write_file(&src, b"data");

        let entries = vec![entry(src.to_str().unwrap(), "unknown.wav", None, vec![])];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 1);
        assert!(dst_dir.path().join("misc").join("unknown.wav").exists());
    }

    #[test]
    fn copy_existing_file_is_skipped() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("kick.wav");
        write_file(&src, b"data");

        std::fs::create_dir_all(dst_dir.path().join("kick")).unwrap();
        write_file(&dst_dir.path().join("kick").join("kick.wav"), b"existing");

        let entries = vec![entry(
            src.to_str().unwrap(),
            "kick.wav",
            Some("kick"),
            vec![],
        )];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 0);
        assert_eq!(r.skipped, 1);
        assert_eq!(
            std::fs::read(dst_dir.path().join("kick").join("kick.wav")).unwrap(),
            b"existing"
        );
    }

    #[test]
    fn copy_missing_source_file_is_error() {
        let dst_dir = tempdir().unwrap();
        let entries = vec![entry(
            "/nonexistent/path/sample.wav",
            "sample.wav",
            Some("kick"),
            vec![],
        )];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 0);
        assert_eq!(r.skipped, 1);
        assert!(!r.errors.is_empty());
    }

    #[test]
    fn copy_multiple_samples_different_folders() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let kick = src_dir.path().join("kick.wav");
        let snare = src_dir.path().join("snare.wav");
        write_file(&kick, b"kick");
        write_file(&snare, b"snare");

        let entries = vec![
            entry(kick.to_str().unwrap(), "kick.wav", Some("kick"), vec![]),
            entry(snare.to_str().unwrap(), "snare.wav", Some("snare"), vec![]),
        ];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 2);
        assert!(dst_dir.path().join("kick").join("kick.wav").exists());
        assert!(dst_dir.path().join("snare").join("snare.wav").exists());
    }

    #[test]
    fn copy_same_folder_groups_files() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let k1 = src_dir.path().join("kick1.wav");
        let k2 = src_dir.path().join("kick2.wav");
        write_file(&k1, b"k1");
        write_file(&k2, b"k2");

        let entries = vec![
            entry(k1.to_str().unwrap(), "kick1.wav", Some("kick"), vec![]),
            entry(k2.to_str().unwrap(), "kick2.wav", Some("kick"), vec![]),
        ];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries, DEFAULT);

        assert_eq!(r.copied, 2);
        assert!(dst_dir.path().join("kick").join("kick1.wav").exists());
        assert!(dst_dir.path().join("kick").join("kick2.wav").exists());
    }

    #[test]
    fn copy_samples_with_nested_template() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("pad.wav");
        write_file(&src, b"data");

        let mut e = entry(src.to_str().unwrap(), "pad.wav", Some("loop"), vec![]);
        e.musical_key = Some("Dm".to_string());

        let r = copy_samples(
            dst_dir.path().to_str().unwrap(),
            &[e],
            "%category%/%key%/%filename%",
        );

        assert_eq!(r.copied, 1);
        assert!(dst_dir
            .path()
            .join("loop")
            .join("Dm")
            .join("pad.wav")
            .exists());
    }
}
