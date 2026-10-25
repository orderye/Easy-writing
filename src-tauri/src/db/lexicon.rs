use rusqlite::{params, Connection, Result};
use shared_types::LexiconEntry;
use uuid::Uuid;
use std::collections::{HashMap, HashSet};

pub fn list_lexicon(conn: &Connection, novel_id: &str) -> Result<Vec<LexiconEntry>> {
    let mut stmt = conn.prepare(
        "SELECT id, novel_id, term, category, frequency, source_id, created_at, updated_at
         FROM novel_lexicon
         WHERE novel_id = ?1
         ORDER BY frequency DESC, length(term) DESC, term ASC",
    )?;
    let rows = stmt.query_map(params![novel_id], |row| {
        Ok(LexiconEntry {
            id: row.get(0)?,
            novel_id: row.get(1)?,
            term: row.get(2)?,
            category: row.get(3)?,
            frequency: row.get(4)?,
            source_id: row.get(5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
        })
    })?;

    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

pub fn upsert_lexicon_term(
    conn: &Connection,
    novel_id: &str,
    term: &str,
    category: &str,
    source_id: Option<&str>,
) -> Result<LexiconEntry> {
    let trimmed = term.trim();
    if trimmed.is_empty() {
        return Err(rusqlite::Error::InvalidQuery);
    }

    // Check if already exists
    let existing: Result<(String, i32, String), _> = conn.query_row(
        "SELECT id, frequency, created_at FROM novel_lexicon WHERE novel_id = ?1 AND term = ?2",
        params![novel_id, trimmed],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );

    match existing {
        Ok((id, freq, created_at)) => {
            conn.execute(
                "UPDATE novel_lexicon 
                 SET category = ?1, source_id = coalesce(?2, source_id), updated_at = datetime('now')
                 WHERE id = ?3",
                params![category, source_id, id],
            )?;
            Ok(LexiconEntry {
                id,
                novel_id: novel_id.to_string(),
                term: trimmed.to_string(),
                category: category.to_string(),
                frequency: freq,
                source_id: source_id.map(|s| s.to_string()),
                created_at,
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
        }
        Err(_) => {
            let id = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO novel_lexicon (id, novel_id, term, category, frequency, source_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?6)",
                params![id, novel_id, trimmed, category, source_id, now],
            )?;
            Ok(LexiconEntry {
                id,
                novel_id: novel_id.to_string(),
                term: trimmed.to_string(),
                category: category.to_string(),
                frequency: 1,
                source_id: source_id.map(|s| s.to_string()),
                created_at: now.clone(),
                updated_at: now,
            })
        }
    }
}

pub fn record_term_usage(conn: &Connection, novel_id: &str, term: &str) -> Result<()> {
    let trimmed = term.trim();
    conn.execute(
        "UPDATE novel_lexicon 
         SET frequency = frequency + 1, updated_at = datetime('now')
         WHERE novel_id = ?1 AND term = ?2",
        params![novel_id, trimmed],
    )?;
    Ok(())
}

pub fn delete_lexicon_term(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM novel_lexicon WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn clear_lexicon_category(conn: &Connection, novel_id: &str, category: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM novel_lexicon WHERE novel_id = ?1 AND category = ?2",
        params![novel_id, category],
    )?;
    Ok(())
}

/// Automatically synchronizes terms from Characters, Worldview entries, and Plot points
pub fn sync_lexicon_from_settings(conn: &Connection, novel_id: &str) -> Result<usize> {
    let mut added_count = 0;

    // 1. Sync from Worldview entries (功法、地名、宗派、世界观设定等)
    let mut stmt = conn.prepare(
        "SELECT id, title FROM worldview_entry WHERE novel_id = ?1 AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![novel_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for r in rows {
        if let Ok((id, title)) = r {
            let t = title.trim();
            if t.chars().count() >= 2 && t.chars().count() <= 30 {
                let _ = upsert_lexicon_term(conn, novel_id, t, "worldview", Some(&id));
                added_count += 1;
            }
        }
    }

    // 2. Sync from Characters (角色姓名、别名/绰号)
    let mut stmt = conn.prepare(
        "SELECT id, name, aliases, profile FROM character WHERE novel_id = ?1 AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![novel_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    for r in rows {
        if let Ok((id, name, aliases_json, profile_json)) = r {
            let name_trim = name.trim();
            if name_trim.chars().count() >= 2 && name_trim.chars().count() <= 20 {
                let _ = upsert_lexicon_term(conn, novel_id, name_trim, "character", Some(&id));
                added_count += 1;
            }

            // Parse aliases JSON array
            if let Ok(aliases) = serde_json::from_str::<Vec<String>>(&aliases_json) {
                for alias in aliases {
                    let a_trim = alias.trim();
                    if a_trim.chars().count() >= 2 && a_trim.chars().count() <= 20 {
                        let _ = upsert_lexicon_term(conn, novel_id, a_trim, "character", Some(&id));
                        added_count += 1;
                    }
                }
            }

            // Check profile for assets / kungfu / weapons
            if let Ok(profile_val) = serde_json::from_str::<serde_json::Value>(&profile_json) {
                if let Some(assets) = profile_val.get("assets").and_then(|v| v.as_array()) {
                    for item in assets {
                        if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                            let item_name = n.trim();
                            if item_name.chars().count() >= 2 && item_name.chars().count() <= 30 {
                                let _ = upsert_lexicon_term(conn, novel_id, item_name, "character", Some(&id));
                                added_count += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Sync from Plot Items
    let mut stmt = conn.prepare(
        "SELECT id, title FROM plot_item WHERE novel_id = ?1 AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![novel_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for r in rows {
        if let Ok((id, title)) = r {
            let t = title.trim();
            if t.chars().count() >= 2 && t.chars().count() <= 25 {
                let _ = upsert_lexicon_term(conn, novel_id, t, "plot", Some(&id));
                added_count += 1;
            }
        }
    }

    Ok(added_count)
}

/// Scans chapter text and extracts frequent novel terms (2~8 Chinese characters)
pub fn scan_and_extract_corpus_terms(conn: &Connection, novel_id: &str) -> Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT content_plain FROM chapter WHERE novel_id = ?1 AND deleted_at IS NULL",
    )?;
    let rows = stmt.query_map(params![novel_id], |row| row.get::<_, String>(0))?;

    // Collect all text
    let mut all_text = String::new();
    for r in rows {
        if let Ok(text) = r {
            all_text.push_str(&text);
            all_text.push('\n');
        }
    }

    if all_text.is_empty() {
        return Ok(0);
    }

    // Common Chinese stopwords/connectives that should NOT be in lexicon
    let stop_words: HashSet<&'static str> = [
        "这个", "那个", "什么", "怎么", "如何", "为什么", "虽然", "但是", "因为", "所以",
        "如果", "不过", "而且", "然后", "就是", "还是", "没有", "不是", "可以", "可能",
        "已经", "现在", "这个时候", "突然之间", "与此同时", "不知不觉", "一时之间",
        "微微一笑", "摇了摇头", "点了点头", "深吸一口气", "倒吸一口凉气", "只见那",
        "自己", "他们", "我们", "你们", "大家", "所有人", "这一切", "无论如何",
    ].into_iter().collect();

    // Extract Chinese phrases between 2 and 6 characters
    // Clean text by punctuation
    let mut freq_map: HashMap<String, i32> = HashMap::new();
    
    // Split text into chunks by punctuation / whitespace
    let delimiters = [' ', '\t', '\n', '\r', '，', '。', '！', '？', '：', '；', '“', '”', '‘', '’', '（', '）', '《', '》', '【', '】', '…', '—', '、'];
    let chunks: Vec<&str> = all_text.split(|c| delimiters.contains(&c)).collect();

    for chunk in chunks {
        let chars: Vec<char> = chunk.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        let len = chars.len();
        if len < 2 {
            continue;
        }

        // Check lengths 3, 4, 5, 6
        for n in [4, 3, 5, 2] {
            if len >= n {
                for window in chars.windows(n) {
                    let s: String = window.iter().collect();
                    // Must be all Chinese
                    if s.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)) {
                        if !stop_words.contains(s.as_str()) {
                            *freq_map.entry(s).or_insert(0) += 1;
                        }
                    }
                }
            }
        }
    }

    // Filter phrases that appear at least 3 times (or 4 times for 2-char)
    let mut extracted = 0;
    for (phrase, count) in freq_map {
        let char_count = phrase.chars().count();
        let min_count = if char_count == 2 { 4 } else { 3 };
        if count >= min_count {
            // Check if term already exists
            let _ = upsert_lexicon_term(conn, novel_id, &phrase, "corpus", None);
            let _ = conn.execute(
                "UPDATE novel_lexicon SET frequency = ?1 WHERE novel_id = ?2 AND term = ?3 AND category = 'corpus'",
                params![count, novel_id, phrase],
            );
            extracted += 1;
        }
    }

    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS novel (
                id          TEXT PRIMARY KEY,
                title       TEXT NOT NULL DEFAULT '未命名作品',
                synopsis    TEXT NOT NULL DEFAULT '',
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now')),
                deleted_at  TEXT
            );
            CREATE TABLE IF NOT EXISTS worldview_entry (
                id                  TEXT PRIMARY KEY,
                novel_id            TEXT NOT NULL,
                group_name          TEXT NOT NULL DEFAULT '',
                title               TEXT NOT NULL,
                content             TEXT NOT NULL DEFAULT '',
                origin              TEXT NOT NULL DEFAULT 'manual',
                source_chapter_id   TEXT,
                sort_order          INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
                deleted_at          TEXT
            );
            CREATE TABLE IF NOT EXISTS character (
                id                  TEXT PRIMARY KEY,
                novel_id            TEXT NOT NULL,
                name                TEXT NOT NULL,
                aliases             TEXT NOT NULL DEFAULT '[]',
                profile             TEXT NOT NULL DEFAULT '{}',
                origin              TEXT NOT NULL DEFAULT 'manual',
                source_chapter_id   TEXT,
                sort_order          INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
                deleted_at          TEXT
            );
            CREATE TABLE IF NOT EXISTS plot_item (
                id                  TEXT PRIMARY KEY,
                novel_id            TEXT NOT NULL,
                title               TEXT NOT NULL,
                content             TEXT NOT NULL DEFAULT '',
                kind                TEXT NOT NULL DEFAULT 'mainline',
                status              TEXT NOT NULL DEFAULT 'planted',
                chapter_id          TEXT,
                resolved_chapter_id TEXT,
                origin              TEXT NOT NULL DEFAULT 'manual',
                sort_order          INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
                deleted_at          TEXT
            );
            CREATE TABLE IF NOT EXISTS chapter (
                id                  TEXT PRIMARY KEY,
                novel_id            TEXT NOT NULL,
                volume_id           TEXT NOT NULL DEFAULT '',
                title               TEXT NOT NULL DEFAULT '',
                summary             TEXT NOT NULL DEFAULT '',
                content             TEXT NOT NULL DEFAULT '{}',
                content_plain       TEXT NOT NULL DEFAULT '',
                word_count          INTEGER NOT NULL DEFAULT 0,
                status              TEXT NOT NULL DEFAULT 'draft',
                extracted_hash      TEXT,
                sort_order          INTEGER NOT NULL DEFAULT 0,
                created_at          TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at          TEXT NOT NULL DEFAULT (datetime('now')),
                deleted_at          TEXT
            );
            CREATE TABLE IF NOT EXISTS novel_lexicon (
                id          TEXT PRIMARY KEY,
                novel_id    TEXT NOT NULL,
                term        TEXT NOT NULL,
                category    TEXT NOT NULL DEFAULT 'custom',
                frequency   INTEGER NOT NULL DEFAULT 1,
                source_id   TEXT,
                created_at  TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_lexicon_novel_term ON novel_lexicon(novel_id, term);"
        ).unwrap();
        conn
    }

    #[test]
    fn test_lexicon_sync_and_ranking() {
        let conn = setup_test_db();
        let novel_id = "test-novel-1";

        // Insert Worldview: 降龙十八掌
        conn.execute(
            "INSERT INTO worldview_entry (id, novel_id, title) VALUES ('wv1', ?1, '降龙十八掌')",
            params![novel_id],
        ).unwrap();

        // Insert Character: 萧峰, 别名 乔帮主
        conn.execute(
            "INSERT INTO character (id, novel_id, name, aliases) VALUES ('ch1', ?1, '萧峰', '[\"乔帮主\"]')",
            params![novel_id],
        ).unwrap();

        let count = sync_lexicon_from_settings(&conn, novel_id).unwrap();
        assert!(count >= 3);

        let list = list_lexicon(&conn, novel_id).unwrap();
        let terms: Vec<String> = list.iter().map(|e| e.term.clone()).collect();
        assert!(terms.contains(&"降龙十八掌".to_string()));
        assert!(terms.contains(&"萧峰".to_string()));
        assert!(terms.contains(&"乔帮主".to_string()));

        // Increase frequency
        record_term_usage(&conn, novel_id, "降龙十八掌").unwrap();
        let updated_list = list_lexicon(&conn, novel_id).unwrap();
        assert_eq!(updated_list[0].term, "降龙十八掌");
        assert_eq!(updated_list[0].frequency, 2);
    }
}

