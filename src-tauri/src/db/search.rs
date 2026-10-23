use rusqlite::Connection;

pub use shared_types::ChapterSearchHit;

/// 每章最多返回几条片段。
const MAX_SNIPPETS: usize = 3;

/// trigram 分词器要求查询串至少 3 个字符。
///
/// 这是 FTS5 trigram 的固有限制，不是本实现的取舍：trigram 按三字符滑窗建索引，
/// 「沈砚」这种两字词根本不在索引里，MATCH 一定返回空。因此短查询必须回退 LIKE。
pub const MIN_FTS_CHARS: usize = 3;

/// 全书搜索。
///
/// 两条路径：
///   - **≥3 字符**：走 `chapter_fts`（trigram 索引），按 `rank` 排序；
///   - **<3 字符**：trigram 索引里不存在这么短的 token，回退到 LIKE 全表扫。
///     章节量级下可接受，且结果与 FTS 路径同构。
pub fn search_chapters(
    conn: &Connection,
    novel_id: &str,
    query: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ChapterSearchHit>> {
    let term = query.trim();
    if term.is_empty() {
        return Ok(Vec::new());
    }

    if term.chars().count() >= MIN_FTS_CHARS {
        search_via_fts(conn, novel_id, term, limit)
    } else {
        search_via_like(conn, novel_id, term, limit)
    }
}

/// 把用户输入包成 FTS5 的短语查询。
///
/// 必须包：不包的话，用户输入 `AND` / `OR` / `*` / `-` / `NOT` 会被当成查询语法，
/// 轻则结果全空，重则直接报 `fts5: syntax error`。包成 `"…"` 后一切按字面处理，
/// 引号本身按 FTS5 规则用两个引号转义。
fn fts_phrase(term: &str) -> String {
    let escaped = term.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

fn search_via_fts(
    conn: &Connection,
    novel_id: &str,
    term: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ChapterSearchHit>> {
    let mut stmt = conn.prepare(
        "SELECT f.chapter_id, c.volume_id, c.title, c.content_plain
         FROM chapter_fts f
         JOIN chapter c ON c.id = f.chapter_id
         WHERE chapter_fts MATCH ?1 AND f.novel_id = ?2 AND c.deleted_at IS NULL
         ORDER BY rank
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(
        rusqlite::params![fts_phrase(term), novel_id, limit as i64],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        },
    )?;

    let mut out = Vec::new();
    for row in rows {
        let (chapter_id, volume_id, title, plain) = row?;
        out.push(ChapterSearchHit {
            hits: count_hits(&plain, &title, term),
            snippets: snippets_around(&plain, term),
            has_body: !plain.trim().is_empty(),
            chapter_id,
            volume_id,
            title,
        });
    }
    Ok(out)
}

fn search_via_like(
    conn: &Connection,
    novel_id: &str,
    term: &str,
    limit: usize,
) -> rusqlite::Result<Vec<ChapterSearchHit>> {
    let mut stmt = conn.prepare(
        "SELECT id, volume_id, title, content_plain
         FROM chapter
         WHERE novel_id = ?1 AND deleted_at IS NULL
           AND (content_plain LIKE '%' || ?2 || '%' OR title LIKE '%' || ?2 || '%')
         ORDER BY sort_order ASC, created_at ASC
         LIMIT ?3",
    )?;

    let rows = stmt.query_map(rusqlite::params![novel_id, term, limit as i64], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;

    let mut out = Vec::new();
    for row in rows {
        let (chapter_id, volume_id, title, plain) = row?;
        out.push(ChapterSearchHit {
            chapter_id,
            volume_id,
            title: title.clone(),
            hits: count_hits(&plain, &title, term),
            snippets: snippets_around(&plain, term),
            has_body: !plain.trim().is_empty(),
        });
    }
    Ok(out)
}

/// 正文 + 标题里的命中总次数。
fn count_hits(plain: &str, title: &str, term: &str) -> i64 {
    (plain.matches(term).count() + title.matches(term).count()) as i64
}

/// 取前 `MAX_SNIPPETS` 处命中的上下文片段。
fn snippets_around(text: &str, term: &str) -> Vec<String> {
    if term.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0usize;
    while out.len() < MAX_SNIPPETS {
        let Some(rel) = text[from..].find(term) else {
            break;
        };
        let pos = from + rel;
        out.push(crate::db::chapter::snippet_around_at(text, pos, term, 30));
        from = pos + term.len();
        if from >= text.len() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_query_escapes_double_quotes() {
        assert_eq!(fts_phrase("沈砚"), "\"沈砚\"");
        assert_eq!(fts_phrase("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn short_query_detection_matches_char_count_not_bytes() {
        // 「沈砚」两字六字节：按字节判断会误判成 ≥3 而走 FTS，结果永远为空。
        assert_eq!("沈砚".chars().count(), 2);
        assert_eq!("沈砚之".chars().count(), 3);
    }
}
