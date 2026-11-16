use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Deserialize;

use crate::pinyin;

#[derive(Debug, Clone, Deserialize)]
pub struct VocabEntry {
    pub id: String,
    pub simplified: String,
    #[serde(default)]
    pub traditional: String,
    pub pinyin: String,
    pub meaning: String,
    pub hsk: u8,
}

impl VocabEntry {
    pub fn syllables(&self) -> Vec<String> {
        self.pinyin
            .split_whitespace()
            .map(|syllable| syllable.to_string())
            .collect()
    }

    pub fn display_pinyin(&self) -> String {
        pinyin::marks(&self.pinyin)
    }

    pub fn display_traditional(&self) -> Option<&str> {
        if self.traditional.is_empty() || self.traditional == self.simplified {
            None
        } else {
            Some(&self.traditional)
        }
    }

    pub fn is_word(&self) -> bool {
        self.simplified.chars().count() > 1
    }
}

#[derive(Debug, Clone, Deserialize)]
struct GroupDef {
    id: String,
    name: String,
    entries: Vec<String>,
}

#[derive(Deserialize)]
struct VocabFile {
    version: u32,
    groups: Vec<GroupDef>,
    entries: Vec<VocabEntry>,
}

#[derive(Debug, Clone)]
pub struct GroupSummary {
    pub id: String,
    pub name: String,
    pub count: i64,
    pub custom: bool,
    /// Lowest mastery level across the group's entries (0 = not all started).
    pub mastery: u8,
}

/// Mastery thresholds: both quiz and writing counts must reach a step.
/// 1, 2, 3, 5, 8 correct each -> levels 1-5.
pub fn level_from_counts(quiz: i64, write: i64) -> u8 {
    match quiz.min(write) {
        n if n >= 8 => 5,
        n if n >= 5 => 4,
        n if n >= 3 => 3,
        n if n >= 2 => 2,
        n if n >= 1 => 1,
        _ => 0,
    }
}

pub struct Db {
    conn: Connection,
}

