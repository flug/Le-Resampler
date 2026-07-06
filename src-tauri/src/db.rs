use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub struct DbPool(pub Mutex<Connection>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SampleRecord {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub duration_ms: Option<i64>,
    pub bpm: Option<f64>,
    pub musical_key: Option<String>,
    pub category: Option<String>,
    pub sample_type: Option<String>,
    pub date_added: String,
    pub file_size: Option<i64>,
    pub tags: Vec<String>,
}

impl DbPool {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self(Mutex::new(conn)))
    }

    pub fn init_schema(&self) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS samples (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                filename TEXT NOT NULL,
                duration_ms INTEGER,
                bpm REAL,
                musical_key TEXT,
                category TEXT,
                sample_type TEXT,
                date_added TEXT NOT NULL,
                file_size INTEGER
            );
            CREATE TABLE IF NOT EXISTS tags (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS sample_tags (
                sample_id INTEGER NOT NULL REFERENCES samples(id) ON DELETE CASCADE,
                tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (sample_id, tag_id)
            );
            CREATE TABLE IF NOT EXISTS folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    // Returns true if newly inserted, false if already existed
    pub fn upsert_sample(&self, record: &SampleRecord) -> Result<bool> {
        let conn = self.0.lock().unwrap();
        let rows = conn.execute(
            "INSERT OR IGNORE INTO samples
             (path, filename, duration_ms, bpm, musical_key, category, sample_type, date_added, file_size)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.path,
                record.filename,
                record.duration_ms,
                record.bpm,
                record.musical_key,
                record.category,
                record.sample_type,
                record.date_added,
                record.file_size,
            ],
        )?;
        Ok(rows > 0)
    }

    pub fn list_samples(
        &self,
        filter_category: Option<&str>,
        filter_tag: Option<&str>,
        search: Option<&str>,
    ) -> Result<Vec<SampleRecord>> {
        let conn = self.0.lock().unwrap();

        // Build query with optional WHERE clauses
        let mut sql = String::from(
            "SELECT s.id, s.path, s.filename, s.duration_ms, s.bpm, s.musical_key,
                    s.category, s.sample_type, s.date_added, s.file_size,
                    GROUP_CONCAT(t.name, ',') as tag_list
             FROM samples s
             LEFT JOIN sample_tags st ON s.id = st.sample_id
             LEFT JOIN tags t ON st.tag_id = t.id
             WHERE 1=1",
        );

        let mut conditions: Vec<String> = Vec::new();
        if filter_category.is_some() {
            conditions.push("s.category = ?".to_string());
        }
        if search.is_some() {
            conditions.push("s.filename LIKE ?".to_string());
        }
        for c in &conditions {
            sql.push_str(&format!(" AND {}", c));
        }
        sql.push_str(" GROUP BY s.id");
        if filter_tag.is_some() {
            sql.push_str(" HAVING tag_list LIKE ?");
        }
        sql.push_str(" ORDER BY s.date_added DESC");

        let mut stmt = conn.prepare(&sql)?;

        // Build params dynamically
        let mut param_values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(cat) = filter_category {
            param_values.push(Box::new(cat.to_string()));
        }
        if let Some(s) = search {
            param_values.push(Box::new(s.to_string()));
        }
        if let Some(tag) = filter_tag {
            param_values.push(Box::new(format!("%{}%", tag)));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();

        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            let tag_list: Option<String> = row.get(10)?;
            let tags = tag_list
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            Ok(SampleRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                duration_ms: row.get(3)?,
                bpm: row.get(4)?,
                musical_key: row.get(5)?,
                category: row.get(6)?,
                sample_type: row.get(7)?,
                date_added: row.get(8)?,
                file_size: row.get(9)?,
                tags,
            })
        })?;

        rows.collect()
    }

    pub fn update_sample(
        &self,
        id: i64,
        category: Option<&str>,
        bpm: Option<f64>,
        musical_key: Option<&str>,
    ) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "UPDATE samples SET category = ?1, bpm = ?2, musical_key = ?3 WHERE id = ?4",
            params![category, bpm, musical_key, id],
        )?;
        Ok(())
    }

    pub fn add_tag(&self, sample_id: i64, tag_name: &str) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![tag_name],
        )?;
        let tag_id: i64 = conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            params![tag_name],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO sample_tags (sample_id, tag_id) VALUES (?1, ?2)",
            params![sample_id, tag_id],
        )?;
        Ok(())
    }

    pub fn remove_tag(&self, sample_id: i64, tag_name: &str) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "DELETE FROM sample_tags WHERE sample_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![sample_id, tag_name],
        )?;
        Ok(())
    }

    pub fn get_all_tags(&self) -> Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM tags ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn add_folder(&self, path: &str) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO folders (path) VALUES (?1)",
            params![path],
        )?;
        Ok(())
    }

    pub fn get_folders(&self) -> Result<Vec<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path FROM folders ORDER BY path")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.0.lock().unwrap();
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        match stmt.query_row(params![key], |r| r.get(0)) {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.0.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn new_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self(Mutex::new(conn)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn make_db() -> Arc<DbPool> {
        let db = DbPool::new_in_memory().unwrap();
        db.init_schema().unwrap();
        Arc::new(db)
    }

    fn make_sample(path: &str, category: Option<&str>) -> SampleRecord {
        SampleRecord {
            id: 0,
            path: path.to_string(),
            filename: path.split('/').next_back().unwrap_or(path).to_string(),
            duration_ms: None,
            bpm: None,
            musical_key: None,
            category: category.map(|s| s.to_string()),
            sample_type: None,
            date_added: "2026-01-01T00:00:00Z".to_string(),
            file_size: None,
            tags: vec![],
        }
    }

    #[test]
    fn schema_init_succeeds() {
        make_db(); // panics if init fails
    }

    #[test]
    fn upsert_new_returns_true() {
        let db = make_db();
        let r = make_sample("/audio/kick.wav", None);
        assert!(db.upsert_sample(&r).unwrap());
    }

    #[test]
    fn upsert_duplicate_returns_false() {
        let db = make_db();
        let r = make_sample("/audio/kick.wav", None);
        db.upsert_sample(&r).unwrap();
        assert!(!db.upsert_sample(&r).unwrap());
    }

    #[test]
    fn list_empty() {
        let db = make_db();
        assert_eq!(db.list_samples(None, None, None).unwrap().len(), 0);
    }

    #[test]
    fn list_all_two_samples() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick.wav", None)).unwrap();
        db.upsert_sample(&make_sample("/a/snare.wav", None))
            .unwrap();
        assert_eq!(db.list_samples(None, None, None).unwrap().len(), 2);
    }

    #[test]
    fn list_filter_category_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick.wav", Some("kick")))
            .unwrap();
        db.upsert_sample(&make_sample("/a/snare.wav", Some("snare")))
            .unwrap();
        let results = db.list_samples(Some("kick"), None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "kick.wav");
    }

    #[test]
    fn list_filter_category_no_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick.wav", Some("kick")))
            .unwrap();
        let results = db.list_samples(Some("snare"), None, None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn list_search_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick_001.wav", None))
            .unwrap();
        db.upsert_sample(&make_sample("/a/snare_001.wav", None))
            .unwrap();
        let results = db.list_samples(None, None, Some("%kick%")).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "kick_001.wav");
    }

    #[test]
    fn list_search_no_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick.wav", None)).unwrap();
        let results = db.list_samples(None, None, Some("%xyz%")).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn list_filter_tag_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/groove.wav", None))
            .unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "groovy").unwrap();
        let results = db.list_samples(None, Some("groovy"), None).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn list_filter_tag_no_match() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/groove.wav", None))
            .unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "groovy").unwrap();
        let results = db.list_samples(None, Some("heavy"), None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn list_all_filters_combined() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/kick_001.wav", Some("kick")))
            .unwrap();
        db.upsert_sample(&make_sample("/a/snare_001.wav", Some("snare")))
            .unwrap();
        let all = db.list_samples(None, None, None).unwrap();
        let kick_id = all
            .iter()
            .find(|r| r.filename == "kick_001.wav")
            .unwrap()
            .id;
        db.add_tag(kick_id, "punchy").unwrap();
        let results = db
            .list_samples(Some("kick"), Some("punchy"), Some("%kick%"))
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].filename, "kick_001.wav");
    }

    #[test]
    fn update_sample_reflects_in_list() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/pad.wav", None)).unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.update_sample(id, Some("loop"), Some(120.0), Some("Am"))
            .unwrap();
        let results = db.list_samples(None, None, None).unwrap();
        assert_eq!(results[0].category.as_deref(), Some("loop"));
        assert_eq!(results[0].bpm, Some(120.0));
        assert_eq!(results[0].musical_key.as_deref(), Some("Am"));
    }

    #[test]
    fn add_tag_appears_in_get_all_tags() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/pad.wav", None)).unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "chill").unwrap();
        let tags = db.get_all_tags().unwrap();
        assert!(tags.contains(&"chill".to_string()));
    }

    #[test]
    fn add_tag_idempotent() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/pad.wav", None)).unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "chill").unwrap();
        db.add_tag(id, "chill").unwrap(); // second add should be ignored
        let tags = db.get_all_tags().unwrap();
        assert_eq!(tags.iter().filter(|t| *t == "chill").count(), 1);
        let sample_tags = db.list_samples(None, Some("chill"), None).unwrap();
        assert_eq!(sample_tags.len(), 1);
    }

    #[test]
    fn remove_tag_disassociates() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/pad.wav", None)).unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "warm").unwrap();
        db.remove_tag(id, "warm").unwrap();
        let results = db.list_samples(None, Some("warm"), None).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn add_folder_stores_path() {
        let db = make_db();
        db.add_folder("/music/samples").unwrap();
        let folders = db.get_folders().unwrap();
        assert_eq!(folders, vec!["/music/samples"]);
    }

    #[test]
    fn add_folder_idempotent() {
        let db = make_db();
        db.add_folder("/music/samples").unwrap();
        db.add_folder("/music/samples").unwrap();
        let folders = db.get_folders().unwrap();
        assert_eq!(folders.len(), 1);
    }

    #[test]
    fn get_folders_ordered_alphabetically() {
        let db = make_db();
        db.add_folder("/z/folder").unwrap();
        db.add_folder("/a/folder").unwrap();
        let folders = db.get_folders().unwrap();
        assert_eq!(folders, vec!["/a/folder", "/z/folder"]);
    }

    #[test]
    fn get_folders_empty() {
        let db = make_db();
        assert_eq!(db.get_folders().unwrap().len(), 0);
    }

    #[test]
    fn settings_set_and_get() {
        let db = make_db();
        db.set_setting("export_template", "%category%/%filename%")
            .unwrap();
        assert_eq!(
            db.get_setting("export_template").unwrap(),
            Some("%category%/%filename%".to_string())
        );
    }

    #[test]
    fn settings_overwrite() {
        let db = make_db();
        db.set_setting("export_template", "%category%/%filename%")
            .unwrap();
        db.set_setting("export_template", "%key%/%filename%")
            .unwrap();
        assert_eq!(
            db.get_setting("export_template").unwrap(),
            Some("%key%/%filename%".to_string())
        );
    }

    #[test]
    fn settings_get_missing_key_returns_none() {
        let db = make_db();
        assert_eq!(db.get_setting("nonexistent").unwrap(), None);
    }

    #[test]
    fn get_all_tags_ordered_alphabetically() {
        let db = make_db();
        db.upsert_sample(&make_sample("/a/pad.wav", None)).unwrap();
        let id = db.list_samples(None, None, None).unwrap()[0].id;
        db.add_tag(id, "zebra").unwrap();
        db.add_tag(id, "apple").unwrap();
        db.add_tag(id, "mango").unwrap();
        let tags = db.get_all_tags().unwrap();
        assert_eq!(tags, vec!["apple", "mango", "zebra"]);
    }
}
