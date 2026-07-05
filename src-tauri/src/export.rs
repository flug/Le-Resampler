use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Deserialize)]
pub struct ExportEntry {
    pub path: String,
    pub category: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct ExportResult {
    pub copied: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub fn copy_samples(dest: &str, samples: &[ExportEntry]) -> ExportResult {
    let dest_path = Path::new(dest);
    let mut copied = 0;
    let mut skipped = 0;
    let mut errors: Vec<String> = Vec::new();

    for entry in samples {
        let folder_name = entry
            .category
            .as_deref()
            .or_else(|| entry.tags.first().map(|t| t.as_str()))
            .unwrap_or("misc");

        let source = Path::new(&entry.path);

        let filename = match source.file_name() {
            Some(f) => f,
            None => {
                errors.push(format!("Invalid path: {}", entry.path));
                skipped += 1;
                continue;
            }
        };

        let folder = dest_path.join(folder_name);
        if let Err(e) = std::fs::create_dir_all(&folder) {
            errors.push(format!("Cannot create folder {}: {}", folder.display(), e));
            skipped += 1;
            continue;
        }

        let dest_file = folder.join(filename);
        if dest_file.exists() {
            skipped += 1;
            continue;
        }

        match std::fs::copy(source, &dest_file) {
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

    #[test]
    fn copy_single_sample_creates_folder_and_file() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("kick_001.wav");
        write_file(&src, b"data");

        let entries = vec![ExportEntry {
            path: src.to_str().unwrap().to_string(),
            category: Some("kick".to_string()),
            tags: vec![],
        }];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

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

        let entries = vec![ExportEntry {
            path: src.to_str().unwrap().to_string(),
            category: None,
            tags: vec!["groovy".to_string()],
        }];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

        assert_eq!(r.copied, 1);
        assert!(dst_dir.path().join("groovy").join("loop.wav").exists());
    }

    #[test]
    fn copy_falls_back_to_misc() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("unknown.wav");
        write_file(&src, b"data");

        let entries = vec![ExportEntry {
            path: src.to_str().unwrap().to_string(),
            category: None,
            tags: vec![],
        }];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

        assert_eq!(r.copied, 1);
        assert!(dst_dir.path().join("misc").join("unknown.wav").exists());
    }

    #[test]
    fn copy_existing_file_is_skipped() {
        let src_dir = tempdir().unwrap();
        let dst_dir = tempdir().unwrap();
        let src = src_dir.path().join("kick.wav");
        write_file(&src, b"data");

        // Pre-create destination file
        std::fs::create_dir_all(dst_dir.path().join("kick")).unwrap();
        write_file(&dst_dir.path().join("kick").join("kick.wav"), b"existing");

        let entries = vec![ExportEntry {
            path: src.to_str().unwrap().to_string(),
            category: Some("kick".to_string()),
            tags: vec![],
        }];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

        assert_eq!(r.copied, 0);
        assert_eq!(r.skipped, 1);
        // Existing file must not be overwritten
        assert_eq!(
            std::fs::read(dst_dir.path().join("kick").join("kick.wav")).unwrap(),
            b"existing"
        );
    }

    #[test]
    fn copy_missing_source_file_is_error() {
        let dst_dir = tempdir().unwrap();
        let entries = vec![ExportEntry {
            path: "/nonexistent/path/sample.wav".to_string(),
            category: Some("kick".to_string()),
            tags: vec![],
        }];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

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
            ExportEntry {
                path: kick.to_str().unwrap().to_string(),
                category: Some("kick".to_string()),
                tags: vec![],
            },
            ExportEntry {
                path: snare.to_str().unwrap().to_string(),
                category: Some("snare".to_string()),
                tags: vec![],
            },
        ];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

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
            ExportEntry {
                path: k1.to_str().unwrap().to_string(),
                category: Some("kick".to_string()),
                tags: vec![],
            },
            ExportEntry {
                path: k2.to_str().unwrap().to_string(),
                category: Some("kick".to_string()),
                tags: vec![],
            },
        ];
        let r = copy_samples(dst_dir.path().to_str().unwrap(), &entries);

        assert_eq!(r.copied, 2);
        assert!(dst_dir.path().join("kick").join("kick1.wav").exists());
        assert!(dst_dir.path().join("kick").join("kick2.wav").exists());
    }
}