impl Db {
    pub fn open() -> Result<Self> {
        let dir = data_dir().join("hanzi");
        fs::create_dir_all(&dir)
            .with_context(|| format!("could not create data directory {}", dir.display()))?;

        let conn = Connection::open(dir.join("hanzi.db"))
            .with_context(|| format!("could not open database in {}", dir.display()))?;

        let mut db = Self { conn };
        db.init_schema()?;
        db.seed()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS vocab (
                id TEXT PRIMARY KEY,
                simplified TEXT NOT NULL,
                traditional TEXT NOT NULL DEFAULT '',
                pinyin TEXT NOT NULL,
                meaning TEXT NOT NULL,
                hsk INTEGER NOT NULL DEFAULT 1
            );

            CREATE TABLE IF NOT EXISTS syllables (
                vocab_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                key TEXT NOT NULL,
                PRIMARY KEY (vocab_id, position)
            );

            CREATE INDEX IF NOT EXISTS syllables_key ON syllables(key);

            CREATE TABLE IF NOT EXISTS groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                position INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS group_entries (
                group_id TEXT NOT NULL,
                vocab_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (group_id, vocab_id)
            );

            CREATE INDEX IF NOT EXISTS group_entries_group ON group_entries(group_id, position);

            -- User-created groups live outside the seeded data so a vocab
            -- version bump never deletes them.
            CREATE TABLE IF NOT EXISTS custom_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                position INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS custom_group_entries (
                group_id TEXT NOT NULL,
                vocab_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                PRIMARY KEY (group_id, vocab_id)
            );

            CREATE INDEX IF NOT EXISTS custom_group_entries_group
                ON custom_group_entries(group_id, position);

            -- Correct quiz picks and clean writings per deck entry. Kept out of
            -- the seeded tables so a vocab update never resets progress.
            CREATE TABLE IF NOT EXISTS mastery (
                vocab_id TEXT PRIMARY KEY,
                quiz_correct INTEGER NOT NULL DEFAULT 0,
                write_correct INTEGER NOT NULL DEFAULT 0
            );

            -- The old SRS scheduling tables are intentionally removed.
            DROP TABLE IF EXISTS cards;
            DROP TABLE IF EXISTS review_log;
            "#,
        )?;
        Ok(())
    }

    fn seed(&mut self) -> Result<()> {
        let raw = include_str!("../data/vocab.json");
        let file: VocabFile = serde_json::from_str(raw).context("data/vocab.json is not valid")?;
        let version = file.version.to_string();

        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'data_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;

        if current.as_deref() == Some(version.as_str()) {
            return Ok(());
        }

        let entry_ids: HashSet<&str> = file.entries.iter().map(|entry| entry.id.as_str()).collect();
        let tx = self.conn.unchecked_transaction()?;

        tx.execute("DELETE FROM group_entries", [])?;
        tx.execute("DELETE FROM groups", [])?;
        tx.execute("DELETE FROM syllables", [])?;
        tx.execute("DELETE FROM vocab", [])?;

        for entry in &file.entries {
            tx.execute(
                "INSERT INTO vocab (id, simplified, traditional, pinyin, meaning, hsk)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.id,
                    entry.simplified,
                    entry.traditional,
                    entry.pinyin,
                    entry.meaning,
                    entry.hsk
                ],
            )?;

            for (position, syllable) in entry.syllables().iter().enumerate() {
                tx.execute(
                    "INSERT INTO syllables (vocab_id, position, key) VALUES (?1, ?2, ?3)",
                    params![entry.id, position as i64, syllable],
                )?;
            }
        }

        for (group_position, group) in file.groups.iter().enumerate() {
            tx.execute(
                "INSERT INTO groups (id, name, position) VALUES (?1, ?2, ?3)",
                params![group.id, group.name, group_position as i64],
            )?;

            for (position, entry_id) in group.entries.iter().enumerate() {
                if !entry_ids.contains(entry_id.as_str()) {
                    eprintln!(
                        "hanzi: group {} references unknown entry {entry_id}",
                        group.id
                    );
                    continue;
                }
                tx.execute(
                    "INSERT INTO group_entries (group_id, vocab_id, position)
                     VALUES (?1, ?2, ?3)",
                    params![group.id, entry_id, position as i64],
                )?;
            }
        }

        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('data_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![version],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn groups(&self) -> Result<Vec<GroupSummary>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT id, name, count, custom FROM (
                SELECT g.id AS id, g.name AS name,
                       COUNT(v.id) AS count, 0 AS custom, g.position AS pos
                FROM groups g
                LEFT JOIN group_entries ge ON ge.group_id = g.id
                LEFT JOIN vocab v ON v.id = ge.vocab_id
                GROUP BY g.id, g.name, g.position
                UNION ALL
                SELECT cg.id, cg.name,
                       COUNT(v.id), 1, cg.position
                FROM custom_groups cg
                LEFT JOIN custom_group_entries cge ON cge.group_id = cg.id
                LEFT JOIN vocab v ON v.id = cge.vocab_id
                GROUP BY cg.id, cg.name, cg.position
            )
            ORDER BY custom, pos, name
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            Ok(GroupSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                count: row.get(2)?,
                custom: row.get::<_, i64>(3)? != 0,
                mastery: 0,
            })
        })?;

        let mut groups = Vec::new();
        for row in rows {
            groups.push(row?);
        }
        drop(statement);

        for group in &mut groups {
            group.mastery = self.group_mastery(&group.id).unwrap_or(0);
        }
        Ok(groups)
    }

    /// Raw correct counts for one entry.
    pub fn mastery_counts(&self, entry_id: &str) -> Result<(i64, i64)> {
        let counts: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT quiz_correct, write_correct FROM mastery WHERE vocab_id = ?1",
                params![entry_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        Ok(counts.unwrap_or((0, 0)))
    }

    /// Mastery level for one entry, derived from the lower of the quiz and
    /// writing counts so both skills have to progress.
    pub fn mastery_level(&self, entry_id: &str) -> Result<u8> {
        let (quiz, write) = self.mastery_counts(entry_id)?;
        Ok(level_from_counts(quiz, write))
    }

    /// Mastery levels for every entry of a group (built-in or custom).
    pub fn mastery_levels(&self, group_id: &str) -> Result<HashMap<String, u8>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT ge.vocab_id,
                   COALESCE(m.quiz_correct, 0),
                   COALESCE(m.write_correct, 0)
            FROM (
                SELECT vocab_id FROM group_entries WHERE group_id = ?1
                UNION ALL
                SELECT vocab_id FROM custom_group_entries WHERE group_id = ?1
            ) ge
            LEFT JOIN mastery m ON m.vocab_id = ge.vocab_id
            "#,
        )?;
        let rows = statement.query_map(params![group_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        let mut levels = HashMap::new();
        for row in rows {
            let (entry_id, quiz, write) = row?;
            levels.insert(entry_id, level_from_counts(quiz, write));
        }
        Ok(levels)
    }

    /// Lowest mastery level in a group; 0 if any entry is unstarted.
    pub fn group_mastery(&self, group_id: &str) -> Result<u8> {
        let levels = self.mastery_levels(group_id)?;
        Ok(levels.values().copied().min().unwrap_or(0))
    }

    pub fn record_quiz_correct(&self, entry_id: &str) -> Result<u8> {
        self.conn.execute(
            "INSERT INTO mastery (vocab_id, quiz_correct, write_correct)
             VALUES (?1, 1, 0)
             ON CONFLICT(vocab_id) DO UPDATE SET quiz_correct = quiz_correct + 1",
            params![entry_id],
        )?;
        self.mastery_level(entry_id)
    }

    pub fn record_write_correct(&self, entry_id: &str) -> Result<u8> {
        self.conn.execute(
            "INSERT INTO mastery (vocab_id, quiz_correct, write_correct)
             VALUES (?1, 0, 1)
             ON CONFLICT(vocab_id) DO UPDATE SET write_correct = write_correct + 1",
            params![entry_id],
        )?;
        self.mastery_level(entry_id)
    }

    /// A wrong quiz attempt removes one quiz point (never below zero).
    pub fn record_quiz_wrong(&self, entry_id: &str) -> Result<u8> {
        self.conn.execute(
            "INSERT INTO mastery (vocab_id, quiz_correct, write_correct)
             VALUES (?1, 0, 0)
             ON CONFLICT(vocab_id) DO UPDATE SET
                 quiz_correct = CASE WHEN quiz_correct > 0
                                     THEN quiz_correct - 1 ELSE 0 END",
            params![entry_id],
        )?;
        self.mastery_level(entry_id)
    }

    /// A writing run with mistakes removes one writing point (never below zero).
    pub fn record_write_wrong(&self, entry_id: &str) -> Result<u8> {
        self.conn.execute(
            "INSERT INTO mastery (vocab_id, quiz_correct, write_correct)
             VALUES (?1, 0, 0)
             ON CONFLICT(vocab_id) DO UPDATE SET
                 write_correct = CASE WHEN write_correct > 0
                                      THEN write_correct - 1 ELSE 0 END",
            params![entry_id],
        )?;
        self.mastery_level(entry_id)
    }

    pub fn group_entries(&self, group_id: &str) -> Result<Vec<VocabEntry>> {
        let mut statement = self.conn.prepare(
            r#"
            SELECT v.id, v.simplified, v.traditional, v.pinyin, v.meaning, v.hsk
            FROM (
                SELECT vocab_id, position FROM group_entries WHERE group_id = ?1
                UNION ALL
                SELECT vocab_id, position FROM custom_group_entries WHERE group_id = ?1
            ) ge
            JOIN vocab v ON v.id = ge.vocab_id
            ORDER BY ge.position
            "#,
        )?;

        let rows = statement.query_map(params![group_id], |row| {
            Ok(VocabEntry {
                id: row.get(0)?,
                simplified: row.get(1)?,
                traditional: row.get(2)?,
                pinyin: row.get(3)?,
                meaning: row.get(4)?,
                hsk: row.get(5)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn create_custom_group(&self, name: &str) -> Result<String> {
        let id = format!(
            "custom-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        );
        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM custom_groups",
            [],
            |row| row.get(0),
        )?;
        self.conn.execute(
            "INSERT INTO custom_groups (id, name, position) VALUES (?1, ?2, ?3)",
            params![id, name, position],
        )?;
        Ok(id)
    }

    /// Append an entry to a user group. Returns false when it is already there.
    pub fn append_to_custom_group(&self, group_id: &str, vocab_id: &str) -> Result<bool> {
        let exists: Option<i64> = self
            .conn
            .query_row(
                "SELECT 1 FROM custom_groups WHERE id = ?1",
                params![group_id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_none() {
            anyhow::bail!("unknown custom group: {group_id}");
        }

        let position: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(position), 0) + 1 FROM custom_group_entries WHERE group_id = ?1",
            params![group_id],
            |row| row.get(0),
        )?;
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO custom_group_entries (group_id, vocab_id, position)
             VALUES (?1, ?2, ?3)",
            params![group_id, vocab_id, position],
        )?;
        Ok(inserted > 0)
    }

    /// Delete a user group and its membership rows. Returns false if missing.
    pub fn delete_custom_group(&self, group_id: &str) -> Result<bool> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM custom_group_entries WHERE group_id = ?1",
            params![group_id],
        )?;
        let removed = tx.execute("DELETE FROM custom_groups WHERE id = ?1", params![group_id])?;
        tx.commit()?;
        Ok(removed > 0)
    }

    pub fn meanings(&self) -> Result<Vec<String>> {
        let mut statement = self
            .conn
            .prepare("SELECT DISTINCT meaning FROM vocab ORDER BY meaning")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;

        let mut meanings = Vec::new();
        for row in rows {
            meanings.push(row?);
        }
        Ok(meanings)
    }

    pub fn search_entries(&self, query: &str, limit: usize) -> Result<Vec<VocabEntry>> {
        let raw = query.trim().to_lowercase();
        if raw.is_empty() {
            return Ok(Vec::new());
        }
        // Pinyin is stored with tone numbers and spaces ("ni3 hao3"). Build a
        // compact form so "ni hao", "nihao" and "ni3hao3" all match.
        let compact: String = raw
            .chars()
            .filter(|c| c.is_ascii_alphabetic() || *c == 'ü')
            .map(|c| if c == 'ü' { 'v' } else { c })
            .collect();

        let allow_meaning = raw.chars().count() >= 3;
        let compact_match = compact.chars().count() >= 3;
        let mut statement = self.conn.prepare(
            r#"
            WITH searchable AS (
                SELECT id, simplified, traditional, pinyin, meaning, hsk,
                       lower(simplified) AS simp_l,
                       lower(traditional) AS trad_l,
                       lower(pinyin) AS pinyin_l,
                       lower(meaning) AS meaning_l,
                       lower(replace(replace(replace(replace(replace(replace(pinyin,
                           '1',''),'2',''),'3',''),'4',''),'5',''), ' ', '')) AS pinyin_compact
                FROM vocab
            )
            SELECT id, simplified, traditional, pinyin, meaning, hsk
            FROM searchable
            WHERE instr(simp_l, ?1) > 0
               OR instr(trad_l, ?1) > 0
               OR (?4 = 1 AND instr(meaning_l, ?1) > 0)
               OR EXISTS (
                   SELECT 1 FROM syllables sy
                   WHERE sy.vocab_id = searchable.id
                     AND (sy.key = ?1 OR sy.key LIKE ?1 || '%')
               )
               OR (?5 = 1 AND instr(pinyin_compact, ?2) > 0)
            ORDER BY
                CASE
                    WHEN simp_l = ?1 THEN 0
                    WHEN pinyin_l = ?1 THEN 1
                    WHEN instr(simp_l, ?1) = 1 THEN 2
                    WHEN instr(pinyin_l, ?1) = 1 THEN 3
                    WHEN ?5 = 1 AND instr(pinyin_compact, ?2) = 1 THEN 4
                    ELSE 5
                END,
                hsk,
                length(simplified),
                simplified
            LIMIT ?3
            "#,
        )?;

        let rows = statement.query_map(
            params![
                raw,
                compact,
                limit as i64,
                allow_meaning as i64,
                compact_match as i64
            ],
            |row| {
                Ok(VocabEntry {
                    id: row.get(0)?,
                    simplified: row.get(1)?,
                    traditional: row.get(2)?,
                    pinyin: row.get(3)?,
                    meaning: row.get(4)?,
                    hsk: row.get(5)?,
                })
            },
        )?;

        let mut found = Vec::new();
        for row in rows {
            found.push(row?);
        }
        Ok(found)
    }
}

fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(dir);
    }

    let home = std::env::var_os("HOME").unwrap_or_default();
    PathBuf::from(home).join(".local").join("share")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocab_file_is_consistent() {
        let raw = include_str!("../data/vocab.json");
        let file: VocabFile = serde_json::from_str(raw).expect("valid vocab.json");

        assert!(file.entries.len() > 2000);
        assert!(!file.groups.is_empty());

        let ids: HashSet<&str> = file.entries.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids.len(), file.entries.len(), "duplicate entry ids");

        for group in &file.groups {
            assert!(!group.entries.is_empty(), "empty group {}", group.id);
            for entry_id in &group.entries {
                assert!(
                    ids.contains(entry_id.as_str()),
                    "group {} references unknown {}",
                    group.id,
                    entry_id
                );
            }
        }

        for entry in &file.entries {
            assert!(
                !entry.meaning.trim().is_empty(),
                "empty meaning: {}",
                entry.id
            );
        }
    }

    #[test]
    fn custom_groups_roundtrip() {
        let base = std::env::temp_dir().join(format!("hanzi-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        std::env::set_var("XDG_DATA_HOME", &base);

        let db = Db::open().expect("open test database");
        let group_id = db.create_custom_group("Test group").expect("create group");

        let summary = db
            .groups()
            .expect("groups")
            .into_iter()
            .find(|group| group.id == group_id)
            .expect("custom group appears");
        assert!(summary.custom);
        assert_eq!(summary.count, 0);

        let vocab_id = {
            let raw = include_str!("../data/vocab.json");
            let file: VocabFile = serde_json::from_str(raw).expect("valid vocab.json");
            file.entries[0].id.clone()
        };

        assert!(db
            .append_to_custom_group(&group_id, &vocab_id)
            .expect("append"));
        assert!(!db
            .append_to_custom_group(&group_id, &vocab_id)
            .expect("append twice"));

        let entries = db.group_entries(&group_id).expect("group entries");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, vocab_id);

        let ai = db.search_entries("ai", 20).expect("pinyin search");
        assert!(ai.iter().any(|entry| entry.simplified == "爱"));
        assert!(!ai.iter().any(|entry| entry.simplified == "买"));
        assert!(db
            .search_entries("爱", 20)
            .expect("character search")
            .iter()
            .any(|entry| entry.simplified == "爱"));
        assert!(db
            .search_entries("love", 20)
            .expect("meaning search")
            .iter()
            .any(|entry| entry.meaning.contains("love")));

        let summary = db
            .groups()
            .expect("groups")
            .into_iter()
            .find(|group| group.id == group_id)
            .expect("custom group appears");
        assert_eq!(summary.count, 1);
        assert_eq!(summary.mastery, 0);

        assert_eq!(db.record_quiz_correct(&vocab_id).expect("quiz"), 0);
        assert_eq!(db.record_write_correct(&vocab_id).expect("write"), 1);
        assert_eq!(db.mastery_level(&vocab_id).expect("level"), 1);
        assert_eq!(db.group_mastery(&group_id).expect("group level"), 1);

        assert_eq!(db.record_quiz_wrong(&vocab_id).expect("quiz wrong"), 0);
        assert_eq!(db.record_quiz_wrong(&vocab_id).expect("quiz floor"), 0);
        assert_eq!(db.record_quiz_correct(&vocab_id).expect("quiz again"), 1);
        assert_eq!(db.record_write_wrong(&vocab_id).expect("write wrong"), 0);
        assert_eq!(db.mastery_level(&vocab_id).expect("level after"), 0);

        assert!(db.delete_custom_group(&group_id).expect("delete"));
        assert!(db
            .groups()
            .expect("groups")
            .into_iter()
            .all(|group| group.id != group_id));
        assert!(db
            .group_entries(&group_id)
            .expect("entries after delete")
            .is_empty());

        let _ = fs::remove_dir_all(&base);
    }
}
